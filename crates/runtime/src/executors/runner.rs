//! 持久脚本监督与回执恢复。生产任务使用独立 systemd cgroup；Process 模式仅用于显式实验。
//! 授权先于启动落盘，缺失终态回执不会触发脚本自动重放。
use anyhow::{Result, ensure};
pub use converyn_core::model::receipt::{Outcome, Receipt};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::os::unix::{
    fs::{OpenOptionsExt, PermissionsExt},
    process::CommandExt,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn private(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    ensure!(file.metadata()?.is_file(), "expected a regular file");
    Ok(file)
}

fn persist(dir: &Path, receipt: &Receipt) -> Result<()> {
    let mut f = private(&dir.join("receipt.tmp"))?;
    f.set_len(0)?;
    f.write_all(&serde_json::to_vec(receipt)?)?;
    f.sync_all()?;
    fs::rename(dir.join("receipt.tmp"), dir.join("receipt.json"))?;
    File::open(dir)?.sync_all()?;
    Ok(())
}

pub fn run_script(
    state_dir: &Path,
    operation_id: &str,
    script: &Path,
    digest: &str,
    timeout: u64,
) -> Result<Receipt> {
    run_script_cancel(
        state_dir,
        operation_id,
        script,
        digest,
        timeout,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

pub fn run_script_cancel(
    state_dir: &Path,
    operation_id: &str,
    script: &Path,
    digest: &str,
    timeout: u64,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Receipt> {
    run_script_with_policy(
        state_dir,
        operation_id,
        script,
        digest,
        timeout,
        cancel,
        None,
    )
}
pub fn run_script_with_policy(
    state_dir: &Path,
    operation_id: &str,
    script: &Path,
    digest: &str,
    timeout: u64,
    cancel: &std::sync::atomic::AtomicBool,
    policy: Option<&crate::executors::execution::Policy>,
) -> Result<Receipt> {
    run_script_options(
        state_dir,
        operation_id,
        script,
        digest,
        timeout,
        cancel,
        Options {
            outputs: None,
            policy,
            environment: None,
        },
    )
}
pub struct Options<'a> {
    pub outputs: Option<&'a crate::executors::outputs::Context>,
    pub policy: Option<&'a crate::executors::execution::Policy>,
    pub environment: Option<&'a crate::credentials::secrets::Resolved>,
}
pub fn run_script_options(
    state_dir: &Path,
    operation_id: &str,
    script: &Path,
    digest: &str,
    timeout: u64,
    cancel: &std::sync::atomic::AtomicBool,
    options: Options<'_>,
) -> Result<Receipt> {
    let policy = options.policy;
    let empty = crate::credentials::secrets::Resolved::default();
    let mut environment = options.environment.unwrap_or(&empty).clone();
    ensure!(
        environment.redactions.is_empty() || policy.is_some(),
        "secret-bearing execution requires a separate task identity"
    );
    ensure!(
        !operation_id.is_empty() && operation_id.len() <= 256,
        "invalid operation id"
    );
    ensure!(
        (1..=86400).contains(&timeout),
        "timeout must be 1..86400 seconds"
    );
    fs::create_dir_all(state_dir)?;
    ensure!(
        fs::symlink_metadata(state_dir)?.is_dir(),
        "state directory must not be a symlink"
    );
    fs::set_permissions(state_dir, fs::Permissions::from_mode(0o700))?;
    let state_dir = fs::canonicalize(state_dir)?;
    let dir = state_dir.join(hex::encode(Sha256::digest(operation_id.as_bytes())));
    fs::create_dir_all(&dir)?;
    ensure!(
        fs::symlink_metadata(&dir)?.is_dir(),
        "operation directory must not be a symlink"
    );
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let mut ancestor = Some(state_dir.as_path());
    while let Some(path) = ancestor {
        File::open(path)?.sync_all()?;
        ancestor = path.parent();
    }
    let lock = private(&dir.join("lock"))?;
    lock.try_lock_exclusive()?;
    let receipt_path = dir.join("receipt.json");
    if receipt_path.try_exists()? {
        let f = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&receipt_path)?;
        let mut r: Receipt = serde_json::from_reader(
            f.take((converyn_core::model::outputs::MAX_RECEIPT_BYTES + 1) as u64),
        )?;
        ensure!(
            r.operation_id == operation_id
                && r.script_sha256 == digest
                && r.timeout_seconds == timeout,
            "operation id already bound to different inputs"
        );
        if r.outcome == Outcome::Running {
            r = recover_systemd(&dir, r, cancel)?;
            persist(&dir, &r)?;
        }
        return Ok(r);
    }
    if dir.join("script.sh").exists()
        || dir.join("stdout.log").exists()
        || dir.join("stderr.log").exists()
    {
        let receipt = Receipt {
            outputs: Default::default(),
            diagnostic: Some(
                "local execution journal missing after staging; no script replay authorized".into(),
            ),
            execution: None,
            logs: None,
            native: None,
            operation_id: operation_id.into(),
            script_sha256: digest.into(),
            timeout_seconds: timeout,
            outcome: Outcome::NeedsAttention,
            exit_code: None,
            boot_id: crate::executors::host::boot()?,
        };
        persist(&dir, &receipt)?;
        return Ok(receipt);
    }
    // Recover a recorded intent even if its uploaded source was lost on power
    // failure. A matching receipt never executes anything; only a new intent
    // reads and verifies source bytes before recording permission to start.
    let mut bytes = Vec::new();
    File::open(script)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "script exceeds 1 MiB");
    ensure!(
        hex::encode(Sha256::digest(&bytes)) == digest,
        "script digest mismatch"
    );
    let snapshot = dir.join("script.sh");
    let mut file = private(&snapshot)?;
    file.set_len(0)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    let mut r = Receipt {
        outputs: Default::default(),
        diagnostic: None,
        execution: None,
        logs: None,
        native: None,
        operation_id: operation_id.into(),
        script_sha256: digest.into(),
        timeout_seconds: timeout,
        outcome: Outcome::Running,
        exit_code: None,
        boot_id: fs::read_to_string("/proc/sys/kernel/random/boot_id")?
            .trim()
            .into(),
    };
    persist(&dir, &r)?;
    let systemd = policy.is_some_and(|p| p.mode == crate::executors::execution::Mode::Systemd);
    let stdout = private(&dir.join(if systemd {
        "supervisor.stdout.log"
    } else {
        "stdout.log"
    }))?;
    let stderr = private(&dir.join(if systemd {
        "supervisor.stderr.log"
    } else {
        "stderr.log"
    }))?;
    let (mut command, _sealed) = if let Some(policy) = policy {
        if policy.privileged {
            crate::nodes::agent::durable(
                &dir.join("authorization.json"),
                &serde_json::to_vec(
                    &serde_json::json!({"operation_id":operation_id,"script_sha256":digest,"uid":policy.uid,"gid":policy.gid}),
                )?,
            )?;
        }
        let (command, sealed, evidence) = policy.command(
            &hex::encode(Sha256::digest(operation_id)),
            &bytes,
            timeout,
            &dir,
        )?;
        r.execution = Some(Box::new(evidence));
        persist(&dir, &r)?;
        (command, sealed)
    } else {
        let mut command = Command::new("/bin/bash");
        command.arg(&snapshot).stdin(Stdio::null()).process_group(0);
        (command, None)
    };
    if let Some(context) = options.outputs {
        ensure!(
            !context.no_log() || policy.is_some(),
            "sensitive results require supervised execution"
        );
        let key = hex::encode(Sha256::digest(operation_id));
        let work = policy
            .map(|p| p.work_root.join(key).join("work"))
            .unwrap_or_else(|| dir.clone());
        let uid = policy.map_or_else(|| unsafe { libc::geteuid() }, |p| p.uid);
        let file = crate::executors::outputs::prepare(
            &dir,
            &work,
            uid,
            policy.map_or_else(|| unsafe { libc::getegid() }, |p| p.gid),
            context,
        )?;
        environment.values.insert(
            "CONVERYN_OUTPUT_FILE".into(),
            file.to_string_lossy().into_owned(),
        );
        environment.values.insert(
            "CONVERYN_EXECUTABLE".into(),
            std::env::current_exe()?.to_string_lossy().into_owned(),
        );
    }
    if systemd {
        crate::nodes::agent::durable(
            &dir.join("environment.json"),
            &serde_json::to_vec(&environment)?,
        )?;
    } else {
        command.envs(&environment.values);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let logs_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let out_thread = crate::executors::task_log::capture_prefixed(
        child.stdout.take().unwrap(),
        stdout,
        logs_done.clone(),
        environment.redactions.clone(),
        if systemd {
            Vec::new()
        } else {
            crate::executors::task_log::environment_prefix(&environment)
        },
    );
    let err_thread = crate::executors::task_log::capture_redacted(
        child.stderr.take().unwrap(),
        stderr,
        logs_done.clone(),
        environment.redactions.clone(),
    );
    let began = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            r.exit_code = status.code();
            r.outcome = if status.success() {
                Outcome::Succeeded
            } else {
                Outcome::Failed
            };
            break;
        }
        if began.elapsed() >= Duration::from_secs(timeout)
            || cancel.load(std::sync::atomic::Ordering::SeqCst)
        {
            // Bound the process group. Scripts escaping it require cgroup supervision
            // in the production runner; this CLI does not claim a sandbox boundary.
            if let Some(unit) = r.execution.as_ref().and_then(|e| e.unit.as_deref()) {
                if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                    crate::nodes::agent::durable(&dir.join("cancel-requested"), b"cancel")?;
                }
                if crate::executors::execution::stop_unit(unit).is_err() {
                    let _ = child.kill();
                    let _ = child.wait();
                    r.outcome = Outcome::NeedsAttention;
                    break;
                }
            }
            // SAFETY: Child 创建独立进程组，负 PID 用于清理该任务组；生产任务另由 cgroup 监督。
            let rc = unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            ensure!(
                rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
                "cannot stop script process group"
            );
            child.wait()?;
            r.outcome = if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                Outcome::Canceled
            } else {
                Outcome::TimedOut
            };
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    if r.execution.as_ref().is_none_or(|e| e.unit.is_none()) {
        // A Shell task may not keep background members running after its main process exits.
        // SAFETY: Child 创建独立进程组，负 PID 用于清理该任务组；生产任务另由 cgroup 监督。
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    }
    if r.execution.as_ref().is_some_and(|e| e.unit.is_some()) {
        r = recover_systemd(&dir, r, cancel)?;
    }
    logs_done.store(true, std::sync::atomic::Ordering::SeqCst);
    let stdout = out_thread
        .join()
        .map_err(|_| anyhow::anyhow!("stdout capture failed"))??;
    let stderr = err_thread
        .join()
        .map_err(|_| anyhow::anyhow!("stderr capture failed"))??;
    if r.logs.is_none() {
        r.logs = Some(Box::new(crate::executors::task_log::Logs {
            stdout,
            stderr,
        }));
    }
    if !systemd {
        crate::executors::outputs::finish(&dir, &mut r)?;
    }
    if options.outputs.is_some_and(|c| c.no_log()) {
        r.logs = None;
    }
    persist(&dir, &r)?;
    Ok(r)
}

fn completion(dir: &Path, expected: &Receipt) -> Result<Option<Receipt>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("completion.json"))
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let receipt: Receipt = serde_json::from_reader(
        file.take((converyn_core::model::outputs::MAX_RECEIPT_BYTES + 1) as u64),
    )?;
    ensure!(
        receipt.operation_id == expected.operation_id
            && receipt.script_sha256 == expected.script_sha256
            && receipt.timeout_seconds == expected.timeout_seconds
            && receipt.boot_id == expected.boot_id
            && receipt.execution == expected.execution
            && receipt.outcome != Outcome::Running,
        "completion binding mismatch"
    );
    crate::executors::outputs::cleanup(dir)?;
    Ok(Some(receipt))
}
fn recover_systemd(
    dir: &Path,
    mut receipt: Receipt,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Receipt> {
    if let Some(completed) = completion(dir, &receipt)? {
        return Ok(completed);
    }
    let Some(unit) = receipt.execution.as_ref().and_then(|e| e.unit.clone()) else {
        receipt.outcome = Outcome::NeedsAttention;
        return Ok(receipt);
    };
    let start = Instant::now();
    loop {
        if let Some(completed) = completion(dir, &receipt)? {
            return Ok(completed);
        }
        if fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim() != receipt.boot_id
            || start.elapsed() > Duration::from_secs(receipt.timeout_seconds + 30)
        {
            break;
        }
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            crate::nodes::agent::durable(&dir.join("cancel-requested"), b"cancel")?;
            if crate::executors::execution::stop_unit(&unit).is_err() {
                break;
            }
        }
        let Ok(active) = crate::executors::service::command(
            false,
            &["show", "--property=ActiveState", "--value", &unit],
            &std::sync::atomic::AtomicBool::new(false),
        ) else {
            break;
        };
        if !matches!(
            active.trim(),
            "active" | "activating" | "deactivating" | "reloading"
        ) {
            if let Some(completed) = completion(dir, &receipt)? {
                return Ok(completed);
            }
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    receipt.outcome = Outcome::NeedsAttention;
    Ok(receipt)
}
/// Called only by the root-owned systemd task definition after its process and
/// cgroup lifecycle. A separate file avoids deadlocking the supervisor's lock.
pub fn complete_task(dir: &Path, unit: &str) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(
        // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
        unsafe { libc::geteuid() } == 0,
        "completion callback requires root"
    );
    let metadata = fs::symlink_metadata(dir)?;
    ensure!(
        metadata.is_dir() && metadata.uid() == 0 && metadata.mode() & 0o077 == 0,
        "completion directory must be private and root-owned"
    );
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("receipt.json"))?;
    let mut receipt: Receipt = serde_json::from_reader(
        file.take((converyn_core::model::outputs::MAX_RECEIPT_BYTES + 1) as u64),
    )?;
    ensure!(
        receipt.execution.as_ref().and_then(|e| e.unit.as_deref()) == Some(unit),
        "completion unit mismatch"
    );
    ensure!(
        receipt.boot_id == fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim(),
        "completion boot mismatch"
    );
    if completion(dir, &receipt)?.is_some() {
        return Ok(());
    }
    let result = std::env::var("SERVICE_RESULT")?;
    let code = std::env::var("EXIT_CODE").unwrap_or_default();
    let exit = std::env::var("EXIT_STATUS")
        .ok()
        .and_then(|s| s.parse::<i32>().ok());
    receipt.outcome = if result == "success" && code == "exited" && exit == Some(0) {
        Outcome::Succeeded
    } else if dir.join("cancel-requested").exists() {
        Outcome::Canceled
    } else if result == "timeout" {
        Outcome::TimedOut
    } else {
        Outcome::Failed
    };
    receipt.exit_code = if code == "exited" { exit } else { None };
    let mut recorded_exit = false;
    if let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("task-exit.json"))
    {
        recorded_exit = true;
        let record: serde_json::Value = serde_json::from_reader(
            file.take((converyn_core::model::outputs::MAX_RECEIPT_BYTES + 1) as u64),
        )?;
        ensure!(
            record["operation_id"] == receipt.operation_id && record["boot_id"] == receipt.boot_id,
            "task exit binding mismatch"
        );
        receipt.exit_code = record["exit_code"]
            .as_i64()
            .and_then(|n| i32::try_from(n).ok());
    }
    if !recorded_exit && !matches!(receipt.outcome, Outcome::Canceled | Outcome::TimedOut) {
        receipt.outcome = Outcome::NeedsAttention;
    }
    if receipt.outcome == Outcome::Succeeded && receipt.exit_code != Some(0) {
        receipt.outcome = Outcome::NeedsAttention;
    }
    receipt.logs = if let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("task-logs.json"))
    {
        let logs: crate::executors::task_log::Logs = serde_json::from_reader(
            file.take((converyn_core::model::outputs::MAX_RECEIPT_BYTES + 1) as u64),
        )?;
        ensure!(logs.valid(), "invalid task logs");
        Some(Box::new(logs))
    } else {
        None
    };
    if dir.join("environment.json").exists() {
        fs::remove_file(dir.join("environment.json"))?;
        File::open(dir)?.sync_all()?;
    }
    crate::executors::outputs::finish(dir, &mut receipt)?;
    crate::nodes::agent::durable(&dir.join("completion.json"), &serde_json::to_vec(&receipt)?)?;
    Ok(())
}
