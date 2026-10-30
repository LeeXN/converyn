//! Local installation changes only Converyn's identity, files and service.
//! SSH installation uses the operator's OpenSSH key/agent and strict host keys.
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn systemctl(args: &[&str]) -> Result<String> {
    crate::executors::service::command(false, args, &std::sync::atomic::AtomicBool::new(false))
}
fn installation_lock(state: &Path) -> Result<File> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(state.join("installation.lock"))?;
    lock.try_lock_exclusive()
        .context("another installation/uninstall is in progress")?;
    Ok(lock)
}
fn publish_binary(source: &Path) -> Result<()> {
    let binary = Path::new("/usr/local/bin/converyn");
    let temporary = binary.with_file_name(format!(
        ".converyn-install-{}",
        crate::credentials::pki::random_token()?
    ));
    fs::copy(source, &temporary)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))?;
    File::open(&temporary)?.sync_all()?;
    fs::rename(temporary, binary)?;
    File::open(binary.parent().unwrap())?.sync_all()?;
    Ok(())
}
struct PreviousInstallation {
    config: Vec<u8>,
    unit: Vec<u8>,
    binary: PathBuf,
    state: PathBuf,
    active: bool,
    enabled: bool,
}
impl PreviousInstallation {
    fn capture(state: &Path) -> Result<Self> {
        ensure!(
            fs::symlink_metadata("/etc/systemd/system/converyn-agent.service")?.is_file(),
            "nonstandard service symlink requires an explicit migration"
        );
        let unit = fs::read("/etc/systemd/system/converyn-agent.service")?;
        let text = std::str::from_utf8(&unit)?;
        ensure!(
            text.contains("Description=Converyn native supervisor and isolated task executor\n")
                && text.contains(
                    "ExecStart=/usr/local/bin/converyn agent --config /etc/converyn/agent.json\n"
                ),
            "nonstandard service requires an explicit migration"
        );
        let binary = state.join(format!(
            "install-previous-{}",
            crate::credentials::pki::random_token()?
        ));
        let pid = systemctl(&["show", "--property=MainPID", "--value", "converyn-agent"])?
            .trim()
            .parse::<u32>()?;
        let running = systemctl(&[
            "show",
            "--property=ActiveState",
            "--value",
            "converyn-agent",
        ])?;
        let source = if running.trim() == "active" && pid > 0 {
            PathBuf::from(format!("/proc/{pid}/exe"))
        } else {
            PathBuf::from("/usr/local/bin/converyn")
        };
        fs::copy(source, &binary)?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o600))?;
        File::open(&binary)?.sync_all()?;
        File::open(state)?.sync_all()?;
        Ok(Self {
            config: fs::read("/etc/converyn/agent.json")?,
            unit,
            binary,
            state: state.into(),
            active: running.trim() == "active",
            enabled: systemctl(&[
                "show",
                "--property=UnitFileState",
                "--value",
                "converyn-agent",
            ])?
            .trim()
                == "enabled",
        })
    }
    fn rollback(&self) -> Result<()> {
        // Never replace a supervisor after it acquired a new execution grant.
        // Keep its journal and current credentials for recovery instead.
        ensure!(
            !self.state.join("active.json").exists(),
            "active intent prevents rollback; current supervisor retained for recovery"
        );
        systemctl(&["stop", "converyn-agent"])?;
        if self.state.join("active.json").exists() {
            let _ = systemctl(&["start", "converyn-agent"]);
            anyhow::bail!(
                "an intent arrived while stopping; current supervisor restarted for recovery"
            );
        }
        publish_binary(&self.binary)?;
        crate::nodes::agent::durable(Path::new("/etc/converyn/agent.json"), &self.config)?;
        crate::nodes::agent::durable(
            Path::new("/etc/systemd/system/converyn-agent.service"),
            &self.unit,
        )?;
        systemctl(&["daemon-reload"])?;
        systemctl(&["reset-failed", "converyn-agent"])?;
        systemctl(&[
            if self.enabled { "enable" } else { "disable" },
            "converyn-agent",
        ])?;
        if self.active {
            systemctl(&["start", "converyn-agent"])?;
        }
        Ok(())
    }
}
fn task_user() -> Result<(u32, u32)> {
    let name = c"converyn-task";
    let lookup = || -> Option<(u32, u32, String)> {
        let mut record = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut storage = vec![0u8; 65536];
        let mut result = std::ptr::null_mut();
        // SAFETY: record 与 storage 在整个调用期间有效；_r 接口不会共享 NSS 静态缓冲区。
        let status = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                record.as_mut_ptr(),
                storage.as_mut_ptr().cast(),
                storage.len(),
                &mut result,
            )
        };
        if status != 0 || result.is_null() {
            return None;
        }
        // SAFETY: getpwnam_r 成功且 result 非空，record 已初始化，字段指针由 storage 保持有效。
        let record = unsafe { record.assume_init() };
        if record.pw_shell.is_null() {
            return None;
        }
        // SAFETY: NSS 返回 NUL 结尾的 shell，转换结果在 storage 释放前复制为 String。
        let shell = unsafe { std::ffi::CStr::from_ptr(record.pw_shell) }
            .to_string_lossy()
            .into_owned();
        Some((record.pw_uid, record.pw_gid, shell))
    };
    if lookup().is_none() {
        ensure!(
            Command::new("/usr/sbin/useradd")
                .args([
                    "--system",
                    "--no-create-home",
                    "--user-group",
                    "--shell",
                    "/usr/sbin/nologin",
                    "converyn-task"
                ])
                .status()?
                .success(),
            "cannot create dedicated task account"
        );
    }
    let (uid, gid, shell) = lookup().context("task account missing")?;
    ensure!(
        uid > 0 && gid > 0 && (shell.ends_with("/nologin") || shell.ends_with("/false")),
        "existing converyn-task account is not an unprivileged service identity"
    );
    Ok((uid, gid))
}
pub async fn install_local(
    bootstrap: &[u8],
    policy_file: Option<&Path>,
    hardware_file: Option<&Path>,
) -> Result<()> {
    ensure!(
        // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
        unsafe { libc::geteuid() } == 0,
        "installation requires root"
    );
    ensure!(bootstrap.len() <= 1024 * 1024, "bootstrap bundle too large");
    let bundle: crate::nodes::enrollment::BootstrapBundle = serde_json::from_slice(bootstrap)?;
    ensure!(
        converyn_core::model::domain::valid_id(&bundle.grant_id),
        "invalid bootstrap grant"
    );
    let root = Path::new("/var/lib/converyn");
    fs::create_dir_all(root)?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o711))?;
    let state = root.join("state");
    fs::create_dir_all(&state)?;
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700))?;
    crate::credentials::security::private_state_directory(&state)?;
    let _installation_lock = installation_lock(&state)?;
    if state.join("active.json").exists() {
        anyhow::bail!(
            "active execution/outbox prevents installation; use identity recovery without replacing task state"
        );
    }
    let config_dir = Path::new("/etc/converyn");
    fs::create_dir_all(config_dir)?;
    fs::set_permissions(config_dir, fs::Permissions::from_mode(0o700))?;
    let config = config_dir.join("agent.json");
    if !config.exists()
        && systemctl(&["show", "--property=LoadState", "--value", "converyn-agent"])
            .is_ok_and(|s| s.trim() == "loaded")
    {
        anyhow::bail!(
            "existing nonstandard Agent service requires explicit state migration before installation"
        );
    }
    let old_config = if config.exists() {
        let old: crate::nodes::agent::Config = serde_json::from_slice(&fs::read(&config)?)?;
        ensure!(
            old.node_uid == bundle.node_uid && old.cluster == bundle.cluster,
            "installation cannot replace another Node identity"
        );
        ensure!(
            old.state_dir == state,
            "installation cannot implicitly migrate an existing execution directory"
        );
        Some(old)
    } else {
        None
    };
    let (uid, gid) = task_user()?;
    ensure!(
        policy_file.is_none(),
        "node policies were removed; declare host execution in the workflow"
    );
    let mut template = crate::nodes::agent::Config {
        log_max_bytes: crate::executors::log_retention::default_limit(),
        node_uid: bundle.node_uid.clone(),
        cluster: bundle.cluster.clone(),
        servers: bundle.servers.clone(),
        state_dir: state.clone(),
        token_file: config_dir.join("token"),
        tls: crate::credentials::security::Tls {
            cert: config_dir.join("tls.crt"),
            key: config_dir.join("tls.key"),
            ca: config_dir.join("ca.crt"),
        },
        artifact_sources: Default::default(),
        execution: Some(crate::executors::execution::Policy {
            privileged: false,
            uid,
            gid,
            mode: crate::executors::execution::Mode::Systemd,
            work_root: root.join("tasks"),
            home: None,
            writable_paths: vec![],
            memory_bytes: 512 * 1024 * 1024,
            tasks_max: 128,
        }),
        hardware_id_file: hardware_file.map(Path::to_path_buf),
        enrolled: true,
    };
    let existing = old_config.is_some();
    if let Some(old) = old_config {
        if let Some(path) = hardware_file {
            ensure!(
                old.hardware_id_file.as_deref() == Some(path),
                "installation cannot change a bound hardware identity path"
            );
        }
        template = upgrade_template(template, old);
    }
    crate::nodes::agent::validate_local_policy(
        &template,
        &config,
        Path::new("/usr/local/bin/converyn"),
    )?;
    if let Some(execution) = &template.execution {
        execution.validate()?;
    }
    let hardware = crate::executors::host::hardware(template.hardware_id_file.as_deref())?
        .context("no unique hardware identity available; provide --hardware-id-file")?;
    if let Ok(bytes) = fs::read(state.join("status.json"))
        && let Ok(status) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && status["controller_confirmed"] == true
        && let Some(bound) = status["hardware_sha256"].as_str()
    {
        ensure!(
            bound == hardware,
            "bound platform identity changed; explicit identity recovery is required"
        );
    }
    let template_path = state.join(format!("install-template-{}.json", bundle.grant_id));
    crate::nodes::agent::durable(&template_path, &serde_json::to_vec(&template)?)?;
    let bootstrap_path = state.join(format!("install-bootstrap-{}.json", bundle.grant_id));
    crate::nodes::agent::durable(&bootstrap_path, bootstrap)?;
    let already = if config.exists() {
        let old: crate::nodes::agent::Config = serde_json::from_slice(&fs::read(&config)?)?;
        crate::nodes::enrollment::active(&old)?.is_some_and(|c| c.grant_id == bundle.grant_id)
    } else {
        false
    };
    let standard_unit = Path::new("/etc/systemd/system/converyn-agent.service");
    if existing && !standard_unit.exists() {
        let loaded = systemctl(&["show", "--property=LoadState", "--value", "converyn-agent"]);
        ensure!(
            !loaded.is_ok_and(|s| s.trim() == "loaded"),
            "nonstandard service requires an explicit migration"
        );
    }
    let previous = if existing && standard_unit.exists() {
        Some(PreviousInstallation::capture(&state)?)
    } else {
        None
    };
    let outcome=async {
    if previous.is_some() {systemctl(&["stop","converyn-agent"])?;if state.join("active.json").exists(){let _=systemctl(&["start","converyn-agent"]);}ensure!(!state.join("active.json").exists(),"an execution arrived before installation; original supervisor must recover it");}
    if !already {
        crate::nodes::enrollment::enroll(&bootstrap_path, &state, &config, Some(&template_path), None)
            .await?;
    } else {
        crate::nodes::agent::durable(&config, &serde_json::to_vec(&template)?)?;
    }
    let binary = Path::new("/usr/local/bin/converyn");
    if fs::canonicalize(std::env::current_exe()?)? != binary {
        publish_binary(&std::env::current_exe()?)?;
    }
    // The root supervisor executes approved native package transactions. On
    // enforcing SELinux, no_new_privs prevents RPM scriptlet domain transitions;
    // privilege restrictions belong to isolated user tasks, not this supervisor.
    let unit = "[Unit]\nDescription=Converyn native supervisor and isolated task executor\nAfter=network-online.target\nWants=network-online.target\n[Service]\nUser=root\nExecStart=/usr/local/bin/converyn agent --config /etc/converyn/agent.json\nWorkingDirectory=/var/lib/converyn\nRestart=on-failure\nRestartSec=2\nKillMode=control-group\nKillSignal=SIGINT\nTimeoutStopSec=15\nNoNewPrivileges=no\nPrivateTmp=yes\nTasksMax=128\nMemoryMax=1G\n[Install]\nWantedBy=multi-user.target\n";
    crate::nodes::agent::durable(
        Path::new("/etc/systemd/system/converyn-agent.service"),
        unit.as_bytes(),
    )?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "converyn-agent"])?;
    let started_ms = crate::executors::host::now_ms();
    systemctl(&["restart", "converyn-agent"])?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let pid = systemctl(&["show", "--property=MainPID", "--value", "converyn-agent"])
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok());
        let status = fs::read(state.join("status.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        if let (Some(pid), Some(status)) = (pid, status)
            && status["pid"] == pid
            && status["controller_confirmed"] == true
            && status["node_uid"] == bundle.node_uid
            && status["cluster"] == bundle.cluster
            && status["observed_ms"]
                .as_u64()
                .is_some_and(|t| t >= started_ms)
        {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "Agent service installed but authenticated controller heartbeat is not yet confirmed; inspect local status"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok::<(),anyhow::Error>(())
    }.await;
    if let Err(error) = outcome {
        if let Some(previous) = &previous {
            match previous.rollback() {
                Ok(()) => {
                    let _ = fs::remove_file(&previous.binary);
                    return Err(error.context("previous Agent executable/configuration/service restored; credential and execution journals retained"));
                }
                Err(rollback) => {
                    return Err(
                        error.context(format!("installation recovery incomplete: {rollback}"))
                    );
                }
            }
        }
        return Err(error);
    }
    if let Some(previous) = previous {
        let _ = fs::remove_file(previous.binary);
    }
    let _ = fs::remove_file(bootstrap_path);
    let _ = fs::remove_file(template_path);
    File::open(&state)?.sync_all()?;
    Ok(())
}
fn ssh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// 从完整的旧配置继承，避免新增本地策略字段在升级时被默认值静默清空。
fn upgrade_template(
    defaults: crate::nodes::agent::Config,
    mut old: crate::nodes::agent::Config,
) -> crate::nodes::agent::Config {
    old.servers = defaults.servers;
    old.enrolled = true;
    if old.execution.is_none() {
        old.execution = defaults.execution;
    }
    old
}
pub fn uninstall_local(node: &str) -> Result<()> {
    // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
    ensure!(unsafe { libc::geteuid() } == 0, "uninstall requires root");
    ensure!(
        converyn_core::model::domain::valid_id(node),
        "invalid Node UID"
    );
    let config: crate::nodes::agent::Config =
        serde_json::from_slice(&fs::read("/etc/converyn/agent.json")?)?;
    let state = Path::new("/var/lib/converyn/state");
    ensure!(
        config.node_uid == node && config.state_dir == state,
        "standard installation Node/state do not match"
    );
    crate::credentials::security::private_state_directory(state)?;
    let _installation_lock = installation_lock(state)?;
    let unit = Path::new("/etc/systemd/system/converyn-agent.service");
    let text = fs::read_to_string(unit)?;
    ensure!(
        fs::symlink_metadata(unit)?.is_file()
            && text.contains("Description=Converyn native supervisor and isolated task executor\n")
            && text.contains(
                "ExecStart=/usr/local/bin/converyn agent --config /etc/converyn/agent.json\n"
            ),
        "refusing to uninstall an unrecognized Agent service"
    );
    ensure!(
        !state.join("active.json").exists(),
        "active intent/outbox prevents uninstall; resolve and acknowledge it first"
    );
    systemctl(&["stop", "converyn-agent"])?;
    // An authorization can race the initial preflight. Preserve its journal and
    // resume recovery rather than uninstalling a supervisor with pending work.
    if state.join("active.json").exists() {
        let _ = systemctl(&["start", "converyn-agent"]);
        anyhow::bail!("an intent arrived while stopping; Agent restarted for recovery");
    }
    systemctl(&["disable", "converyn-agent"])?;
    let archive = state.join(format!(
        "uninstalled-agent-{}.service",
        crate::credentials::pki::random_token()?
    ));
    crate::credentials::pki::private(&archive, text.as_bytes())?;
    fs::remove_file(unit)?;
    File::open(unit.parent().unwrap())?.sync_all()?;
    systemctl(&["daemon-reload"])?;
    Ok(())
}
fn binary_machine(path: &Path) -> Result<u16> {
    let mut bytes = [0; 20];
    File::open(path)?.read_exact(&mut bytes)?;
    ensure!(
        &bytes[..4] == b"\x7fELF" && bytes[4] == 2 && bytes[5] == 1,
        "installer requires a 64-bit little-endian ELF artifact"
    );
    Ok(u16::from_le_bytes([bytes[18], bytes[19]]))
}
pub struct SshInstall {
    pub hardware_id_file: Option<PathBuf>,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub identity: Option<PathBuf>,
    pub known_hosts: PathBuf,
    pub binary: PathBuf,
    pub bootstrap: PathBuf,
}
pub fn install_ssh(options: SshInstall) -> Result<()> {
    ensure!(
        converyn_core::model::domain::valid_id(&options.user)
            && reqwest::Url::parse(&format!("https://{}/", options.host))?
                .host_str()
                .is_some(),
        "invalid SSH target"
    );
    ensure!(
        !options.host.starts_with('-') && !options.host.contains(['@', '/', '\\', '\n', ' ']),
        "invalid SSH host"
    );
    ensure!(
        options.known_hosts.is_file(),
        "an existing verified known-hosts file is required"
    );
    let _ = crate::credentials::security::secret(&options.bootstrap)?;
    ensure!(options.port > 0, "invalid SSH port");
    let mut args = vec![
        "-p".to_string(),
        options.port.to_string(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=yes".into(),
        "-o".into(),
        format!("UserKnownHostsFile={}", options.known_hosts.display()),
        "-o".into(),
        "ConnectTimeout=10".into(),
    ];
    if let Some(key) = options.identity {
        args.extend([
            "-i".into(),
            key.display().to_string(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
        ]);
    }
    args.push(format!("{}@{}", options.user, options.host));
    let execute = |command: &str, input: Option<&Path>| -> Result<Vec<u8>> {
        let child = Command::new("/usr/bin/ssh")
            .args(&args)
            .arg(command)
            .stdin(if let Some(path) = input {
                Stdio::from(File::open(path)?)
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "SSH operation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    };
    let arch = String::from_utf8(execute("uname -m", None)?)?;
    let machine = binary_machine(&options.binary)?;
    ensure!(
        matches!((arch.trim(), machine), ("x86_64", 62) | ("aarch64", 183)),
        "remote architecture differs; explicitly provide the matching binary, no automatic download occurs"
    );
    let stage = format!(
        "/tmp/converyn-install-{}",
        &crate::credentials::pki::random_token()?[..24]
    );
    execute(
        &format!(
            "set -eu; umask 077; mkdir {}; cat > {}/converyn",
            ssh_quote(&stage),
            ssh_quote(&stage)
        ),
        Some(&options.binary),
    )?;
    let expected = crate::artifacts::artifact::hash_file(&options.binary)?.0;
    let hardware = if let Some(path) = options.hardware_id_file {
        ensure!(
            path.is_absolute(),
            "remote hardware identity path must be absolute"
        );
        format!(
            " --hardware-id-file {}",
            ssh_quote(path.to_str().context("invalid hardware path")?)
        )
    } else {
        String::new()
    };
    let script = format!(
        "set -eu; install -o root -g root -m 755 {stage}/converyn /usr/local/bin/converyn.next; printf '%s\\n' '{expected}  /usr/local/bin/converyn.next' | sha256sum --check --status -; exec /usr/local/bin/converyn.next admin install-local --bootstrap-stdin{hardware}"
    );
    let remote = format!(
        "{} /bin/sh -c {}",
        if options.user == "root" {
            ""
        } else {
            "sudo -n"
        },
        ssh_quote(&script)
    );
    let result = execute(&remote, Some(&options.bootstrap));
    let _ = execute(&format!("rm -rf -- {}", ssh_quote(&stage)), None);
    result?;
    Ok(())
}

#[cfg(test)]
mod config_upgrade_tests {
    use super::*;
    #[test]
    fn upgrade_preserves_complete_local_policy_and_only_explicit_overrides() {
        let original:crate::nodes::agent::Config=serde_json::from_value(serde_json::json!({"node_uid":"node","cluster":"control","servers":["https://127.0.0.1:1"],"state_dir":"/var/lib/converyn/state","token_file":"/etc/converyn/token","tls":{"cert":"/etc/converyn/cert","key":"/etc/converyn/key","ca":"/etc/converyn/ca"},"artifact_sources":{"sources":{"origin":{"origin":"https://example.com"}}},"hardware_id_file":"/etc/converyn/platform-id","enrolled":true})).unwrap();
        let mut defaults = original.clone();
        defaults.artifact_sources = Default::default();
        defaults.servers = vec!["https://127.0.0.1:2".into()];
        let mut expected = original.clone();
        expected.servers = defaults.servers.clone();
        let updated = upgrade_template(defaults.clone(), original.clone());
        assert_eq!(
            serde_json::to_value(updated).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        let updated = upgrade_template(defaults, original);
        assert_eq!(
            serde_json::to_value(updated).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
}
