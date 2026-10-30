use converyn_runtime::artifacts::{artifact::hash_file, managed};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

fn retry_busy<T>(mut operation: impl FnMut() -> anyhow::Result<T>) -> T {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match operation() {
            Ok(value) => return value,
            Err(error) => {
                // A parallel test spawning Git can briefly inherit a flock FD
                // before exec. The upload API explicitly permits retrying busy.
                let busy = error.chain().any(|cause| {
                    cause
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::WouldBlock)
                });
                assert!(busy && std::time::Instant::now() < deadline, "{error:#}");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

#[test]
fn chunk_retries_restart_and_immutable_publication_verify_real_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("blobs");
    let bytes = (0..managed::CHUNK * 2 + 31)
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>();
    let sha = hex::encode(Sha256::digest(&bytes));
    let size = bytes.len() as u64;
    managed::directory(&root).unwrap();
    fs::write(root.join(format!("{sha}.part")), &bytes[..17]).unwrap();
    assert!(
        managed::put_chunk(
            &root,
            &sha,
            size,
            managed::CHUNK as u64,
            &bytes[..managed::CHUNK]
        )
        .is_err()
    );
    for (index, chunk) in bytes.chunks(managed::CHUNK).enumerate() {
        let offset = (index * managed::CHUNK) as u64;
        retry_busy(|| managed::put_chunk(&root, &sha, size, offset, chunk));
        retry_busy(|| managed::put_chunk(&root, &sha, size, offset, chunk));
    }
    retry_busy(|| managed::finish(&root, &sha, size));
    retry_busy(|| managed::finish(&root, &sha, size));
    assert_eq!(
        hash_file(&managed::path(&root, &sha, size).unwrap()).unwrap(),
        (sha.clone(), size)
    );
    assert_eq!(
        fs::metadata(root.join(&sha)).unwrap().permissions().mode() & 0o777,
        0o600
    );
    retry_busy(|| managed::put_chunk(&root, &sha, size, 0, &bytes[..managed::CHUNK]));
    assert!(managed::put_chunk(&root, &sha, size, 0, &vec![255; managed::CHUNK]).is_err());
    assert!(!root.join(format!("{sha}.part")).exists());
}
#[test]
fn incomplete_corrupt_or_symlinked_artifacts_never_publish() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("blobs");
    let sha = "a".repeat(64);
    managed::put_chunk(&root, &sha, 3, 0, b"bad").unwrap();
    assert!(managed::finish(&root, &sha, 3).is_err());
    assert!(!root.join(&sha).exists());
    let another = "b".repeat(64);
    let external = temp.path().join("external");
    fs::write(&external, b"keep").unwrap();
    symlink(&external, root.join(format!("{another}.part"))).unwrap();
    assert!(managed::put_chunk(&root, &another, 4, 0, b"xxxx").is_err());
    assert_eq!(fs::read(external).unwrap(), b"keep");
}
#[test]
fn local_payloads_are_hashed_without_embedding_bytes_or_host_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("payload"), b"large binary payload fixture").unwrap();
    let file = root.join("workflow.yaml");
    let value = serde_json::json!({"uid":"payload","name":"Payload","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"playbook":{"api_version":"converyn/v1","name":"payload","steps":[{"id":"install","phase":"rollout","scope":"per_node","operation":{"kind":"binary","path":"/opt/app/tool","mode":493,"artifact":{"$artifact":"payload"}}}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}});
    fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
    let packaged = converyn_runtime::orchestration::workflows::package_artifacts(&file).unwrap();
    let wire = serde_json::to_string(&packaged.revision).unwrap();
    assert!(!wire.contains("large binary payload fixture"));
    assert!(!wire.contains(root.to_str().unwrap()));
    assert_eq!(packaged.artifacts.len(), 1);
    let artifact = packaged.revision.playbook.steps[0]
        .operation
        .artifact()
        .unwrap();
    assert!(artifact.locations.is_empty());
    assert_eq!(artifact.size, 28);
    assert_eq!(artifact.sha256, hash_file(&root.join("payload")).unwrap().0);
    fs::remove_file(root.join("payload")).unwrap();
    symlink("/etc/passwd", root.join("payload")).unwrap();
    assert!(converyn_runtime::orchestration::workflows::package(&file).is_err());
}

#[tokio::test]
async fn committed_artifact_metadata_survives_sqlite_restart_and_logical_backup() {
    use converyn_core::orchestration::{administration::Change, machine::Command};
    use converyn_runtime::storage::{backup, store::Store};
    use openraft::{CommittedLeaderId, Entry, EntryPayload, LogId, storage::RaftStateMachine};
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("controller");
    let sha = "d".repeat(64);
    let mut store = Store::open(&data, 1).unwrap();
    store.bind_cluster("artifacts").await.unwrap();
    let reply = store
        .apply([Entry {
            log_id: LogId::new(CommittedLeaderId::new(1, 1), 1),
            payload: EntryPayload::Normal(Command::Administer {
                change: Change::Artifact {
                    sha256: sha.clone(),
                    size: 123,
                },
            }),
        }])
        .await
        .unwrap();
    assert!(reply[0].ok);
    let (carrier, _) = store.backup_carrier().await.unwrap();
    drop(store);
    let reopened = Store::open(&data, 1).unwrap();
    assert_eq!(
        reopened
            .machine()
            .await
            .unwrap()
            .state
            .administration
            .artifacts[&sha],
        123
    );
    let restored_path = temp.path().join("restored");
    backup::restore(&carrier, &restored_path, 1, true)
        .await
        .unwrap();
    let restored = Store::open(&restored_path, 1).unwrap();
    assert_eq!(
        restored
            .machine()
            .await
            .unwrap()
            .state
            .administration
            .artifacts[&sha],
        123
    );
    assert!(!restored_path.join("artifacts").join(sha).exists());
}

#[test]
fn hashing_progress_reports_actual_monotonic_bytes_without_changing_digest() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("payload");
    let bytes = vec![13; 800_123];
    fs::write(&path, &bytes).unwrap();
    let mut progress = Vec::new();
    let actual =
        converyn_runtime::artifacts::artifact::hash_file_with_progress(&path, |read, total| {
            progress.push((read, total))
        })
        .unwrap();
    assert_eq!(
        actual,
        (hex::encode(Sha256::digest(&bytes)), bytes.len() as u64)
    );
    assert_eq!(progress.first(), Some(&(0, bytes.len() as u64)));
    assert_eq!(
        progress.last(),
        Some(&(bytes.len() as u64, bytes.len() as u64))
    );
    assert!(progress.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert!(
        progress
            .iter()
            .all(|(read, total)| read <= total && *total == bytes.len() as u64)
    );
}
