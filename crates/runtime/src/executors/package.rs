//! Package installation is a transaction, not a replayable version comparison.
//! A durable Running receipt precedes the package manager; interruption remains
//! uncertain even if its package database later shows the requested version.
use crate::executors::native::Policy;
use anyhow::{Context, Result, ensure};
use converyn_core::model::domain::Operation;
pub use converyn_core::model::package::{Evidence, Manager};
use std::{
    io::Read,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
pub(crate) fn query(program: &str, args: &[&str]) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut output = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .by_ref()
            .take(65537)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            anyhow::bail!("native command query timed out");
        }
        thread::sleep(Duration::from_millis(50));
    };
    let bytes = reader
        .join()
        .map_err(|_| anyhow::anyhow!("package query reader failed"))??;
    ensure!(
        status.success() && bytes.len() <= 65536,
        "native command query failed"
    );
    Ok(String::from_utf8(bytes)?.trim().into())
}
fn observed(manager: &Manager, name: &str, expected: Option<&str>) -> Result<String> {
    match manager {
        Manager::Rpm => {
            let versions = query(
                "/usr/bin/rpm",
                &[
                    "-q",
                    "--qf",
                    "%{EPOCHNUM}:%{VERSION}-%{RELEASE}.%{ARCH}\n",
                    "--",
                    name,
                ],
            )?;
            if let Some(expected) = expected {
                ensure!(
                    versions.lines().any(|v| v == expected),
                    "requested RPM version is not installed"
                );
                Ok(expected.into())
            } else {
                Ok(versions)
            }
        }
        Manager::Deb => {
            ensure!(
                query(
                    "/usr/bin/dpkg-query",
                    &["-W", "-f", "${Status}", "--", name]
                )? == "install ok installed",
                "Debian package is not fully configured"
            );
            query(
                "/usr/bin/dpkg-query",
                &["-W", "-f", "${Version}", "--", name],
            )
        }
    }
}
pub fn prepare(policy: &Policy, operation: &Operation, path: &Path) -> Result<Evidence> {
    operation.validate()?;
    let Operation::Package {
        manager,
        name,
        version,
        artifact,
        ..
    } = operation
    else {
        anyhow::bail!("not a package operation")
    };
    ensure!(
        // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
        unsafe { libc::geteuid() } == 0,
        "package installation requires root supervisor"
    );
    ensure!(
        policy.packages.contains(name),
        "package is not in the configured allowlist"
    );
    ensure!(
        crate::artifacts::artifact::hash_file(path)? == (artifact.sha256.clone(), artifact.size),
        "package artifact failed verification"
    );
    let file = path.to_str().context("invalid artifact path")?;
    let (actual_name, actual_version) = match manager {
        Manager::Rpm => (
            query("/usr/bin/rpm", &["-qp", "--qf", "%{NAME}", "--", file])?,
            query(
                "/usr/bin/rpm",
                &[
                    "-qp",
                    "--qf",
                    "%{EPOCHNUM}:%{VERSION}-%{RELEASE}.%{ARCH}",
                    "--",
                    file,
                ],
            )?,
        ),
        Manager::Deb => (
            query("/usr/bin/dpkg-deb", &["-f", file, "Package"])?,
            query("/usr/bin/dpkg-deb", &["-f", file, "Version"])?,
        ),
    };
    ensure!(
        &actual_name == name && &actual_version == version,
        "artifact package name/version differs from approved plan"
    );
    Ok(Evidence {
        before: observed(manager, name, None).ok(),
        after: None,
        transaction_exit: None,
        diagnostic: String::new(),
    })
}
pub fn install(
    operation: &Operation,
    path: &Path,
    cancel: &AtomicBool,
    mut evidence: Evidence,
    journal: &Path,
) -> Result<Evidence> {
    let Operation::Package {
        manager,
        name,
        version,
        install_only,
        timeout_seconds,
        ..
    } = operation
    else {
        anyhow::bail!("not a package operation")
    };
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "package transaction canceled before start"
    );
    let mut command = match manager {
        Manager::Rpm => {
            let mut c = Command::new("/usr/bin/rpm");
            if *install_only {
                c.args(["-i", "--replacepkgs", "--"]);
            } else {
                c.args(["-U", "--replacepkgs", "--oldpackage", "--"]);
            }
            c.arg(path);
            c
        }
        Manager::Deb => {
            let mut c = Command::new("/usr/bin/dpkg");
            c.args(["--install", "--"])
                .arg(path)
                .env("DEBIAN_FRONTEND", "noninteractive");
            c
        }
    };
    use std::os::unix::fs::OpenOptionsExt;
    let log = |suffix: &str| -> Result<std::fs::File> {
        Ok(std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(journal.with_extension(suffix))?)
    };
    let stdout = log("stdout.log")?;
    let stderr = log("stderr.log")?;
    let mut child = command
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let done = std::sync::Arc::new(AtomicBool::new(false));
    let out =
        crate::executors::task_log::capture(child.stdout.take().unwrap(), stdout, done.clone());
    let err =
        crate::executors::task_log::capture(child.stderr.take().unwrap(), stderr, done.clone());
    let mut interrupted = false;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel.load(Ordering::SeqCst)
            || start.elapsed() >= Duration::from_secs(u64::from(*timeout_seconds))
        {
            // SAFETY: Child 创建独立进程组，负 PID 用于清理该任务组；生产任务另由 cgroup 监督。
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            interrupted = true;
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(100));
    };
    done.store(true, Ordering::SeqCst);
    let stdout = out
        .join()
        .map_err(|_| anyhow::anyhow!("package stdout capture failed"))??;
    let stderr = err
        .join()
        .map_err(|_| anyhow::anyhow!("package stderr capture failed"))??;
    evidence.diagnostic = if stderr.tail.is_empty() {
        stdout.tail
    } else {
        stderr.tail
    };
    ensure!(
        !interrupted,
        "package transaction interrupted; version alone is not completion proof: {}",
        evidence.diagnostic
    );
    evidence.transaction_exit = status.code();
    evidence.after = observed(manager, name, Some(version)).ok();
    if !status.success() {
        evidence.diagnostic = format!(
            "package manager exited with {status}: {}",
            evidence.diagnostic
        );
    }
    Ok(evidence)
}
