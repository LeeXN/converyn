//! OP-owned source packaging. The controller receives content, never local paths
//! or Git credentials, and all source reads are confined to the manifest root.
use anyhow::{Context, Result, ensure};
use converyn_core::orchestration::workflow::Revision;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn content(root: &Path, relative: &str) -> Result<String> {
    let path = Path::new(relative);
    ensure!(
        !path.is_absolute()
            && path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "source path must stay inside workflow directory"
    );
    let mut current = PathBuf::from(root);
    for part in path.components() {
        current.push(part);
        ensure!(
            !std::fs::symlink_metadata(&current)?
                .file_type()
                .is_symlink(),
            "workflow source symlinks are not accepted"
        );
    }
    ensure!(
        current.canonicalize()?.starts_with(root) && std::fs::metadata(&current)?.len() <= 65536,
        "workflow source outside root or exceeds 64 KiB"
    );
    Ok(std::fs::read_to_string(current)?)
}
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).trim().to_string())
}
pub fn package(file: &Path) -> Result<Revision> {
    Ok(package_artifacts(file)?.revision)
}
pub struct Packaged {
    pub revision: Revision,
    pub artifacts: BTreeMap<String, PathBuf>,
}
pub fn package_artifacts(file: &Path) -> Result<Packaged> {
    package_artifacts_with_progress(file, |_, _, _| {})
}
pub fn package_artifacts_with_progress(
    file: &Path,
    mut progress: impl FnMut(&Path, u64, u64),
) -> Result<Packaged> {
    let entry = if file.is_dir() {
        file.join("workflow.yaml")
    } else {
        file.to_path_buf()
    };
    ensure!(
        !std::fs::symlink_metadata(&entry)?.file_type().is_symlink(),
        "workflow entry symlinks are not accepted"
    );
    let file = entry.canonicalize()?;
    let root = file.parent().context("manifest has no parent")?;
    ensure!(
        std::fs::metadata(&file)?.len() <= 1024 * 1024,
        "workflow manifest exceeds 1 MiB"
    );
    let mut value: Value = serde_yaml::from_slice(&std::fs::read(&file)?)?;
    if let Some(steps) = value.pointer("/playbook/steps").and_then(Value::as_array) {
        ensure!(
            steps
                .iter()
                .all(|s| s.pointer("/operation/execution_profile").is_none()),
            "execution_profile was removed; declare execution: host or user"
        );
    }
    let mut artifacts = BTreeMap::new();
    super::workflow_source::compose(root, &mut value, &mut artifacts, &mut progress)?;
    if let Some(path) = value
        .get("values")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        let defaults: Value = serde_yaml::from_str(&content(root, &path)?)?;
        ensure!(
            defaults.is_object(),
            "workflow values must contain an object"
        );
        value["values"] = defaults;
    }
    let mut definitions = BTreeMap::new();
    for (key, field) in [("scripts", "script_sha256"), ("files", "sha256")] {
        let references = value
            .as_object_mut()
            .context("workflow must be an object")?
            .remove(key)
            .unwrap_or_else(|| serde_json::json!({}));
        let references: BTreeMap<String, String> = serde_json::from_value(references)?;
        for (step_id, path) in references {
            let bytes = content(root, &path)?;
            let sha = hex::encode(Sha256::digest(bytes.as_bytes()));
            let steps = value
                .pointer_mut("/playbook/steps")
                .and_then(Value::as_array_mut)
                .context("playbook steps missing")?;
            let step = steps
                .iter_mut()
                .find(|s| s["id"] == step_id)
                .context("definition step does not exist")?;
            let op = step["operation"]
                .as_object_mut()
                .context("step operation missing")?;
            ensure!(
                (key == "scripts" && op.get("kind").and_then(Value::as_str) == Some("exec"))
                    || (key == "files" && op.get("kind").and_then(Value::as_str) == Some("file")),
                "definition kind mismatch"
            );
            op.insert(field.into(), Value::String(sha.clone()));
            definitions.insert(sha, bytes);
        }
    }
    let mut revision: Revision = serde_json::from_value(value)?;
    revision.definitions.extend(definitions);
    if let Some(commit) = git(root, &["rev-parse", "HEAD"]) {
        revision.provenance.insert("git_commit".into(), commit);
        revision.provenance.insert(
            "git_dirty".into(),
            (!git(root, &["status", "--porcelain"])
                .unwrap_or_default()
                .is_empty())
            .to_string(),
        );
        if let Some(origin) = git(root, &["remote", "get-url", "origin"]) {
            let public = if let Ok(mut url) = reqwest::Url::parse(&origin) {
                let _ = url.set_username("");
                let _ = url.set_password(None);
                url.set_query(None);
                url.set_fragment(None);
                url.to_string()
            } else {
                origin.rsplit('@').next().unwrap_or("local").to_string()
            };
            revision.provenance.insert("git_repository".into(), public);
        }
    }
    revision.validate()?;
    for step in &revision.playbook.steps {
        if let Some(artifact) = step.operation.artifact() {
            // Parameter bindings can supply required artifact fields at admission.
            // Validate fixed references here without rejecting those templates.
            let parameterized = revision.parameters.values().any(|parameter| {
                parameter.bindings.iter().any(|binding| {
                    binding.step_id == step.id && binding.path.starts_with("/artifact/")
                })
            });
            if !parameterized {
                artifact.validate().with_context(|| {
                    format!(
                        "workflow {} step {} operation.artifact: supply the release payload's 64-character lowercase SHA-256, byte size and source locations",
                        file.display(), step.id
                    )
                })?;
            }
        }
    }
    Ok(Packaged {
        revision,
        artifacts,
    })
}

/// Describe declared needs without claiming that remote Agent configuration was checked.
pub fn requirements(revision: &Revision) -> Value {
    let steps: Vec<_> = revision
        .playbook
        .steps
        .iter()
        .map(|step| {
            let operation = &step.operation;
            let node_operation=operation.location()==converyn_plugin_api::Location::Node;
            let mut needs =
                serde_json::json!({"step":step.id,"hosts":step.hosts,"operation":operation,"executor":if node_operation {"node"}else{"controller"}});
            if node_operation && let Some(inventory) = &revision.inventory {
                needs["nodes"] = serde_json::json!(
                    inventory
                        .select(step.hosts.as_deref().unwrap_or("all"))
                        .unwrap_or_default()
                );
            }
            needs
        })
        .collect();
    serde_json::json!({"agent_configuration":"not_verified","platform_scope":"not_verified","steps":steps,"fixed_references":revision.fixed})
}
