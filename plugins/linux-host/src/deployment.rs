//! Bounded host actions used by deployment workflows. Calls name resources;
//! they never accept shell text, executables, environment overrides or wildcards.
use anyhow::{Context as _, Result, ensure};
use converyn_plugin_api as api;
use serde_json::{Value, json};
use std::{io::Read, path::Path, sync::atomic::Ordering};

pub fn compile(action: &str, s: &Value) -> Result<(Vec<String>, u32)> {
    let resources = match action {
        "service" => {
            let name = s["name"].as_str().context("service name missing")?;
            ensure!(
                api::valid_id(name) && !name.starts_with('-') && !name.contains('@'),
                "invalid service name"
            );
            ensure!(
                matches!(
                    s["action"].as_str(),
                    Some("start" | "stop" | "restart" | "reload" | "enable")
                ) && (s["enabled"].is_boolean() || s["enabled"].is_null())
                    && s["optional"].is_boolean()
                    && s.as_object().is_some_and(|m| (m.len() == 4 || m.len() == 5)
                        && m.keys().all(|k| [
                            "name",
                            "action",
                            "enabled",
                            "optional",
                            "requested"
                        ]
                        .contains(&k.as_str()))
                        && (!m.contains_key("requested") || s["requested"].is_boolean())),
                "invalid service action"
            );
            vec![format!("service/{name}")]
        }
        "directories" => {
            let paths = s["paths"].as_array().context("directory list missing")?;
            ensure!(
                !paths.is_empty()
                    && paths.len() <= 64
                    && s.as_object().is_some_and(|m| m.len() == 1),
                "invalid directory list"
            );
            let mut resources = Vec::new();
            for p in paths {
                let p = p.as_str().context("invalid directory")?;
                ensure!(
                    safe_directory(p),
                    "directory must be a normalized absolute non-root path"
                );
                resources.push(format!("directory{p}"));
            }
            resources
        }
        "swapoff" => {
            ensure!(
                s.as_object().is_some_and(|m| m.is_empty()),
                "invalid swap operation"
            );
            vec!["host/swap".into()]
        }
        "modules" => {
            let names = s["names"].as_array().context("module list missing")?;
            ensure!(
                !names.is_empty()
                    && names.len() <= 16
                    && s.as_object().is_some_and(|m| m.len() == 1),
                "invalid module list"
            );
            for n in names {
                ensure!(n.as_str().is_some_and(api::valid_id), "invalid module name");
            }
            names
                .iter()
                .map(|n| format!("module/{}", n.as_str().unwrap()))
                .collect()
        }
        "initramfs" => {
            ensure!(
                s.as_object()
                    .is_some_and(|m| m.is_empty() || (m.len() == 1 && s["requested"].is_boolean())),
                "invalid initramfs operation"
            );
            vec!["host/initramfs".into()]
        }
        "kernel_arguments" => {
            ensure!(
                s["method"]
                    .as_str()
                    .is_some_and(|x| ["grubby", "grub"].contains(&x))
                    && s["arguments"].as_array().is_some_and(|a| !a.is_empty()
                        && a.iter()
                            .all(|v| matches!(v.as_str(), Some("intel_iommu=on" | "iommu=pt"))))
                    && s.as_object().is_some_and(|m| m.len() == 2),
                "invalid kernel argument request"
            );
            vec![
                "host/kernel-arguments".into(),
                "file/etc/default/grub".into(),
            ]
        }
        _ => anyhow::bail!("unknown deployment action"),
    };
    Ok((resources, if action == "initramfs" { 900 } else { 120 }))
}
pub(crate) fn safe_directory(p: &str) -> bool {
    let path = Path::new(p);
    path.is_absolute()
        && p.len() > 1
        && p.len() <= 4096
        && p.bytes().all(|b| b.is_ascii_graphic())
        && p.split('/')
            .skip(1)
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn authorize(action: &str, s: &Value, p: &Value) -> Result<()> {
    compile(action, s)?;
    match action {
        "service" => ensure!(
            p["services"]
                .as_array()
                .is_some_and(|v| v.contains(&s["name"])),
            "service action not granted"
        ),
        "directories" => {
            for path in s["paths"].as_array().unwrap() {
                ensure!(
                    p["directories"]
                        .as_array()
                        .is_some_and(|v| v.contains(path)),
                    "directory not granted"
                );
            }
        }
        "modules" => {
            for name in s["names"].as_array().unwrap() {
                ensure!(
                    p["modules"].as_array().is_some_and(|v| v.contains(name)),
                    "module not granted"
                );
            }
        }
        "swapoff" => ensure!(p["allow_swapoff"] == true, "swap change not granted"),
        "kernel_arguments" => ensure!(
            p["allow_kernel_arguments"] == true,
            "kernel arguments not granted"
        ),
        "initramfs" => ensure!(p["allow_initramfs"] == true, "initramfs update not granted"),
        _ => anyhow::bail!("unknown deployment action"),
    }
    Ok(())
}
fn run(
    program: &str,
    args: &[&str],
    cancel: &std::sync::atomic::AtomicBool,
    seconds: u64,
) -> Result<String> {
    let observation = program == "/usr/bin/systemctl" && args.first() == Some(&"show");
    let mut child = std::process::Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(if observation {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let began = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "host command failed; inspect host state");
            let mut data = String::new();
            if let Some(stdout) = child.stdout.take() {
                stdout.take(65537).read_to_string(&mut data)?;
            }
            ensure!(data.len() <= 65536, "host observation too large");
            return Ok(data.trim().into());
        }
        if cancel.load(Ordering::SeqCst) || began.elapsed().as_secs() > seconds {
            child.kill()?;
            child.wait()?;
            anyhow::bail!("host command interrupted; inspect original operation");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
fn systemctl(args: &[&str], c: &api::Context<'_>) -> Result<String> {
    run("/usr/bin/systemctl", args, c.cancel, 90)
}
fn directories(path: &Path) -> Result<bool> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let mut changed = false;
    let mut parent = std::fs::File::open("/")?;
    for part in path.components().skip(1) {
        let std::path::Component::Normal(part) = part else {
            anyhow::bail!("directory must be normalized");
        };
        let name = std::ffi::CString::new(part.as_encoded_bytes())?;
        // SAFETY: parent holds a live directory fd; name is a live CString.
        let mut fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
            // SAFETY: mkdirat receives a live directory fd and CString.
            if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) } == 0 {
                changed = true;
            } else {
                ensure!(
                    std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists,
                    "cannot create granted directory"
                );
            }
            // SAFETY: same live fd/name; O_NOFOLLOW rejects a raced symlink.
            fd = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
        }
        ensure!(fd >= 0, "unsafe or inaccessible directory component");
        // SAFETY: successful openat returned a new owned fd.
        parent = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    Ok(changed)
}

pub fn apply(action: &str, s: &Value, c: &api::Context<'_>) -> Result<api::Applied> {
    authorize(action, s, c.policy)?;
    ensure!(!c.cancel.load(Ordering::SeqCst), "host action canceled");
    if s["requested"] == false {
        return Ok(api::Applied {
            changed: false,
            payload: json!({"skipped":true}),
        });
    }
    let (changed, payload) = match action {
        "directories" => {
            let mut changed = false;
            for path in s["paths"].as_array().unwrap() {
                changed |= directories(Path::new(path.as_str().unwrap()))?;
            }
            (changed, json!({"paths":s["paths"],"verified":true}))
        }
        "service" => {
            let name = s["name"].as_str().unwrap();
            systemctl(&["daemon-reload"], c)?;
            let before = systemctl(&["show", "--property=LoadState", "--value", name], c)?;
            if before == "not-found" && s["optional"] == true {
                return Ok(api::Applied {
                    changed: false,
                    payload: json!({"name":name,"absent":true}),
                });
            }
            ensure!(before == "loaded", "service unit not loaded");
            systemctl(&["daemon-reload"], c)?;
            let enabled = systemctl(&["show", "--property=UnitFileState", "--value", name], c)?;
            if s["enabled"] == true
                && !matches!(
                    enabled.as_str(),
                    "enabled" | "static" | "indirect" | "generated"
                )
            {
                systemctl(&["enable", name], c)?;
            }
            if s["enabled"] == false && enabled == "enabled" {
                systemctl(&["disable", name], c)?;
            }
            let operation = s["action"].as_str().unwrap();
            if ["ssh", "sshd"].contains(&name) && operation != "stop" {
                run("/usr/sbin/sshd", &["-t"], c.cancel, 30)?;
            }
            if operation != "enable" {
                systemctl(&[operation, name], c)?;
            }
            let active = systemctl(&["show", "--property=ActiveState", "--value", name], c)?;
            let enabled = systemctl(&["show", "--property=UnitFileState", "--value", name], c)?;
            let observed =
                json!({"name":name,"active":active,"enabled":enabled,"action":operation});
            ensure!(verify(action, s, &observed), "service postcondition failed");
            (true, observed)
        }
        "swapoff" => {
            let before = std::fs::read_to_string("/proc/swaps")?;
            let changed = before.lines().count() > 1;
            if changed {
                run("/usr/sbin/swapoff", &["--all"], c.cancel, 90)?;
            }
            let disabled = std::fs::read_to_string("/proc/swaps")?.lines().count() <= 1;
            ensure!(disabled, "swap is still active");
            (changed, json!({"disabled":disabled}))
        }
        "modules" => {
            for n in s["names"].as_array().unwrap() {
                run(
                    "/usr/sbin/modprobe",
                    &["--", n.as_str().unwrap()],
                    c.cancel,
                    90,
                )?;
            }
            (true, json!({"names":s["names"],"verified":true}))
        }
        "kernel_arguments" => {
            let cpu = std::fs::read_to_string("/proc/cpuinfo")?;
            if !cpu.contains("GenuineIntel") {
                return Ok(api::Applied {
                    changed: false,
                    payload: json!({"not_intel":true}),
                });
            }
            let arguments = s["arguments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" ");
            if s["method"] == "grubby" {
                run(
                    "/usr/sbin/grubby",
                    &["--update-kernel=ALL", "--args", &arguments],
                    c.cancel,
                    90,
                )?;
            } else {
                let before = std::fs::read_to_string("/etc/default/grub")?;
                let mut lines = Vec::new();
                let mut found = false;
                for line in before.lines() {
                    if let Some(value) = line.strip_prefix("GRUB_CMDLINE_LINUX_DEFAULT=") {
                        let mut value = value.trim_matches('"').to_owned();
                        for arg in arguments.split_whitespace() {
                            if !value.split_whitespace().any(|v| v == arg) {
                                value.push(' ');
                                value.push_str(arg);
                            }
                        }
                        lines.push(format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"", value.trim()));
                        found = true;
                    } else {
                        lines.push(line.to_owned());
                    }
                }
                if !found {
                    lines.push(format!("GRUB_CMDLINE_LINUX_DEFAULT=\"{arguments}\""));
                }
                c.host.ensure_file(
                    Path::new("/etc/default/grub"),
                    (lines.join("\n") + "\n").as_bytes(),
                    0o644,
                )?;
                run("/usr/sbin/update-grub", &[], c.cancel, 90)?;
            }
            (true, json!({"arguments":s["arguments"],"configured":true}))
        }
        "initramfs" => {
            if Path::new("/usr/sbin/update-initramfs").exists() {
                run("/usr/sbin/update-initramfs", &["-u"], c.cancel, 850)?;
            } else {
                run("/usr/bin/dracut", &["--force"], c.cancel, 850)?;
            }
            (true, json!({"updated":true}))
        }
        _ => anyhow::bail!("unknown deployment action"),
    };
    Ok(api::Applied { changed, payload })
}
pub fn verify(action: &str, s: &Value, p: &Value) -> bool {
    if s["requested"] == false {
        return p["skipped"] == true;
    }
    match action {
        "service" => {
            if s["optional"] == true && p["absent"] == true {
                return p["name"] == s["name"];
            }
            p["name"] == s["name"]
                && p["action"] == s["action"]
                && (s["action"] == "enable"
                    || if s["action"] == "stop" {
                        p["active"] == "inactive"
                    } else {
                        p["active"] == "active"
                    })
                && (s["enabled"].is_null()
                    || (s["enabled"] == false
                        && matches!(
                            p["enabled"].as_str(),
                            Some("disabled" | "static" | "indirect" | "generated")
                        ))
                    || (s["enabled"] == true
                        && matches!(
                            p["enabled"].as_str(),
                            Some("enabled" | "static" | "indirect" | "generated")
                        )))
        }
        "directories" => p["paths"] == s["paths"] && p["verified"] == true,
        "swapoff" => p["disabled"] == true,
        "modules" => p["names"] == s["names"] && p["verified"] == true,
        "initramfs" => p["updated"] == true,
        "kernel_arguments" => {
            p["not_intel"] == true || (p["arguments"] == s["arguments"] && p["configured"] == true)
        }
        _ => false,
    }
}

#[cfg(test)]
mod directory_tests {
    use super::*;
    #[test]
    fn generic_directories_are_idempotent_and_never_follow_symlinks() {
        let root = std::env::temp_dir().join(format!(
            "converyn-host-directories-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let nested = root.join("application/data");
        assert!(directories(&nested).unwrap());
        assert!(!directories(&nested).unwrap());
        let outside = root.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("alias")).unwrap();
        assert!(directories(&root.join("alias/escaped")).is_err());
        assert!(!outside.join("escaped").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
