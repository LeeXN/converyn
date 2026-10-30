//! Disk cache admission. Caller holds quota.lock across reservation and fetch.
//! Eviction skips any artifact whose per-digest lock is held by another fetch.
use anyhow::{Result, ensure};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
pub fn reserve(cache: &Path, wanted: &str, size: u64, limit: u64) -> Result<()> {
    ensure!(size <= limit, "artifact exceeds configured cache quota");
    let pins = pins(cache)?;
    let mut used = 0u64;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(cache)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let digest = name.strip_suffix(".part").unwrap_or(&name);
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            continue;
        }
        let meta = entry.path().symlink_metadata()?;
        ensure!(meta.is_file(), "cache contains a nonregular artifact");
        if digest == wanted {
            continue;
        }
        used = used
            .checked_add(meta.len())
            .ok_or_else(|| anyhow::anyhow!("cache size overflow"))?;
        candidates.push((
            meta.modified()?,
            entry.path(),
            digest.to_owned(),
            meta.len(),
        ));
    }
    candidates.sort_by_key(|c| c.0);
    for (_, path, digest, len) in candidates {
        if used <= limit - size {
            break;
        }
        if pins.contains_key(&digest) {
            continue;
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(cache.join(format!("{digest}.lock")))?;
        if lock.try_lock_exclusive().is_err() {
            continue;
        }
        fs::remove_file(path)?;
        used -= len;
    }
    File::open(cache)?.sync_all()?;
    ensure!(
        used <= limit - size,
        "cache quota exhausted by pinned or locked artifacts"
    );
    Ok(())
}

fn pins(cache: &Path) -> Result<std::collections::BTreeMap<String, u64>> {
    use std::io::Read;
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cache.join("pins.json"))
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "cache pin manifest exceeds limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn set_pins(
    cache: &Path,
    wanted: &std::collections::BTreeMap<String, u64>,
    limit: u64,
) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    ensure!(
        wanted.len() <= 4096
            && wanted.iter().all(|(sha, size)| sha.len() == 64
                && sha
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && *size <= converyn_core::model::domain::MAX_ARTIFACT_SIZE),
        "invalid cache pins"
    );
    ensure!(
        wanted.values().all(|size| *size <= limit),
        "artifact exceeds cache quota"
    );
    // Pins may reference an older successful version already absent from disk.
    // Reserve actual bytes at fetch time; never evict the retained bytes that do
    // exist just because a historical reference cannot be satisfied locally.
    fs::create_dir_all(cache)?;
    ensure!(
        cache.symlink_metadata()?.is_dir(),
        "cache must not be a symlink"
    );
    fs::set_permissions(cache, fs::Permissions::from_mode(0o700))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cache.join("quota.lock"))?;
    lock.try_lock_exclusive()?;
    if pins(cache)? == *wanted {
        return Ok(());
    }
    let temporary = cache.join("pins.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(wanted)?)?;
    file.sync_all()?;
    fs::rename(temporary, cache.join("pins.json"))?;
    File::open(cache)?.sync_all()?;
    Ok(())
}
