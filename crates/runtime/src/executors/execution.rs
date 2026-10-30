//! Root Agents execute Shell as a separate, unprivileged identity. Production
//! systemd mode also bounds the entire task cgroup, including detached children.
use anyhow::{Result, ensure};
pub use converyn_core::model::execution::{Evidence, Mode};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// 只能由本地授权器设置，不能从 JSON 配置或远端 Plan 直接反序列化。
    #[serde(skip)]
    #[doc(hidden)]
    pub privileged: bool,
    pub uid: u32,
    pub gid: u32,
    pub mode: Mode,
    pub work_root: PathBuf,
    #[serde(default)]
    pub home: Option<PathBuf>,
    #[serde(default)]
    pub writable_paths: Vec<PathBuf>,
    #[serde(default = "memory")]
    pub memory_bytes: u64,
    #[serde(default = "tasks")]
    pub tasks_max: u32,
}
fn memory() -> u64 {
    512 * 1024 * 1024
}
fn tasks() -> u32 {
    128
}
/// 系统工具通常位于 sbin；只为本地授权的特权任务提供标准 root 搜索路径。
pub const ROOT_TASK_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
impl Policy {
    pub fn task_path(&self) -> &'static str {
        if self.privileged {
            ROOT_TASK_PATH
        } else {
            "/usr/local/bin:/usr/bin:/bin"
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
            unsafe { libc::geteuid() } == 0,
            "isolated task execution requires a root supervisor"
        );
        ensure!(
            ((self.uid > 0 && self.gid > 0)
                || (self.privileged
                    && self.uid == 0
                    && self.gid == 0
                    && self.mode == Mode::Systemd))
                && self.memory_bytes >= 32 * 1024 * 1024
                && self.tasks_max >= 8,
            "invalid unprivileged task identity or limits"
        );
        for p in std::iter::once(&self.work_root)
            .chain(self.home.iter())
            .chain(self.writable_paths.iter())
        {
            ensure!(
                p.is_absolute()
                    && p != Path::new("/")
                    && p.to_str()
                        .is_some_and(|p| p.bytes().all(|b| b.is_ascii_graphic())
                            && !p.split('/').any(|s| s == "." || s == "..")),
                "task paths must be normalized absolute paths without whitespace"
            );
        }
        Ok(())
    }
    pub fn command(
        &self,
        key: &str,
        script: &[u8],
        timeout: u64,
        journal: &Path,
    ) -> Result<(Command, Option<File>, Evidence)> {
        self.validate()?;
        fs::create_dir_all(&self.work_root)?;
        ensure!(
            fs::symlink_metadata(&self.work_root)?.is_dir()
                && fs::metadata(&self.work_root)?.uid() == 0,
            "task root must be a root-owned directory"
        );
        ensure!(
            fs::canonicalize(&self.work_root)? == self.work_root,
            "task root path must not traverse symlinks"
        );
        for ancestor in self.work_root.ancestors() {
            let metadata = fs::metadata(ancestor)?;
            ensure!(
                metadata.uid() == 0
                    && (metadata.mode() & 0o022 == 0 || metadata.mode() & 0o1000 != 0),
                "task root must have root-owned protected ancestors"
            );
        }
        fs::set_permissions(&self.work_root, fs::Permissions::from_mode(0o711))?;
        let job = self.work_root.join(key);
        fs::create_dir_all(&job)?;
        ensure!(
            fs::symlink_metadata(&job)?.is_dir() && fs::metadata(&job)?.uid() == 0,
            "task directory must be root-owned"
        );
        fs::set_permissions(&job, fs::Permissions::from_mode(0o711))?;
        let work = job.join("work");
        fs::create_dir_all(&work)?;
        ensure!(
            fs::symlink_metadata(&work)?.is_dir(),
            "task work directory must not be symlink"
        );
        let path = std::ffi::CString::new(work.as_os_str().as_encoded_bytes())?;
        ensure!(
            // SAFETY: 路径已验证为受控任务目录，CString 在调用期间有效；检查返回值后才继续。
            unsafe { libc::chown(path.as_ptr(), self.uid, self.gid) } == 0,
            "cannot assign task workspace"
        );
        fs::set_permissions(&work, fs::Permissions::from_mode(0o700))?;
        let home = self.home.as_ref().unwrap_or(&work);
        let mut evidence = Evidence {
            uid: self.uid,
            gid: self.gid,
            mode: self.mode.clone(),
            unit: None,
        };
        let (mut command, keep) = match self.mode {
            Mode::Process => {
                // SAFETY: 名称为静态 NUL 结尾字符串；成功的新 fd 经检查后才接管所有权。
                let fd = unsafe {
                    libc::memfd_create(
                        c"converyn-script".as_ptr(),
                        libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
                    )
                };
                ensure!(fd >= 0, "cannot create sealed script");
                // SAFETY: 系统调用已成功返回新 fd；此处只交给一个 File 接管关闭责任。
                let mut file = unsafe { File::from_raw_fd(fd) };
                file.write_all(script)?;
                ensure!(
                    // SAFETY: 操作的是 File 持有的有效 fd；标志与偏移匹配对应命令，返回值会检查。
                    unsafe {
                        libc::fcntl(
                            fd,
                            libc::F_ADD_SEALS,
                            libc::F_SEAL_SEAL
                                | libc::F_SEAL_SHRINK
                                | libc::F_SEAL_GROW
                                | libc::F_SEAL_WRITE,
                        )
                    } == 0,
                    "cannot seal script"
                );
                ensure!(
                    // SAFETY: 操作的是 File 持有的有效 fd；标志与偏移匹配对应命令，返回值会检查。
                    unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } == 0,
                    "cannot rewind script"
                );
                let mut cmd = Command::new("/bin/bash");
                cmd.arg(format!("/proc/self/fd/{}", file.as_raw_fd()));
                let (uid, gid, memory, tasks) = (
                    self.uid,
                    self.gid,
                    self.memory_bytes,
                    u64::from(self.tasks_max),
                );
                // SAFETY: fork 后闭包只调用异步信号安全的 libc 操作，不分配或加锁；失败阻止执行脚本。
                unsafe {
                    cmd.pre_exec(move || {
                        if libc::fcntl(fd, libc::F_SETFD, 0) < 0
                            || libc::setgroups(0, std::ptr::null()) != 0
                            || libc::setgid(gid) != 0
                            || libc::setuid(uid) != 0
                            || libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                        {
                            return Err(std::io::Error::last_os_error());
                        }
                        for (kind, value) in [
                            (libc::RLIMIT_AS, memory),
                            (libc::RLIMIT_NPROC, tasks),
                            (libc::RLIMIT_NOFILE, 1024),
                            (libc::RLIMIT_CPU, timeout + 5),
                        ] {
                            let limits = libc::rlimit {
                                rlim_cur: value as _,
                                rlim_max: value as _,
                            };
                            if libc::setrlimit(kind, &limits) != 0 {
                                return Err(std::io::Error::last_os_error());
                            }
                        }
                        Ok(())
                    });
                }
                (cmd, Some(file))
            }
            Mode::Systemd => {
                let path = job.join("script.sh");
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o444)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&path)?;
                file.write_all(script)?;
                file.sync_all()?;
                File::open(&job)?.sync_all()?;
                let unit = format!("converyn-task-{}", &key[..key.len().min(32)]);
                evidence.unit = Some(unit.clone());
                let mut cmd = Command::new("/usr/bin/systemd-run");
                cmd.args(["--quiet", "--wait", "--pipe", "--collect", "--unit", &unit]);
                for property in [
                    format!("WorkingDirectory={}", work.display()),
                    "KillMode=control-group".into(),
                    if self.privileged {
                        "NoNewPrivileges=no".into()
                    } else {
                        "NoNewPrivileges=yes".into()
                    },
                    "PrivateTmp=yes".into(),
                    if self.privileged {
                        "ProtectSystem=no".into()
                    } else {
                        "ProtectSystem=strict".into()
                    },
                    if self.privileged {
                        "ProtectHome=no".into()
                    } else {
                        "ProtectHome=read-only".into()
                    },
                    format!("MemoryMax={}", self.memory_bytes),
                    format!("TasksMax={}", self.tasks_max),
                    format!("RuntimeMaxSec={timeout}"),
                    "TimeoutStopSec=5".into(),
                ] {
                    cmd.arg(format!("--property={property}"));
                }
                let journal = journal.to_path_buf();
                let paths = std::iter::once(&work)
                    .chain(std::iter::once(&journal))
                    .chain(self.writable_paths.iter())
                    .map(|p| p.to_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(" ");
                cmd.arg(format!("--property=ReadWritePaths={paths}"));
                cmd.arg(format!("--setenv=PATH={}", self.task_path()))
                    .arg("--setenv=LANG=C")
                    .arg(format!("--setenv=HOME={}", home.display()));
                let binary = std::env::current_exe()?;
                ensure!(
                    binary
                        .to_str()
                        .is_some_and(|s| s.bytes().all(|b| b.is_ascii_graphic())),
                    "supervisor executable path must not contain whitespace"
                );
                cmd.arg(format!(
                    "--property=ExecStopPost={} task-complete --journal {} --unit {}",
                    binary.display(),
                    journal.display(),
                    unit
                ));
                cmd.arg("--")
                    .arg(binary)
                    .arg("task-enter")
                    .arg("--uid")
                    .arg(self.uid.to_string())
                    .arg("--gid")
                    .arg(self.gid.to_string())
                    .arg("--script")
                    .arg(path)
                    .arg("--journal")
                    .arg(&journal);
                if self.privileged {
                    cmd.arg("--privileged");
                }
                (cmd, None)
            }
        };
        command
            .current_dir(&work)
            .env_clear()
            .env("PATH", self.task_path())
            .env("LANG", "C")
            .env("HOME", home)
            .stdin(Stdio::null())
            .process_group(0);
        Ok((command, keep, evidence))
    }
}
pub fn stop_unit(unit: &str) -> Result<()> {
    ensure!(
        unit.starts_with("converyn-task-") && converyn_core::model::domain::valid_id(unit),
        "invalid task unit"
    );
    let mut child = Command::new("/usr/bin/systemctl")
        .args(["--no-ask-password", "stop", unit])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let start = std::time::Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if start.elapsed() > std::time::Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("task unit stop timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let output = crate::executors::service::command(
        false,
        &["show", "--property=ActiveState", "--value", unit],
        &std::sync::atomic::AtomicBool::new(false),
    )?;
    let state = output.trim();
    ensure!(
        state == "inactive" || state == "failed",
        "task cgroup stop not confirmed"
    );
    Ok(())
}

/// PID 1 starts this trusted helper as root so its separate completion callback
/// can write the root-only journal; the actual Shell replaces it after UID drop.
pub fn enter(uid: u32, gid: u32, script: &Path, journal: &Path, privileged: bool) -> Result<()> {
    use std::io::Read;
    ensure!(
        // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
        unsafe { libc::geteuid() } == 0
            && ((uid > 0 && gid > 0 && !privileged) || (uid == 0 && gid == 0 && privileged)),
        "task-enter requires root and a non-root target"
    );
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(script)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.uid() == 0 && meta.mode() & 0o022 == 0,
        "task script must be a protected root-owned regular file"
    );
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "script too large");
    // SAFETY: 名称为静态 NUL 结尾字符串；成功的新 fd 经检查后才接管所有权。
    let fd = unsafe {
        libc::memfd_create(
            c"converyn-task".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    ensure!(fd >= 0, "cannot create sealed script");
    // SAFETY: 系统调用已成功返回新 fd；此处只交给一个 File 接管关闭责任。
    let mut sealed = unsafe { File::from_raw_fd(fd) };
    sealed.write_all(&bytes)?;
    ensure!(
        // SAFETY: 操作的是 File 持有的有效 fd；标志与偏移匹配对应命令，返回值会检查。
        unsafe {
            libc::fcntl(
                fd,
                libc::F_ADD_SEALS,
                libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE,
            )
        } == 0
            // SAFETY: 操作的是 File 持有的有效 fd；标志与偏移匹配对应命令，返回值会检查。
            && unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } == 0,
        "cannot seal script"
    );
    let metadata = fs::symlink_metadata(journal)?;
    ensure!(
        metadata.is_dir() && metadata.uid() == 0 && metadata.mode() & 0o077 == 0,
        "task journal must be private and root-owned"
    );
    let pending: crate::executors::runner::Receipt = serde_json::from_reader(
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(journal.join("receipt.json"))?,
    )?;
    use sha2::{Digest, Sha256};
    ensure!(
        hex::encode(Sha256::digest(&bytes)) == pending.script_sha256
            && pending
                .execution
                .as_ref()
                .is_some_and(|e| e.uid == uid && e.gid == gid && e.mode == Mode::Systemd),
        "task identity or script binding mismatch"
    );
    if privileged {
        let authorization: serde_json::Value = serde_json::from_str(
            &converyn_plugin_api::local::secret(&journal.join("authorization.json"))?,
        )?;
        ensure!(
            authorization["script_sha256"] == pending.script_sha256
                && authorization["operation_id"] == pending.operation_id
                && authorization["uid"] == 0
                && authorization["gid"] == 0,
            "privileged authorization record does not match receipt"
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(journal.join("environment.json"))?;
    let environment: crate::credentials::secrets::Resolved =
        serde_json::from_reader(file.take(4 * 1024 * 1024 + 1))?;
    ensure!(
        environment.values.len() <= 72
            && environment.values.values().map(String::len).sum::<usize>() <= 128 * 1024,
        "invalid task environment"
    );
    let mut command = Command::new("/bin/bash");
    if privileged {
        ensure!(
            environment
                .values
                .keys()
                .all(|name| crate::executors::authorization::safe_privileged_name(name)),
            "unsafe privileged task environment"
        );
        command
            .env_clear()
            .envs(&environment.values)
            .env("PATH", ROOT_TASK_PATH)
            .env("HOME", std::env::current_dir()?)
            .env("LANG", "C");
    } else {
        command.envs(&environment.values);
    }
    command.arg(format!("/proc/self/fd/{fd}"));
    // SAFETY: fork 后闭包只调用异步信号安全的 libc 操作，不分配或加锁；失败阻止执行脚本。
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) < 0
                || libc::setgroups(0, std::ptr::null()) != 0
                || libc::setgid(gid) != 0
                || libc::setuid(uid) != 0
                || (!privileged && libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0)
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let log = |name: &str| -> Result<File> {
        Ok(OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(journal.join(name))?)
    };
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stdout = crate::executors::task_log::capture_prefixed(
        child.stdout.take().unwrap(),
        log("stdout.log")?,
        done.clone(),
        environment.redactions.clone(),
        crate::executors::task_log::environment_prefix(&environment),
    );
    let stderr = crate::executors::task_log::capture_redacted(
        child.stderr.take().unwrap(),
        log("stderr.log")?,
        done.clone(),
        environment.redactions.clone(),
    );
    let status = child.wait()?;
    // SAFETY: Child 创建独立进程组，负 PID 用于清理该任务组；生产任务另由 cgroup 监督。
    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let logs = crate::executors::task_log::Logs {
        stdout: stdout
            .join()
            .map_err(|_| anyhow::anyhow!("task stdout failed"))??,
        stderr: stderr
            .join()
            .map_err(|_| anyhow::anyhow!("task stderr failed"))??,
    };
    crate::nodes::agent::durable(&journal.join("task-logs.json"), &serde_json::to_vec(&logs)?)?;
    crate::nodes::agent::durable(
        &journal.join("task-exit.json"),
        &serde_json::to_vec(
            &serde_json::json!({"operation_id":pending.operation_id,"boot_id":pending.boot_id,"exit_code":status.code()}),
        )?,
    )?;
    fs::remove_file(journal.join("environment.json"))?;
    File::open(journal)?.sync_all()?;
    ensure!(status.success(), "task exited with {status}");
    Ok(())
}
