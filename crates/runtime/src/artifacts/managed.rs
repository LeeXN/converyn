//! Immutable controller-local blobs. Only metadata enters Raft; uploads are bounded chunks.
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub const CHUNK: usize = 1024 * 1024;
pub const QUOTA: u64 = 64 * 1024 * 1024 * 1024;

pub fn validate(sha: &str, size: u64) -> Result<()> {
    ensure!(
        converyn_core::identities::registration::valid_sha(sha)
            && size > 0
            && size <= converyn_core::model::domain::MAX_ARTIFACT_SIZE,
        "invalid managed artifact digest or size"
    );
    Ok(())
}
fn open(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}
pub fn directory(root: &Path) -> Result<()> {
    fs::create_dir_all(root)?;
    ensure!(
        root.symlink_metadata()?.is_dir(),
        "artifact storage must be a directory"
    );
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    crate::credentials::security::private_state_directory(&root.canonicalize()?)?;
    Ok(())
}
pub fn path(root: &Path, sha: &str, size: u64) -> Result<PathBuf> {
    validate(sha, size)?;
    let path = root.join(sha);
    let metadata = path.symlink_metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() == size,
        "artifact is incomplete or not a regular file"
    );
    Ok(path)
}
fn used(root: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let metadata = entry.path().symlink_metadata()?;
        ensure!(
            metadata.is_file(),
            "artifact store contains a nonregular file"
        );
        total = total
            .checked_add(metadata.len())
            .context("artifact quota overflow")?;
    }
    Ok(total)
}
pub fn recovery_reservation(root: &Path, sha: &str, size: u64) -> Result<File> {
    validate(sha, size)?;
    directory(root)?;
    let lock = open(&root.join("upload.lock"))?;
    lock.try_lock_exclusive().context("artifact storage busy")?;
    let part = root.join(format!("{sha}.part"));
    let existing = match part.symlink_metadata() {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && metadata.len() <= size,
                "invalid artifact recovery partial"
            );
            metadata.len()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    let remaining = size - existing;
    ensure!(
        used(root)?
            .checked_add(remaining)
            .is_some_and(|n| n <= QUOTA),
        "controller artifact quota exhausted"
    );
    ensure!(
        fs2::available_space(root)? >= remaining + 64 * 1024 * 1024,
        "insufficient artifact recovery space"
    );
    Ok(lock)
}
pub fn put_chunk(root: &Path, sha: &str, size: u64, offset: u64, bytes: &[u8]) -> Result<()> {
    validate(sha, size)?;
    ensure!(
        !bytes.is_empty()
            && bytes.len() <= CHUNK
            && offset.is_multiple_of(CHUNK as u64)
            && offset
                .checked_add(bytes.len() as u64)
                .is_some_and(|end| end <= size)
            && (bytes.len() == CHUNK || offset + bytes.len() as u64 == size),
        "invalid artifact chunk"
    );
    directory(root)?;
    let lock = open(&root.join("upload.lock"))?;
    lock.try_lock_exclusive()
        .context("artifact upload busy; retry this chunk")?;
    let completed = root.join(sha);
    let immutable = completed.try_exists()?;
    let target = if immutable {
        completed
    } else {
        root.join(format!("{sha}.part"))
    };
    let mut file = open(&target)?;
    ensure!(
        file.metadata()?.is_file(),
        "artifact upload must be a regular file"
    );
    let current = file.metadata()?.len();
    ensure!(
        current <= size && current >= offset,
        "artifact upload has a gap or unexpected size"
    );
    if offset < current {
        if offset + bytes.len() as u64 <= current {
            file.seek(SeekFrom::Start(offset))?;
            let mut old = vec![0; bytes.len()];
            file.read_exact(&mut old)?;
            if old == bytes {
                return Ok(());
            }
            ensure!(
                !immutable && offset == 0,
                "immutable artifact chunk differs"
            );
        } else {
            ensure!(!immutable, "immutable artifact chunk differs");
        }
        // Only uncommitted bytes may be replaced after a killed partial write.
        file.set_len(offset)?;
    }
    ensure!(
        used(root)?
            .checked_add(bytes.len() as u64)
            .is_some_and(|n| n <= QUOTA),
        "controller artifact quota exhausted"
    );
    ensure!(
        fs2::available_space(root)? >= bytes.len() as u64 + 64 * 1024 * 1024,
        "insufficient artifact storage space"
    );
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    File::open(root)?.sync_all()?;
    Ok(())
}
pub fn finish(root: &Path, sha: &str, size: u64) -> Result<()> {
    validate(sha, size)?;
    directory(root)?;
    let lock = open(&root.join("upload.lock"))?;
    lock.try_lock_exclusive()
        .context("artifact upload busy; retry finalize")?;
    let final_path = root.join(sha);
    let part = root.join(format!("{sha}.part"));
    let source = if final_path.try_exists()? {
        &final_path
    } else {
        &part
    };
    let (actual, bytes) = crate::artifacts::artifact::hash_file(source)?;
    ensure!(
        actual == sha && bytes == size,
        "managed artifact checksum or size mismatch"
    );
    File::open(source)?.sync_all()?;
    if source == &part {
        fs::rename(&part, &final_path)?;
    }
    File::open(root)?.sync_all()?;
    Ok(())
}
