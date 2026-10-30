//! Declarative file reconciliation. Every path component is opened without following
//! symlinks, and publication replaces an inode instead of truncating a linked file.
use crate::executors::runner::{Outcome, Receipt};
use anyhow::{Context, Result, ensure};
use converyn_core::model::domain::Operation;
pub use converyn_core::model::evidence::{Evidence, FileState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(skip)]
    pub runtime_inputs: Option<crate::credentials::secrets::Resolved>,
    #[serde(skip)]
    pub runtime_fence: u64,
    #[serde(default)]
    pub plugins: std::collections::BTreeMap<String, crate::plugins::Grant>,
    #[serde(default)]
    pub file_paths: Vec<PathBuf>,
    #[serde(skip)]
    pub runtime_state_dir: Option<PathBuf>,
    #[serde(skip)]
    pub runtime_project: Option<String>,
    #[serde(skip)]
    pub runtime_owner: Option<String>,
    #[serde(default)]
    pub health_origins: Vec<String>,
    #[serde(default)]
    pub allow_reboot: bool,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default = "default_user_services")]
    pub user_services: bool,
    #[serde(default)]
    pub file_roots: Vec<PathBuf>,
}

fn default_user_services() -> bool {
    true
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            runtime_inputs: None,
            runtime_fence: 0,
            plugins: Default::default(),
            file_roots: Vec::new(),
            file_paths: Vec::new(),
            packages: Vec::new(),
            runtime_project: None,
            runtime_owner: None,

            runtime_state_dir: None,

            allow_reboot: false,
            health_origins: Vec::new(),

            services: Vec::new(),
            user_services: true,
        }
    }
}

impl Policy {
    pub(crate) fn allows_file(&self, path: &Path) -> bool {
        self.file_paths.iter().any(|p| p == path)
            || self.file_roots.iter().any(|root| {
                root.is_absolute()
                    && root != Path::new("/")
                    && path != root
                    && path.starts_with(root)
            })
    }
    pub fn validate(&self, protected: &[&Path]) -> Result<()> {
        for file in &self.file_paths {
            Operation::File {
                path: file.to_str().context("invalid file policy path")?.into(),
                sha256: "a".repeat(64),
                mode: 0o600,
            }
            .validate()?;
            for path in protected {
                let target = if path.exists() {
                    std::fs::canonicalize(path)?
                } else {
                    path.to_path_buf()
                };
                ensure!(
                    !file.starts_with(&target) && !target.starts_with(file),
                    "allowed file overlaps protected runtime identity"
                );
            }
        }

        for root in &self.file_roots {
            ensure!(
                root.is_absolute()
                    && root != Path::new("/")
                    && std::fs::canonicalize(root)? == *root
                    && root.is_dir(),
                "native file root must be an existing canonical directory"
            );
            for path in protected {
                let absolute = if path.exists() {
                    std::fs::canonicalize(path)?
                } else {
                    path.to_path_buf()
                };
                ensure!(
                    absolute.is_absolute()
                        && !absolute.starts_with(root)
                        && !root.starts_with(&absolute),
                    "native root overlaps agent state or credentials"
                );
            }
        }
        Ok(())
    }
}

pub(crate) fn component(parent: &File, name: &str, flags: i32, mode: u32) -> Result<File> {
    let name = CString::new(name)?;
    // SAFETY: 父目录 fd 与 CString 在调用期间存活；O_NOFOLLOW 拒绝符号链接。
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: 系统调用已成功返回新 fd；此处只交给一个 File 接管关闭责任。
    Ok(unsafe { File::from_raw_fd(fd) })
}
pub(crate) fn parent(path: &Path) -> Result<(File, String)> {
    let text = path.to_str().context("non-UTF8 path")?;
    ensure!(
        text.starts_with('/')
            && text
                .split('/')
                .skip(1)
                .all(|s| !s.is_empty() && s != "." && s != ".."),
        "path must be normalized and absolute"
    );
    let mut names = text.split('/').skip(1).collect::<Vec<_>>();
    let name = names.pop().context("missing file name")?.to_string();
    let mut dir = File::open("/")?;
    for part in names {
        dir = component(&dir, part, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    }
    Ok((dir, name))
}
pub(crate) fn parent_create(path: &Path, policy: &Policy) -> Result<(File, String)> {
    let text = path.to_str().context("non-UTF8 path")?;
    let mut names = text.split('/').skip(1).collect::<Vec<_>>();
    let leaf = names.pop().context("missing file name")?.to_owned();
    let mut dir = File::open("/")?;
    let mut current = PathBuf::from("/");
    for name in names {
        current.push(name);
        match component(&dir, name, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
            Ok(next) => dir = next,
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                ensure!(
                    policy.file_paths.iter().any(|p| p == path)
                        || policy
                            .file_roots
                            .iter()
                            .any(|root| current.starts_with(root)),
                    "directory creation is outside allowed roots"
                );
                let name_c = CString::new(name)?;
                // SAFETY: 父目录 fd 持有已授权目录，名称是有效 CString，返回值和 EEXIST 会检查。
                let result = unsafe { libc::mkdirat(dir.as_raw_fd(), name_c.as_ptr(), 0o755) };
                if result != 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST)
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                dir.sync_all()?;
                dir = component(&dir, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok((dir, leaf))
}

pub(crate) fn observe(dir: &File, name: &str, limit: u64) -> Result<Option<FileState>> {
    let mut f = match component(dir, name, libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(f) => f,
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    let meta = f.metadata()?;
    ensure!(meta.is_file(), "target is not a regular file");
    ensure!(
        meta.len() <= limit,
        "managed file exceeds operation size limit"
    );
    let mut hash = Sha256::new();
    let mut buf = [0u8; 256 * 1024];
    let mut count = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        ensure!(count <= limit, "managed file grew beyond limit");
        hash.update(&buf[..n]);
    }
    Ok(Some(FileState {
        bytes: count,
        sha256: hex::encode(hash.finalize()),
        mode: meta.mode() & 0o7777,
    }))
}
/// Each invocation observes actual bytes and permissions. This is safe to repeat
/// after a lost receipt; it never infers success from a generation or boot counter.
pub fn reconcile_file(
    policy: &Policy,
    operation: &Operation,
    content: &[u8],
    cancel: &AtomicBool,
) -> Result<Evidence> {
    operation.validate()?;
    let Operation::File { path, sha256, mode } = operation else {
        anyhow::bail!("not a file operation")
    };
    ensure!(
        content.len() <= 65536 && hex::encode(Sha256::digest(content)) == *sha256,
        "definition digest or size mismatch"
    );
    let path = Path::new(path);
    ensure!(
        policy.allows_file(path),
        "file path is outside configured roots"
    );
    let (dir, name) = parent_create(path, policy)?;
    let before = observe(&dir, &name, 65536)?;
    let wanted = FileState {
        bytes: content.len() as u64,
        sha256: sha256.clone(),
        mode: *mode,
    };
    if before.as_ref() == Some(&wanted) {
        return Ok(Evidence {
            operation: operation.clone(),
            plugin: None,

            service_before: None,
            service_after: None,
            before: before.clone(),
            after: before,
            changed: false,
            error: None,

            artifact_size: None,
            package: None,

            reboot: None,
            health: None,
        });
    }
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "canceled before file publication"
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
        staged.write_all(content)?;
        staged.set_permissions(std::fs::Permissions::from_mode(*mode))?;
        staged.sync_all()?;
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "canceled before file publication"
        );
        let rc =
            // SAFETY: 目录 fd 和名称 CString 在调用期间有效；暂存文件在同一受控目录原子发布。
            unsafe { libc::renameat(dir.as_raw_fd(), tmp.as_ptr(), dir.as_raw_fd(), dst.as_ptr()) };
        if rc != 0 {
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
    let after = observe(&dir, &name, 65536)?;
    ensure!(
        after.as_ref() == Some(&wanted),
        "file changed during verification"
    );
    Ok(Evidence {
        operation: operation.clone(),
        plugin: None,

        service_before: None,
        service_after: None,
        before,
        after,
        changed: true,
        error: None,

        artifact_size: None,
        package: None,

        reboot: None,
        health: None,
    })
}

pub fn cached_receipt(state: &Path, id: &str, operation: &Operation) -> Result<Option<Receipt>> {
    let key = hex::encode(Sha256::digest(id));
    let journal = state.join(format!("{key}.json"));
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&journal)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let previous: Receipt = serde_json::from_reader(file.take(128 * 1024))?;
    ensure!(
        previous.operation_id == id
            && previous
                .native
                .as_ref()
                .is_some_and(|e| &e.operation == operation),
        "native journal binding conflict"
    );
    Ok(Some(previous))
}

pub fn execute(
    state: &Path,
    id: &str,
    policy: &Policy,
    operation: &Operation,
    content: &[u8],
    cancel: &AtomicBool,
    artifact_path: Option<&Path>,
) -> Result<Receipt> {
    use fs2::FileExt;
    std::fs::create_dir_all(state)?;
    let key = hex::encode(Sha256::digest(id));
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(state.join(format!("{key}.lock")))?;
    lock.try_lock_exclusive()?;
    let journal = state.join(format!("{key}.json"));
    if matches!(operation, Operation::Reboot { .. }) {
        return crate::executors::host::reboot(
            &journal,
            id,
            policy,
            operation,
            cached_receipt(state, id, operation)?,
        );
    }
    if let Some(mut previous) = cached_receipt(state, id, operation)? {
        if previous.outcome == Outcome::Running {
            previous.outcome = Outcome::NeedsAttention;
            if let Some(e) = &mut previous.native {
                e.error = Some(
                    "transaction interrupted; observed version is not completion proof".into(),
                );
            }
            crate::nodes::agent::durable(&journal, &serde_json::to_vec(&previous)?)?;
        }
        return Ok(previous);
    }
    if matches!(operation, Operation::Package { .. }) {
        return execute_package(
            &journal,
            id,
            policy,
            operation,
            artifact_path.context("package artifact missing")?,
            cancel,
        );
    }
    let result = match operation {
        Operation::File { .. } => reconcile_file(policy, operation, content, cancel),
        Operation::Service { .. } => {
            crate::executors::service::reconcile(policy, operation, cancel)
        }

        Operation::HttpCheck { .. } => crate::executors::health::check(policy, operation, cancel),

        Operation::Plugin { call } => crate::plugins::execute_node(policy, operation, call, cancel),
        Operation::Binary { .. } => crate::executors::binary::reconcile(
            policy,
            operation,
            artifact_path.context("artifact cache path missing")?,
            cancel,
        ),
        _ => anyhow::bail!("unsupported native operation"),
    };
    let (outcome, evidence) = match result {
        Ok(e) => (Outcome::Succeeded, e),
        Err(e) => (
            if matches!(operation, Operation::HttpCheck { .. }) {
                if cancel.load(Ordering::SeqCst) {
                    Outcome::Canceled
                } else {
                    Outcome::Failed
                }
            } else {
                Outcome::NeedsAttention
            },
            Evidence {
                operation: operation.clone(),
                plugin: None,

                service_before: None,
                service_after: None,
                before: None,
                after: None,
                changed: false,
                error: Some(e.to_string()),
                artifact_size: None,
                package: None,

                reboot: None,
                health: None,
            },
        ),
    };
    let receipt = Receipt {
        outputs: Default::default(),
        diagnostic: None,
        execution: None,
        logs: None,
        operation_id: id.into(),
        script_sha256: String::new(),
        timeout_seconds: 0,
        outcome,
        exit_code: None,
        boot_id: std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
            .trim()
            .into(),
        native: Some(Box::new(evidence)),
    };
    crate::nodes::agent::durable(&journal, &serde_json::to_vec(&receipt)?)?;
    Ok(receipt)
}

fn execute_package(
    journal: &Path,
    id: &str,
    policy: &Policy,
    operation: &Operation,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<Receipt> {
    let prepared = crate::executors::package::prepare(policy, operation, path);
    let mut evidence = Evidence {
        operation: operation.clone(),
        plugin: None,

        before: None,
        after: None,
        service_before: None,
        service_after: None,
        changed: false,
        error: None,

        artifact_size: operation.artifact().map(|a| a.size),
        package: None,

        reboot: None,
        health: None,
    };
    let mut receipt = Receipt {
        outputs: Default::default(),
        diagnostic: None,
        operation_id: id.into(),
        script_sha256: String::new(),
        timeout_seconds: 0,
        exit_code: None,
        outcome: Outcome::Running,
        boot_id: std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
            .trim()
            .into(),
        native: None,
        execution: None,
        logs: None,
    };
    match prepared {
        Err(e) => {
            evidence.error = Some(e.to_string());
            receipt.outcome = Outcome::Failed;
        }
        Ok(before) => {
            evidence.package = Some(before.clone());
            receipt.native = Some(Box::new(evidence.clone()));
            crate::nodes::agent::durable(journal, &serde_json::to_vec(&receipt)?)?;
            match crate::executors::package::install(operation, path, cancel, before, journal) {
                Ok(after) => {
                    evidence.changed = true;
                    evidence.package = Some(after);
                    receipt.outcome = if evidence.verified() {
                        Outcome::Succeeded
                    } else {
                        evidence.error = Some(
                            "package transaction or installed state verification failed".into(),
                        );
                        Outcome::Failed
                    };
                }
                Err(e) => {
                    evidence.error = Some(e.to_string());
                    receipt.outcome = Outcome::NeedsAttention;
                }
            }
        }
    }
    receipt.native = Some(Box::new(evidence));
    crate::nodes::agent::durable(journal, &serde_json::to_vec(&receipt)?)?;
    Ok(receipt)
}

/// A committed Work is already authorized by the control plane. Exact runtime
/// capabilities are derived from that Work rather than maintained on each host.
pub fn managed_policy(operation: &Operation, project: Option<&str>) -> Result<Policy> {
    let mut policy = Policy::default();
    match operation {
        Operation::File { path, .. } => policy.file_paths.push(path.into()),
        Operation::Binary {
            path,
            version_directory,
            artifact,
            mode,
        } => {
            policy.file_paths.push(path.into());
            // Versioned publication stages one immutable file before activation.
            // Grant that exact content/mode path, not the entire version directory.
            if let Some(directory) = version_directory {
                policy
                    .file_paths
                    .push(format!("{directory}/{}-{mode:o}", artifact.sha256).into());
            }
        }
        Operation::Package { name, .. } => policy.packages.push(name.clone()),
        Operation::Service { name, .. } => {
            policy.services.push(name.clone());
            policy.user_services = false;
        }
        Operation::Reboot { .. } => policy.allow_reboot = true,
        Operation::HttpCheck { url, .. } => policy.health_origins.push(
            crate::executors::health::validate_url(url)?
                .origin()
                .ascii_serialization(),
        ),
        Operation::Plugin { call } => {
            crate::plugins::registry().validate(call)?;
            let s = &call.spec;
            let rules = match call.plugin.id.as_str() {
                "converyn.linux-host" => serde_json::json!({
                    "directories":s.get("paths").cloned().unwrap_or(serde_json::json!([])),
                    "modules":s.get("names").cloned().unwrap_or(serde_json::json!([])),
                    "services":s.get("name").map(|v|vec![v]).unwrap_or_default(),
                    "sysctls":s.get("key").map(|v|vec![v]).unwrap_or_default(),
                    "allow_swapoff":true,"allow_initramfs":true,"allow_kernel_arguments":true,"allow_kernel_selection":true
                }),
                "converyn.docker" => {
                    let image = s["image"].as_str().context("image missing")?;
                    let first = image.split('/').next().unwrap();
                    let registry =
                        if first.contains('.') || first.contains(':') || first == "localhost" {
                            first
                        } else {
                            "docker.io"
                        };
                    serde_json::json!({"names":[s["name"]],"registries":[registry],"allow_root_user":true,
                        "volume_roots":s["mounts"].as_array().map(|a|a.iter().map(|m|m["source"].clone()).collect::<Vec<_>>()).unwrap_or_default()})
                }
                _ => serde_json::json!({}),
            };
            policy.plugins.insert(
                call.plugin.id.clone(),
                crate::plugins::Grant {
                    identity: call.plugin.clone(),
                    projects: [project.context("plugin project missing")?.to_owned()].into(),
                    actions: [call.action.clone()].into(),
                    policy: rules,
                    protected_paths: vec![],
                },
            );
        }
        _ => {}
    }
    Ok(policy)
}
