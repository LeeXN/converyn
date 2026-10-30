//! Linux 场景操作：sysctl 与 grubby 内核选择，不参与核心状态机。
use anyhow::{Context as _, Result, ensure};
use converyn_plugin_api::{self as api, Plugin};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::Ordering,
};
include!(concat!(env!("OUT_DIR"), "/identity.rs"));
mod deployment;
pub struct LinuxHost;
impl Plugin for LinuxHost {
    fn policy_paths(&self, policy: &Value) -> Result<api::PolicyPaths> {
        let mut writable = Vec::new();
        if let Some(paths) = policy.get("directories") {
            for path in paths
                .as_array()
                .context("directory grants must be an array")?
            {
                let path = path.as_str().context("directory grant must be a path")?;
                ensure!(
                    deployment::safe_directory(path),
                    "directory grant must be a normalized absolute non-root path"
                );
                writable.push(path.into());
            }
        }
        Ok(api::PolicyPaths {
            writable,
            protected: Vec::new(),
        })
    }
    fn descriptor(&self) -> api::Descriptor {
        api::Descriptor {
            identity: api::Identity {
                id: "converyn.linux-host".into(),
                version: "1.0.0".into(),
                contract: 1,
                sha256: CONTENT_SHA256.into(),
            },
            location: api::Location::Node,
            actions: [
                "sysctl",
                "select_kernel",
                "service",
                "directories",
                "swapoff",
                "modules",
                "initramfs",
                "kernel_arguments",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            description:
                "Linux state, explicit service activation and bounded deployment preparation".into(),
        }
    }
    fn compile(&self, action: &str, spec: Value) -> Result<api::Call> {
        if [
            "service",
            "directories",
            "swapoff",
            "modules",
            "initramfs",
            "kernel_arguments",
        ]
        .contains(&action)
        {
            let (mut resources, timeout_seconds) = deployment::compile(action, &spec)?;
            resources.sort();
            resources.dedup();
            return Ok(api::Call {
                plugin: self.descriptor().identity,
                action: action.into(),
                location: api::Location::Node,
                spec,
                resources,
                reconcilable: false,
                timeout_seconds,
            });
        }
        let mut resources = Vec::new();
        let reconcilable = match action {
            "sysctl" => {
                let key = spec["key"].as_str().context("sysctl key missing")?;
                let value = spec["value"].as_str().context("sysctl value missing")?;
                ensure!(
                    key.len() <= 128
                        && !key.is_empty()
                        && key.split('.').all(|s| !s.is_empty()
                            && s.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
                        && !matches!(key, "vm.drop_caches" | "kernel.sysrq"),
                    "invalid sysctl state key"
                );
                ensure!(
                    !value.is_empty()
                        && value.len() <= 1024
                        && value.split_whitespace().collect::<Vec<_>>().join(" ") == value
                        && value.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
                        && !value.contains(['#', ';']),
                    "invalid sysctl value"
                );
                ensure!(
                    spec["persist"].is_boolean() && spec.as_object().is_some_and(|m| m.len() == 3),
                    "invalid sysctl specification"
                );
                resources.push(format!("sysctl/{key}"));
                if spec["persist"] == true {
                    resources.push(format!("file/etc/sysctl.d/90-converyn-{key}.conf"));
                }
                true
            }
            "select_kernel" => {
                let release = spec["release"].as_str().context("kernel release missing")?;
                ensure!(
                    api::valid_id(release) && spec.as_object().is_some_and(|m| m.len() == 1),
                    "invalid kernel release"
                );
                resources.push("kernel-default".into());
                false
            }
            _ => anyhow::bail!("unknown Linux host action"),
        };
        resources.sort();
        Ok(api::Call {
            plugin: self.descriptor().identity,
            action: action.into(),
            location: api::Location::Node,
            spec,
            resources,
            reconcilable,
            timeout_seconds: 60,
        })
    }
    fn authorize(&self, call: &api::Call, policy: &Value, _project: &str) -> Result<()> {
        self.validate(call)?;
        if [
            "service",
            "directories",
            "swapoff",
            "modules",
            "initramfs",
            "kernel_arguments",
        ]
        .contains(&call.action.as_str())
        {
            return deployment::authorize(&call.action, &call.spec, policy);
        }
        match call.action.as_str() {
            "sysctl" => ensure!(
                policy["sysctls"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|key| key == &call.spec["key"])),
                "sysctl not granted locally"
            ),
            "select_kernel" => ensure!(
                policy["allow_kernel_selection"] == true,
                "kernel selection not granted locally"
            ),
            _ => anyhow::bail!("unknown host action"),
        };
        Ok(())
    }
    fn apply(&self, call: &api::Call, c: &api::Context<'_>) -> Result<api::Applied> {
        self.validate(call)?;
        ensure!(!c.cancel.load(Ordering::SeqCst), "host operation canceled");
        // SAFETY: geteuid 仅读取当前有效 UID；系统写入只允许 root 宿主。
        ensure!(
            unsafe { libc::geteuid() } == 0,
            "host plugin requires root supervisor"
        );
        if [
            "service",
            "directories",
            "swapoff",
            "modules",
            "initramfs",
            "kernel_arguments",
        ]
        .contains(&call.action.as_str())
        {
            return deployment::apply(&call.action, &call.spec, c);
        }
        match call.action.as_str() {
            "sysctl" => {
                let key = call.spec["key"].as_str().unwrap();
                let desired = call.spec["value"].as_str().unwrap();
                ensure!(
                    c.policy["sysctls"]
                        .as_array()
                        .is_some_and(|v| v.iter().any(|x| x == key)),
                    "sysctl not granted locally"
                );
                let path = format!("/proc/sys/{}", key.replace('.', "/"));
                let before = read(&path)?;
                let mut changed = before != desired;
                if changed {
                    let mut f = api::local::open(Path::new(&path), libc::O_WRONLY)?;
                    f.write_all(format!("{desired}\n").as_bytes())?;
                }
                let after = read(&path)?;
                ensure!(after == desired, "sysctl runtime verification failed");
                let persist = call.spec["persist"] == true;
                if persist {
                    changed |= c.host.ensure_file(
                        Path::new(&format!("/etc/sysctl.d/90-converyn-{key}.conf")),
                        format!("{key} = {desired}\n").as_bytes(),
                        0o644,
                    )?;
                }
                Ok(api::Applied {
                    changed,
                    payload: json!({"before":before,"after":after,"persistent":persist}),
                })
            }
            "select_kernel" => {
                ensure!(
                    c.policy["allow_kernel_selection"] == true,
                    "kernel selection not granted locally"
                );
                let image = format!("/boot/vmlinuz-{}", call.spec["release"].as_str().unwrap());
                ensure!(
                    std::fs::metadata(&image)?.is_file(),
                    "kernel image not installed"
                );
                let before = query(&["--default-kernel"])?;
                if before != image {
                    query(&["--set-default", &image])?;
                }
                let after = query(&["--default-kernel"])?;
                ensure!(after == image, "kernel boot selection verification failed");
                Ok(api::Applied {
                    changed: before != after,
                    payload: json!({"before":before,"after":after}),
                })
            }
            _ => unreachable!(),
        }
    }
    fn observe(&self, call: &api::Call, c: &api::Context<'_>) -> Result<Value> {
        self.validate(call)?;
        ensure!(
            call.action == "sysctl",
            "kernel selection does not automatically reconcile"
        );
        let key = call.spec["key"].as_str().unwrap();
        ensure!(
            c.policy["sysctls"]
                .as_array()
                .is_some_and(|v| v.iter().any(|x| x == key)),
            "sysctl not granted locally"
        );
        let persistent = if call.spec["persist"] == true {
            std::fs::read_to_string(format!("/etc/sysctl.d/90-converyn-{key}.conf"))
                .is_ok_and(|s| s == format!("{key} = {}\n", call.spec["value"].as_str().unwrap()))
        } else {
            false
        };
        Ok(
            json!({"after":read(&format!("/proc/sys/{}",key.replace('.',"/")))?,"persistent":persistent}),
        )
    }
    fn verify(&self, call: &api::Call, p: &Value) -> Result<bool> {
        Ok(match call.action.as_str() {
            "sysctl" => p["after"] == call.spec["value"] && p["persistent"] == call.spec["persist"],
            "select_kernel" => {
                p["after"]
                    == format!(
                        "/boot/vmlinuz-{}",
                        call.spec["release"].as_str().unwrap_or("")
                    )
            }
            _ => deployment::verify(&call.action, &call.spec, p),
        })
    }
}
fn read(path: &str) -> Result<String> {
    let f = api::local::open(Path::new(path), libc::O_RDONLY)?;
    let mut s = String::new();
    f.take(4097).read_to_string(&mut s)?;
    ensure!(s.len() <= 4096, "sysctl value too large");
    Ok(s.split_whitespace().collect::<Vec<_>>().join(" "))
}
fn query(args: &[&str]) -> Result<String> {
    let mut child = std::process::Command::new("/usr/sbin/grubby")
        .args(args)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "grubby failed");
            let mut s = String::new();
            child
                .stdout
                .take()
                .unwrap()
                .take(65537)
                .read_to_string(&mut s)?;
            ensure!(s.len() <= 65536, "grubby output too large");
            return Ok(s.trim().into());
        }
        if start.elapsed().as_secs() > 30 {
            child.kill()?;
            child.wait()?;
            anyhow::bail!("grubby timeout; inspect boot configuration");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
