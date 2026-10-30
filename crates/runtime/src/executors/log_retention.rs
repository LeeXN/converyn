//! Raw output may be reclaimed; execution identities and original receipts
//! survive retention. Unknown output needs a separate terminal acknowledgement.
use anyhow::{Result, ensure};
use fs2::FileExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::SystemTime,
};
const FILES: [&str; 4] = [
    "stdout.log",
    "stderr.log",
    "supervisor.stdout.log",
    "supervisor.stderr.log",
];
pub fn default_limit() -> u64 {
    256 * 1024 * 1024
}
struct Candidate {
    modified: SystemTime,
    path: PathBuf,
    lock: PathBuf,
    receipt: PathBuf,
    resolution: PathBuf,
    size: u64,
}
fn sha_name(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn add(
    path: PathBuf,
    lock: PathBuf,
    receipt: PathBuf,
    resolution: PathBuf,
    used: &mut u64,
    items: &mut Vec<Candidate>,
) -> Result<()> {
    let meta = match path.symlink_metadata() {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    ensure!(meta.is_file(), "journal log is not a regular file");
    *used = used
        .checked_add(meta.len())
        .ok_or_else(|| anyhow::anyhow!("raw log size overflow"))?;
    items.push(Candidate {
        modified: meta.modified()?,
        size: meta.len(),
        path,
        lock,
        receipt,
        resolution,
    });
    Ok(())
}
fn scan(root: &Path, flat: bool, used: &mut u64, items: &mut Vec<Candidate>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    ensure!(
        root.symlink_metadata()?.is_dir(),
        "journal is not a directory"
    );
    for item in fs::read_dir(root)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        if flat {
            if let Some(key) = name
                .strip_suffix(".stdout.log")
                .or_else(|| name.strip_suffix(".stderr.log"))
                && sha_name(key)
            {
                add(
                    item.path(),
                    root.join(format!("{key}.lock")),
                    root.join(format!("{key}.json")),
                    root.join(format!("{key}.log-resolution.json")),
                    used,
                    items,
                )?;
            }
        } else if item.file_type()?.is_dir() && sha_name(&name) {
            let directory = item.path();
            for file in FILES {
                add(
                    directory.join(file),
                    directory.join("lock"),
                    directory.join("receipt.json"),
                    directory.join("log-resolution.json"),
                    used,
                    items,
                )?;
            }
        }
    }
    Ok(())
}
fn read(path: &Path) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut bytes = Vec::new();
    file.take(128 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 128 * 1024,
        "journal exceeds retention metadata limit"
    );
    Ok(bytes)
}
fn prune(mut items: Vec<Candidate>, mut used: u64, limit: u64) -> Result<u64> {
    ensure!(
        limit >= 32 * 1024 * 1024,
        "raw log quota must be at least 32 MiB"
    );
    items.sort_by_key(|item| item.modified);
    for item in items {
        if used <= limit {
            break;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&item.lock)?;
        if lock.try_lock_exclusive().is_err() {
            continue;
        }
        let bytes = match read(&item.receipt) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let receipt = match serde_json::from_slice::<crate::executors::runner::Receipt>(&bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if matches!(receipt.outcome, crate::executors::runner::Outcome::Running) {
            continue;
        }
        if receipt.outcome == crate::executors::runner::Outcome::NeedsAttention {
            let marker = read(&item.resolution)
                .ok()
                .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok());
            if !marker.is_some_and(|m| {
                m["operation_id"] == receipt.operation_id
                    && m["receipt_sha256"] == hex::encode(Sha256::digest(&bytes))
                    && m["controller_terminal_ack"] == true
            }) {
                continue;
            }
        }
        fs::remove_file(&item.path)?;
        File::open(item.path.parent().unwrap())?.sync_all()?;
        used = used.saturating_sub(item.size);
    }
    ensure!(
        used <= limit,
        "raw log quota is occupied by active or uncertain operations"
    );
    Ok(used)
}
pub fn collect(root: &Path, limit: u64) -> Result<u64> {
    let mut used = 0;
    let mut items = Vec::new();
    scan(root, false, &mut used, &mut items)?;
    prune(items, used, limit)
}
pub fn collect_agent(state: &Path, limit: u64) -> Result<u64> {
    let mut used = 0;
    let mut items = Vec::new();
    scan(&state.join("executions"), false, &mut used, &mut items)?;
    scan(&state.join("native"), true, &mut used, &mut items)?;
    prune(items, used, limit)
}
/// Called only after the Agent observes/receives a committed terminal attempt.
/// Preserve an unknown receipt as-is, even after the server resolves its result.
pub fn mark_resolved(
    state: &Path,
    id: &str,
    native: bool,
    outbox: &crate::executors::runner::Receipt,
) -> Result<()> {
    let key = hex::encode(Sha256::digest(id));
    let root = state.join(if native { "native" } else { "executions" });
    let directory = if native {
        root.clone()
    } else {
        root.join(&key)
    };
    fs::create_dir_all(&directory)?;
    ensure!(
        directory.symlink_metadata()?.is_dir(),
        "journal must not be a symlink"
    );
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let receipt = if native {
        directory.join(format!("{key}.json"))
    } else {
        directory.join("receipt.json")
    };
    let lock = if native {
        directory.join(format!("{key}.lock"))
    } else {
        directory.join("lock")
    };
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock)?;
    lock.try_lock_exclusive()?;
    if !receipt.exists() {
        ensure!(
            outbox.operation_id == id
                && outbox.outcome == crate::executors::runner::Outcome::NeedsAttention,
            "only an unknown durable outbox can restore its execution tombstone"
        );
        crate::nodes::agent::durable(&receipt, &serde_json::to_vec(outbox)?)?;
    }
    let bytes = read(&receipt)?;
    let original: crate::executors::runner::Receipt = serde_json::from_slice(&bytes)?;
    ensure!(
        original.operation_id == id,
        "resolution journal identity mismatch"
    );
    let marker = if native {
        directory.join(format!("{key}.log-resolution.json"))
    } else {
        directory.join("log-resolution.json")
    };
    crate::nodes::agent::durable(
        &marker,
        &serde_json::to_vec(
            &json!({"operation_id":id,"receipt_sha256":hex::encode(Sha256::digest(bytes)),"controller_terminal_ack":true,"acknowledged_ms":crate::executors::host::now_ms()}),
        )?,
    )?;
    Ok(())
}
