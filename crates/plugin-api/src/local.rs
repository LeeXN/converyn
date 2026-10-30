//! 原生插件共用的本地私有文件读取。这里不提供任何网络或编排状态访问。
use anyhow::{Result, ensure};
use std::{
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
/// 拒绝符号链接、不可信祖先及其他用户可读的凭据，限制读取大小。
pub fn secret(path: &Path) -> Result<String> {
    ensure!(path.is_absolute(), "credential path must be absolute");
    // SAFETY: geteuid 没有指针参数，也不修改进程身份。
    let uid = unsafe { libc::geteuid() };
    for p in path.parent().into_iter().flat_map(Path::ancestors) {
        let m = std::fs::symlink_metadata(p)?;
        ensure!(
            m.is_dir()
                && (m.uid() == 0 || m.uid() == uid)
                && (m.mode() & 0o022 == 0 || m.mode() & 0o1000 != 0),
            "untrusted credential ancestor"
        );
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && (m.uid() == 0 || m.uid() == uid) && m.mode() & 0o077 == 0,
        "credential file must be private and trusted"
    );
    let mut s = String::new();
    f.take(1024 * 1024 + 1).read_to_string(&mut s)?;
    ensure!(s.len() <= 1024 * 1024, "credential too large");
    Ok(s.trim().into())
}
/// 从根目录逐级 openat，拒绝任意路径分量中的符号链接；不创建文件。
pub fn open(path: &Path, flags: i32) -> Result<std::fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    ensure!(path.is_absolute(), "path must be absolute");
    let parts = path
        .components()
        .skip(1)
        .map(|p| match p {
            std::path::Component::Normal(s) => {
                s.to_str().ok_or_else(|| anyhow::anyhow!("non-UTF8 path"))
            }
            _ => Err(anyhow::anyhow!("path must be normalized")),
        })
        .collect::<Result<Vec<_>>>()?;
    let mut dir = std::fs::File::open("/")?;
    for (i, part) in parts.iter().enumerate() {
        let name = std::ffi::CString::new(*part)?;
        // SAFETY: name 在调用期间存活；dir 持有合法目录 fd；成功返回的新 fd 独占所有权。
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_CLOEXEC
                    | libc::O_NOFOLLOW
                    | if i + 1 == parts.len() {
                        flags
                    } else {
                        libc::O_RDONLY | libc::O_DIRECTORY
                    },
            )
        };
        ensure!(
            fd >= 0,
            "cannot safely open path: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: openat 成功返回的新 fd 此前没有被其他 File 接管。
        dir = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    Ok(dir)
}
