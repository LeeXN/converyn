//! Typed host state operations. Reboot completion requires a different kernel
//! boot identity plus the requested kernel release, never a revision counter.
use crate::executors::native::{Evidence, Policy};
use crate::executors::runner::{Outcome, Receipt};
use anyhow::{Context, Result, ensure};
use converyn_core::model::domain::Operation;
pub use converyn_core::model::host::Reboot;
use std::{path::Path, sync::atomic::AtomicBool};
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
pub fn boot() -> Result<String> {
    Ok(std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .into())
}
pub fn kernel() -> Result<String> {
    let mut value = std::mem::MaybeUninit::<libc::utsname>::uninit();
    ensure!(
        // SAFETY: 传入为 utsname 分配的有效可写存储；仅在返回成功后读取。
        unsafe { libc::uname(value.as_mut_ptr()) } == 0,
        "cannot observe running kernel"
    );
    // SAFETY: 上一系统调用已成功初始化该结构体，失败分支不会到达这里。
    let value = unsafe { value.assume_init() };
    // SAFETY: 指针来自成功的 libc 调用，其 NUL 结尾存储在转换期间保持有效。
    Ok(unsafe { std::ffi::CStr::from_ptr(value.release.as_ptr()) }
        .to_str()?
        .into())
}
pub fn reboot(
    journal: &Path,
    id: &str,
    policy: &Policy,
    operation: &Operation,
    previous: Option<Receipt>,
) -> Result<Receipt> {
    let Operation::Reboot {
        requested,
        recovery_timeout_seconds,
        expected_kernel,
    } = operation
    else {
        anyhow::bail!("not reboot")
    };
    let current = boot()?;
    if let Some(mut receipt) = previous {
        if receipt.outcome != Outcome::Running {
            return Ok(receipt);
        }
        let evidence = receipt.native.as_mut().context("missing reboot evidence")?;
        let observed = evidence.reboot.as_mut().context("missing reboot intent")?;
        if current != observed.before_boot {
            observed.after_boot = Some(current.clone());
            observed.kernel = Some(kernel()?);
            receipt.boot_id = current;
            receipt.outcome = if expected_kernel
                .as_ref()
                .is_none_or(|expected| observed.kernel.as_ref() == Some(expected))
            {
                Outcome::Succeeded
            } else {
                evidence.error = Some("booted kernel differs from approved target".into());
                Outcome::Failed
            };
        }
        if now_ms().saturating_sub(observed.requested_ms)
            > u64::from(*recovery_timeout_seconds) * 1000
        {
            receipt.outcome = Outcome::NeedsAttention;
            evidence.error = Some(
                "reboot recovery deadline exceeded; inspect recorded boot/kernel evidence".into(),
            );
        }
        if receipt.outcome != Outcome::Running {
            crate::nodes::agent::durable(journal, &serde_json::to_vec(&receipt)?)?;
        }
        return Ok(receipt);
    }
    let mut evidence = Evidence::empty(operation);
    evidence.reboot = Some(Reboot {
        before_boot: current.clone(),
        after_boot: None,
        kernel: None,
        requested_ms: now_ms(),
    });
    let mut receipt = Receipt {
        outputs: Default::default(),
        diagnostic: None,
        operation_id: id.into(),
        script_sha256: String::new(),
        timeout_seconds: 0,
        exit_code: None,
        outcome: Outcome::Running,
        boot_id: current,
        native: None,
        logs: None,
        execution: None,
    };
    if !requested {
        let seen = evidence.reboot.as_mut().unwrap();
        seen.after_boot = Some(receipt.boot_id.clone());
        seen.kernel = Some(kernel()?);
        receipt.outcome = if expected_kernel
            .as_ref()
            .is_none_or(|k| seen.kernel.as_ref() == Some(k))
        {
            Outcome::Succeeded
        } else {
            evidence.error = Some("current kernel differs from requested condition".into());
            Outcome::Failed
        };
        receipt.diagnostic = Some("reboot_not_requested".into());
        receipt.native = Some(Box::new(evidence));
        crate::nodes::agent::durable(journal, &serde_json::to_vec(&receipt)?)?;
        return Ok(receipt);
    }
    // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
    if !policy.allow_reboot || unsafe { libc::geteuid() } != 0 {
        receipt.outcome = Outcome::Failed;
        evidence.error = Some("reboot requires explicit local policy and root supervisor".into());
        receipt.native = Some(Box::new(evidence));
        crate::nodes::agent::durable(journal, &serde_json::to_vec(&receipt)?)?;
        return Ok(receipt);
    }
    receipt.native = Some(Box::new(evidence));
    crate::nodes::agent::durable(journal, &serde_json::to_vec(&receipt)?)?;
    // Persist before the only request. A lost acknowledgment never resends reboot.
    if let Err(e) = crate::executors::service::command(
        false,
        &["reboot", "--no-block"],
        &AtomicBool::new(false),
    ) {
        receipt.outcome = Outcome::NeedsAttention;
        receipt.native.as_mut().unwrap().error = Some(format!("reboot request unconfirmed: {e}"));
        crate::nodes::agent::durable(journal, &serde_json::to_vec(&receipt)?)?;
    }
    Ok(receipt)
}

pub fn hardware(explicit: Option<&Path>) -> Result<Option<String>> {
    use sha2::{Digest, Sha256};
    if let Some(path) = explicit {
        use std::io::Read;
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        ensure!(
            path.is_absolute(),
            "hardware identity path must be absolute"
        );
        for ancestor in path.parent().into_iter().flat_map(|p| p.ancestors()) {
            let m = std::fs::symlink_metadata(ancestor)?;
            ensure!(
                m.is_dir()
                    // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
                    && (m.uid() == 0 || m.uid() == unsafe { libc::geteuid() })
                    && (m.mode() & 0o022 == 0 || m.mode() & 0o1000 != 0),
                "hardware identity has an untrusted ancestor"
            );
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        let m = file.metadata()?;
        ensure!(
            m.is_file()
                && m.mode() & 0o022 == 0
                // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
                && (m.uid() == 0 || m.uid() == unsafe { libc::geteuid() }),
            "hardware identity must be a trusted regular file"
        );
        let mut value = Vec::new();
        file.take(4097).read_to_end(&mut value)?;
        ensure!(
            !value.is_empty() && value.len() <= 4096,
            "invalid explicit hardware identity file"
        );
        return Ok(Some(hex::encode(Sha256::digest(
            [b"explicit:".as_slice(), value.as_slice()].concat(),
        ))));
    }
    for (kind, path) in [
        ("dmi:", "/sys/class/dmi/id/product_uuid"),
        (
            "device-tree:",
            "/sys/firmware/devicetree/base/serial-number",
        ),
    ] {
        if let Ok(value) = std::fs::read_to_string(path) {
            let value = value
                .trim_matches(|c: char| c.is_whitespace() || c == '\0')
                .to_lowercase();
            if !value.is_empty()
                && value.len() <= 4096
                && value
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .any(|c| c != '0' && c != 'f')
            {
                return Ok(Some(hex::encode(Sha256::digest(format!("{kind}{value}")))));
            }
        }
    }
    Ok(None)
}
