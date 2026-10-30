//! 特权脚本只由 root 本地配置授权，不能由远端 Plan 提供 UID 或可写路径。
use crate::{
    executors::execution::{Mode, Policy},
    nodes::agent::Config,
};
use anyhow::{Context, Result, ensure};
use converyn_core::model::domain::Operation;
use std::path::PathBuf;

/// Validate declared write scopes before a workflow creates its business
/// directories. Existing ancestors must remain canonical and protected paths
/// are resolved even when their final components do not exist yet.
pub fn validate_write_scopes(paths: &[PathBuf], protected: &[&std::path::Path]) -> Result<()> {
    fn future(path: &std::path::Path, grant: bool) -> Result<PathBuf> {
        ensure!(
            path.is_absolute()
                && path.components().all(|c| matches!(
                    c,
                    std::path::Component::RootDir | std::path::Component::Normal(_)
                )),
            "write scope must be a normalized absolute path"
        );
        let mut parent = path.to_path_buf();
        let mut tail = Vec::new();
        loop {
            match std::fs::symlink_metadata(&parent) {
                Ok(_) => {
                    let canonical = std::fs::canonicalize(&parent)?;
                    ensure!(
                        !grant || canonical == parent,
                        "write scope traverses a symlink"
                    );
                    ensure!(
                        tail.is_empty() || canonical.is_dir(),
                        "write scope parent is not a directory"
                    );
                    let mut result = canonical;
                    for part in tail.iter().rev() {
                        result.push(part);
                    }
                    return Ok(result);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    tail.push(
                        parent
                            .file_name()
                            .context("write scope has no existing ancestor")?
                            .to_os_string(),
                    );
                    parent.pop();
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    for root in paths {
        ensure!(
            root != std::path::Path::new("/"),
            "write scope cannot be the filesystem root"
        );
        let root = future(root, true)?;
        for path in protected {
            let target = future(path, false)?;
            ensure!(
                !root.starts_with(&target) && !target.starts_with(&root),
                "write scope overlaps agent state or credentials"
            );
        }
    }
    Ok(())
}
/// 必须在领取 grant 之前调用；被拒绝的脚本不会取得修改授权。
pub fn policy(
    config: &Config,
    operation: &Operation,
    project: Option<&str>,
) -> Result<Option<Policy>> {
    let Operation::Exec {
        execution,
        environment,
        execution_profile,
        ..
    } = operation
    else {
        return Ok(config.execution.clone());
    };
    ensure!(
        execution_profile.is_none(),
        "historical execution_profile cannot execute; register a workflow with execution: host"
    );
    if execution.as_deref() != Some("host") {
        return Ok(config.execution.clone());
    }
    let project = project.context("privileged scripts require an explicit project")?;
    // The controller only grants immutable registered workflow operations. Local
    // profiles constrain privileges, not a per-release script digest allowlist.
    let _ = environment;
    ensure!(
        project == "default",
        "registered workflow namespace required"
    );
    ensure!(
        matches!(operation,Operation::Exec{environment,..} if environment.keys().all(|name|safe_privileged_name(name))),
        "unsafe privileged environment"
    );
    let mut selected = config
        .execution
        .clone()
        .context("privileged scripts require systemd task supervision")?;
    ensure!(
        selected.mode == Mode::Systemd,
        "privileged scripts cannot use process mode"
    );
    selected.home = None;
    selected.uid = 0;
    selected.gid = 0;
    selected.privileged = true;
    selected.writable_paths.clear();
    selected.validate()?;
    Ok(Some(selected))
}

/// 特权解释器/动态加载器的启动入口不能被远端参数替换，即使错误配置了允许名单。
pub fn safe_privileged_name(name: &str) -> bool {
    converyn_core::orchestration::workflow::safe_environment(name)
}
