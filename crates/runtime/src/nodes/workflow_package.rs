//! Read-only, content-addressed dependencies for registered workflow scripts.
use anyhow::{Context, Result, ensure};
use converyn_core::{
    model::domain::digest,
    orchestration::inventory::{PackageFile, valid_package_path},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub fn location(work_root: &Path, files: &BTreeMap<String, PackageFile>) -> PathBuf {
    work_root.join(".packages").join(digest(files))
}
pub fn verify(root: &Path, files: &BTreeMap<String, PackageFile>) -> Result<()> {
    // SAFETY: geteuid has no pointer arguments or side effects.
    let owner = unsafe { libc::geteuid() };
    ensure!(
        fs::symlink_metadata(root)?.is_dir() && root.canonicalize()? == root,
        "unsafe workflow package root"
    );
    for path in [
        root,
        root.parent().context("package parent missing")?,
        root.parent()
            .and_then(Path::parent)
            .context("package work root missing")?,
    ] {
        let metadata = fs::metadata(path)?;
        ensure!(
            metadata.uid() == owner && metadata.permissions().mode() & 0o022 == 0,
            "unsafe workflow package ownership"
        );
    }
    fn entries(root: &Path, path: &Path, files: &BTreeMap<String, PackageFile>) -> Result<()> {
        ensure!(
            fs::metadata(path)?.permissions().mode() & 0o222 == 0,
            "workflow package directory became writable"
        );
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            ensure!(!kind.is_symlink(), "workflow package symlink");
            if kind.is_dir() {
                entries(root, &path, files)?;
            } else {
                ensure!(
                    kind.is_file()
                        && files.contains_key(
                            path.strip_prefix(root)?
                                .to_str()
                                .context("invalid package filename")?
                        ),
                    "unexpected workflow package entry"
                );
            }
        }
        Ok(())
    }
    entries(root, root, files)?;
    for (relative, entry) in files {
        ensure!(
            valid_package_path(relative),
            "invalid workflow package path"
        );
        let mut path = root.to_path_buf();
        for part in relative.split('/') {
            path.push(part);
            ensure!(
                !fs::symlink_metadata(&path)?.file_type().is_symlink(),
                "workflow package symlink"
            );
        }
        let metadata = path.metadata()?;
        ensure!(
            metadata.is_file()
                && metadata.uid() == owner
                && metadata.nlink() == 1
                && metadata.permissions().mode() & 0o222 == 0
                && (metadata.permissions().mode() & 0o111 != 0) == entry.executable
                && hex::encode(Sha256::digest(fs::read(&path)?)) == entry.sha256,
            "workflow package content or mode changed"
        );
    }
    Ok(())
}

pub fn install(
    work_root: &Path,
    files: &BTreeMap<String, PackageFile>,
    contents: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    ensure!(
        work_root.is_absolute(),
        "package work root must be absolute"
    );
    fs::create_dir_all(work_root)?;
    // SAFETY: geteuid has no pointer arguments or side effects.
    let owner = unsafe { libc::geteuid() };
    ensure!(
        work_root.canonicalize()? == work_root && fs::metadata(work_root)?.uid() == owner,
        "unsafe package work root"
    );
    fs::set_permissions(work_root, fs::Permissions::from_mode(0o711))?;
    let parent = work_root.join(".packages");
    fs::create_dir_all(&parent)?;
    ensure!(
        fs::symlink_metadata(&parent)?.is_dir()
            && parent.canonicalize()? == parent
            && fs::metadata(&parent)?.uid() == owner,
        "unsafe package cache"
    );
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755))?;
    let root = location(work_root, files);
    if root.exists() {
        verify(&root, files)?;
        return Ok(root);
    }
    let staging = parent.join(format!(
        ".{}-{}",
        std::process::id(),
        crate::nodes::agent::random_id()?
    ));
    fs::create_dir(&staging)?;
    let result = (|| -> Result<()> {
        for (relative, entry) in files {
            ensure!(
                valid_package_path(relative),
                "invalid workflow package path"
            );
            let content = contents
                .get(&entry.sha256)
                .context("workflow package definition missing")?;
            ensure!(
                content.len() <= 65536
                    && hex::encode(Sha256::digest(content.as_bytes())) == entry.sha256,
                "workflow package digest mismatch"
            );
            let path = staging.join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            file.set_permissions(fs::Permissions::from_mode(if entry.executable {
                0o555
            } else {
                0o444
            }))?;
        }
        fn readonly(path: &Path) -> Result<()> {
            for entry in fs::read_dir(path)? {
                let path = entry?.path();
                if path.is_dir() {
                    readonly(&path)?;
                }
            }
            fs::set_permissions(path, fs::Permissions::from_mode(0o555))?;
            fs::File::open(path)?.sync_all()?;
            Ok(())
        }
        readonly(&staging)?;
        fs::rename(&staging, &root)?;
        fs::File::open(&parent)?.sync_all()?;
        verify(&root, files)
    })();
    if result.is_err() && staging.exists() {
        fn unlock(path: &Path) {
            if path.is_dir() {
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.flatten() {
                        unlock(&entry.path());
                    }
                }
            }
        }
        unlock(&staging);
        let _ = fs::remove_dir_all(&staging);
    }
    result?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_are_isolated_and_modified_dependencies_fail_closed() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tasks");
        let text = "echo version-one\n";
        let hash = hex::encode(Sha256::digest(text));
        let files = BTreeMap::from([(
            "lib/helper.sh".into(),
            PackageFile {
                sha256: hash.clone(),
                executable: true,
            },
        )]);
        let contents = BTreeMap::from([(hash, text.into())]);
        let old = install(&root, &files, &contents).unwrap();
        verify(&old, &files).unwrap();
        let newer = "echo version-two\n";
        let sha = hex::encode(Sha256::digest(newer));
        let new_files = BTreeMap::from([(
            "lib/helper.sh".into(),
            PackageFile {
                sha256: sha.clone(),
                executable: true,
            },
        )]);
        let new = install(&root, &new_files, &BTreeMap::from([(sha, newer.into())])).unwrap();
        assert_ne!(old, new);
        verify(&old, &files).unwrap();
        let helper = old.join("lib/helper.sh");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&helper, "tampered").unwrap();
        assert!(verify(&old, &files).is_err());
        assert!(install(&root, &files, &contents).is_err());
        verify(&new, &new_files).unwrap();
    }
    #[test]
    fn escaping_paths_and_symbolic_links_are_not_materialized() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tasks");
        let text = "safe";
        let hash = hex::encode(Sha256::digest(text));
        let contents = BTreeMap::from([(hash.clone(), text.into())]);
        let files = BTreeMap::from([(
            "../escape".into(),
            PackageFile {
                sha256: hash.clone(),
                executable: false,
            },
        )]);
        assert!(install(&root, &files, &contents).is_err());
        assert!(!d.path().join("escape").exists());
        let files = BTreeMap::from([(
            "helper".into(),
            PackageFile {
                sha256: hash,
                executable: false,
            },
        )]);
        let path = install(&root, &files, &contents).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_file(path.join("helper")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", path.join("helper")).unwrap();
        assert!(verify(&path, &files).is_err());
    }
}
