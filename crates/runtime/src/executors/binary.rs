//! Install an immutable artifact by streaming to a new inode on the target
//! filesystem, validating the staged bytes, then atomically publishing it.
use crate::executors::native::{Evidence, Policy, component, observe, parent_create};
use anyhow::{Result, ensure};
use converyn_core::model::domain::Operation;
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub fn reconcile(
    policy: &Policy,
    operation: &Operation,
    source: &Path,
    cancel: &AtomicBool,
) -> Result<Evidence> {
    operation.validate()?;
    let Operation::Binary {
        path,
        artifact,
        mode,
        version_directory,
    } = operation
    else {
        anyhow::bail!("not a binary operation")
    };
    let target = Path::new(path);
    ensure!(
        policy.allows_file(target),
        "binary path is outside configured roots"
    );
    if let Some(directory) = version_directory {
        return versioned(policy, operation, source, cancel, directory);
    }
    let (dir, name) = parent_create(target, policy)?;
    let before = observe(&dir, &name, converyn_core::model::domain::MAX_ARTIFACT_SIZE)?;
    let matches = |s: &crate::executors::native::FileState| {
        s.sha256 == artifact.sha256 && s.mode == *mode && s.bytes == artifact.size
    };
    if before.as_ref().is_some_and(matches) {
        return Ok(Evidence {
            operation: operation.clone(),
            plugin: None,

            before: before.clone(),
            after: before,
            changed: false,
            error: None,

            service_before: None,
            service_after: None,
            artifact_size: Some(artifact.size),
            package: None,

            reboot: None,
            health: None,
        });
    }
    ensure!(
        fs2::available_space(target.parent().unwrap())? >= artifact.size + 64 * 1024 * 1024,
        "insufficient target space for atomic binary installation"
    );
    let mut input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(source)?;
    ensure!(
        input.metadata()?.is_file() && input.metadata()?.len() == artifact.size,
        "cached artifact size/type mismatch"
    );
    let mut random = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let temporary = format!(".converyn-{}", hex::encode(random));
    let mut staged = component(
        &dir,
        &temporary,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    let tmp = CString::new(temporary)?;
    let dst = CString::new(name.clone())?;
    let result = (|| -> Result<()> {
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buf = [0u8; 256 * 1024];
        loop {
            ensure!(
                !cancel.load(Ordering::SeqCst),
                "binary installation canceled before publication"
            );
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            ensure!(bytes <= artifact.size, "artifact grew during installation");
            hash.update(&buf[..n]);
            staged.write_all(&buf[..n])?;
        }
        ensure!(
            bytes == artifact.size && hex::encode(hash.finalize()) == artifact.sha256,
            "artifact changed during installation"
        );
        staged.set_permissions(std::fs::Permissions::from_mode(*mode))?;
        staged.sync_all()?;
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "binary installation canceled before publication"
        );
        // SAFETY: 目录 fd 和名称 CString 在调用期间有效；暂存文件在同一受控目录原子发布。
        if unsafe { libc::renameat(dir.as_raw_fd(), tmp.as_ptr(), dir.as_raw_fd(), dst.as_ptr()) }
            != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        dir.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        // SAFETY: 仅清理本次操作在持有的目录 fd 下创建的暂存名称，CString 保持有效。
        unsafe { libc::unlinkat(dir.as_raw_fd(), tmp.as_ptr(), 0) };
    }
    result?;
    let after = observe(&dir, &name, converyn_core::model::domain::MAX_ARTIFACT_SIZE)?;
    ensure!(
        after.as_ref().is_some_and(matches),
        "installed binary failed verification"
    );
    Ok(Evidence {
        operation: operation.clone(),
        plugin: None,

        before,
        after,
        changed: true,
        error: None,
        service_before: None,
        service_after: None,
        artifact_size: Some(artifact.size),
        package: None,

        reboot: None,
        health: None,
    })
}

fn versioned(
    policy: &Policy,
    operation: &Operation,
    source: &Path,
    cancel: &AtomicBool,
    directory: &str,
) -> Result<Evidence> {
    let Operation::Binary {
        path,
        artifact,
        mode,
        ..
    } = operation
    else {
        unreachable!()
    };
    let version_path = format!("{directory}/{}-{:o}", artifact.sha256, mode);
    let immutable = Operation::Binary {
        path: version_path.clone(),
        artifact: artifact.clone(),
        mode: *mode,
        version_directory: None,
    };
    // The version directory has its own explicit policy and ownership keys.
    let staged = reconcile(policy, &immutable, source, cancel)?;
    let (dir, name) = parent_create(Path::new(path), policy)?;
    let (versions, version_name) = crate::executors::native::parent(Path::new(&version_path))?;
    ensure!(
        std::os::unix::fs::MetadataExt::dev(&dir.metadata()?)
            == std::os::unix::fs::MetadataExt::dev(&versions.metadata()?),
        "version directory and activation path must be on the same filesystem"
    );
    let before = observe(&dir, &name, converyn_core::model::domain::MAX_ARTIFACT_SIZE)?;
    let matches = |s: &crate::executors::native::FileState| {
        s.sha256 == artifact.sha256 && s.mode == *mode && s.bytes == artifact.size
    };
    let mut evidence = Evidence::empty(operation);
    evidence.before = before.clone();
    evidence.artifact_size = Some(artifact.size);
    if before.as_ref().is_some_and(matches) {
        evidence.after = before;
        evidence.changed = staged.changed;
        return Ok(evidence);
    }
    let mut random = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let tmp = CString::new(format!(".converyn-activate-{}", hex::encode(random)))?;
    let src = CString::new(version_name)?;
    let dst = CString::new(name.clone())?;
    ensure!(!cancel.load(Ordering::SeqCst), "activation canceled");
    // SAFETY: 目录 fd 与名称 CString 在调用期间有效；只链接已验证摘要的不可变版本文件。
    if unsafe {
        libc::linkat(
            versions.as_raw_fd(),
            src.as_ptr(),
            dir.as_raw_fd(),
            tmp.as_ptr(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let result = (|| -> Result<()> {
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "activation canceled before publication"
        );
        // SAFETY: 目录 fd 和名称 CString 在调用期间有效；暂存文件在同一受控目录原子发布。
        if unsafe { libc::renameat(dir.as_raw_fd(), tmp.as_ptr(), dir.as_raw_fd(), dst.as_ptr()) }
            != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        dir.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        // SAFETY: 仅清理本次操作在持有的目录 fd 下创建的暂存名称，CString 保持有效。
        unsafe { libc::unlinkat(dir.as_raw_fd(), tmp.as_ptr(), 0) };
    }
    result?;
    evidence.after = observe(&dir, &name, converyn_core::model::domain::MAX_ARTIFACT_SIZE)?;
    ensure!(
        evidence.after.as_ref().is_some_and(matches),
        "activated binary failed verification"
    );
    evidence.changed = true;
    Ok(evidence)
}
