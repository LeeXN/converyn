//! systemd adapter: argument arrays only, exact unit allowlist, bounded commands,
//! and post-apply observation of both active and enabled state.
use crate::executors::native::{Evidence, Policy};
use anyhow::{Context, Result, ensure};
use converyn_core::model::domain::Operation;
pub use converyn_core::model::service::State;
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
pub(crate) fn command(user: bool, args: &[&str], cancel: &AtomicBool) -> Result<String> {
    ensure!(!cancel.load(Ordering::SeqCst), "service operation canceled");
    let mut cmd = Command::new("/usr/bin/systemctl");
    cmd.env("LC_ALL", "C")
        .arg("--no-pager")
        .arg("--no-ask-password");
    if user {
        // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
        let uid = unsafe { libc::geteuid() };
        cmd.arg("--user")
            .env("XDG_RUNTIME_DIR", format!("/run/user/{uid}"))
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path=/run/user/{uid}/bus"),
            );
    }
    let mut child = cmd
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child.stdout.take().context("missing systemctl stdout")?;
    let reader = thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut saved = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stdout.read(&mut buf)?;
            if n == 0 {
                break;
            }
            if saved.len() + n <= 65536 {
                saved.extend_from_slice(&buf[..n]);
            }
        }
        Ok(saved)
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(30) || cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            anyhow::bail!("systemctl timed out or canceled; service outcome requires observation");
        }
        thread::sleep(Duration::from_millis(50));
    };
    let output = reader
        .join()
        .map_err(|_| anyhow::anyhow!("systemctl output reader failed"))??;
    ensure!(status.success(), "systemctl command rejected ({status})");
    Ok(String::from_utf8(output)?)
}
fn observe(user: bool, name: &str, cancel: &AtomicBool, verify_executable: bool) -> Result<State> {
    let text = command(
        user,
        &[
            "show",
            "--property=LoadState",
            "--property=ActiveState",
            "--property=UnitFileState",
            "--property=SubState",
            "--property=MainPID",
            "--",
            name,
        ],
        cancel,
    )?;
    let properties = text
        .lines()
        .filter_map(|s| s.split_once('='))
        .collect::<std::collections::BTreeMap<_, _>>();
    let property = |key| -> Result<String> {
        Ok(properties
            .get(key)
            .context("missing systemd property")?
            .to_string())
    };
    let pid = property("MainPID")?.parse::<u32>()?;
    let executable_sha256 = if verify_executable && pid > 0 {
        Some(executable_digest(pid, cancel)?)
    } else {
        None
    };
    Ok(State {
        executable_sha256,
        main_pid: if verify_executable && pid > 0 {
            Some(pid)
        } else {
            None
        },
        load: property("LoadState")?,
        active: property("ActiveState")?,
        enabled: property("UnitFileState")?,
        sub: property("SubState")?,
    })
}
pub fn reconcile(policy: &Policy, operation: &Operation, cancel: &AtomicBool) -> Result<Evidence> {
    operation.validate()?;
    let Operation::Service {
        name,
        running,
        enabled,
        executable_sha256,
    } = operation
    else {
        anyhow::bail!("not a service operation")
    };
    ensure!(
        policy.services.contains(name),
        "service is not in the configured allowlist"
    );
    // File delivery can replace a unit while systemd still uses its old
    // ExecStart/environment. Reload that definition before observing/applying.
    let definition_changed = command(
        policy.user_services,
        &["show", "--property=NeedDaemonReload", "--value", "--", name],
        cancel,
    )?
    .trim()
        == "yes";
    if definition_changed {
        command(policy.user_services, &["daemon-reload"], cancel)?;
    }
    let before = observe(
        policy.user_services,
        name,
        cancel,
        executable_sha256.is_some(),
    )?;
    ensure!(
        before.load == "loaded",
        "unit must already exist and load successfully"
    );
    ensure!(
        matches!(before.enabled.as_str(), "enabled" | "disabled"),
        "unit must support explicit persistent enable/disable; masked, static and transient units need operator review"
    );
    let mut changed = definition_changed;
    if before.enabled != if *enabled { "enabled" } else { "disabled" } {
        command(
            policy.user_services,
            &[if *enabled { "enable" } else { "disable" }, "--", name],
            cancel,
        )?;
        changed = true;
    }
    if before.active != if *running { "active" } else { "inactive" } {
        command(
            policy.user_services,
            &[if *running { "start" } else { "stop" }, "--", name],
            cancel,
        )?;
        changed = true;
    }
    if *running
        && before.active == "active"
        && (definition_changed
            || executable_sha256
                .as_ref()
                .is_some_and(|sha| before.executable_sha256.as_ref() != Some(sha)))
    {
        command(policy.user_services, &["restart", "--", name], cancel)?;
        changed = true;
    }
    let after = observe(
        policy.user_services,
        name,
        cancel,
        executable_sha256.is_some(),
    )?;
    ensure!(
        after.matches(*running, *enabled)
            && executable_sha256
                .as_ref()
                .is_none_or(|sha| after.executable_sha256.as_ref() == Some(sha)),
        "service did not reach desired state"
    );
    Ok(Evidence {
        operation: operation.clone(),
        plugin: None,

        before: None,
        after: None,
        changed,
        error: None,
        artifact_size: None,
        package: None,

        reboot: None,
        health: None,

        service_before: Some(before),
        service_after: Some(after),
    })
}

/// Hash the running inode, not the replacement file at ExecStart's path.
/// Re-check the inode afterward so an exec during hashing cannot be accepted.
pub fn executable_digest(pid: u32, cancel: &AtomicBool) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::MetadataExt;
    ensure!(pid > 0, "service has no main process");
    let path = format!("/proc/{pid}/exe");
    let mut file = std::fs::File::open(&path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= converyn_core::model::domain::MAX_ARTIFACT_SIZE,
        "invalid process executable"
    );
    let mut sha = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "executable observation canceled"
        );
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        ensure!(count <= before.len(), "process executable changed");
        sha.update(&buffer[..n]);
    }
    let after = std::fs::metadata(path)?;
    ensure!(
        count == before.len() && before.ino() == after.ino() && before.dev() == after.dev(),
        "process changed executable during observation"
    );
    Ok(hex::encode(sha.finalize()))
}
