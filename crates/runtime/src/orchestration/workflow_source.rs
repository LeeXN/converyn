//! Declarative source composition; no external programs or application-specific translators.
use super::workflows::content;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

fn references(
    value: &Value,
    document: &Value,
    item: Option<&Value>,
    depth: usize,
) -> Result<Value> {
    resolve(value, document, item, depth, &mut (1024 * 1024))
}
fn resolve(
    value: &Value,
    document: &Value,
    item: Option<&Value>,
    depth: usize,
    remaining: &mut usize,
) -> Result<Value> {
    ensure!(depth < 32, "source reference recursion limit");
    let charge = match value {
        Value::String(s) => s.len() + 2,
        Value::Object(m) => m.keys().map(|k| k.len() + 3).sum::<usize>() + 2,
        _ => 4,
    };
    *remaining = remaining
        .checked_sub(charge)
        .context("source reference expansion exceeds 1 MiB")?;
    if let Some(object) = value.as_object() {
        if object.len() == 1 {
            if let Some(literal) = object.get("$literal") {
                *remaining = remaining
                    .checked_sub(serde_json::to_vec(literal)?.len())
                    .context("literal source value exceeds 1 MiB")?;
                return Ok(literal.clone());
            }
            if let Some(parts) = object.get("$concat").and_then(Value::as_array) {
                let strings = parts
                    .iter()
                    .map(|p| {
                        resolve(p, document, item, depth + 1, remaining)?
                            .as_str()
                            .map(str::to_owned)
                            .context("concatenation requires strings")
                    })
                    .collect::<Result<Vec<_>>>()?;
                return Ok(Value::String(strings.concat()));
            }
            for key in ["$ref", "$item"] {
                if let Some(pointer) = object.get(key).and_then(Value::as_str) {
                    // URI/fragment references are ordinary payload data (e.g.
                    // a schema document), not source-document JSON Pointers.
                    if !pointer.is_empty() && !pointer.starts_with('/') {
                        continue;
                    }
                    let origin = if key == "$item" {
                        item.context("item reference outside for_each")?
                    } else {
                        document
                    };
                    let selected = origin
                        .pointer(pointer)
                        .context("source reference does not exist")?;
                    return resolve(selected, document, item, depth + 1, remaining);
                }
            }
        }
        return object
            .iter()
            .map(|(k, v)| Ok((k.clone(), resolve(v, document, item, depth + 1, remaining)?)))
            .collect::<Result<serde_json::Map<_, _>>>()
            .map(Value::Object);
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .map(|v| resolve(v, document, item, depth + 1, remaining))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array);
    }
    Ok(value.clone())
}

pub(super) fn compose(
    root: &Path,
    value: &mut Value,
    payloads: &mut BTreeMap<String, std::path::PathBuf>,
    progress: &mut impl FnMut(&Path, u64, u64),
) -> Result<()> {
    for name in ["inventory", "values", "artifacts"] {
        if let Some(path) = value.get(name).and_then(Value::as_str).map(str::to_owned) {
            value[name] = serde_yaml::from_str(&content(root, &path)?)?;
        }
    }
    local_artifacts(root, value, payloads, progress)?;
    if value.get("inventory").is_some() && value.get("batch_size").is_none() {
        value["batch_size"] = json!(
            value
                .pointer("/inventory/nodes")
                .and_then(Value::as_array)
                .context("inventory nodes missing")?
                .len()
        );
    }
    let document = value.clone();
    let mut expanded = Vec::new();
    let mut expanded_bytes = 0usize;
    for step in value
        .pointer("/playbook/steps")
        .and_then(Value::as_array)
        .context("playbook steps missing")?
    {
        if let Some(pointer) = step.get("for_each").and_then(Value::as_str) {
            let items = document
                .pointer(pointer)
                .and_then(Value::as_array)
                .context("for_each must reference an array")?;
            ensure!(items.len() <= 300, "source expansion too large");
            ensure!(
                !items.is_empty() || step.get("allow_empty").and_then(Value::as_bool) == Some(true),
                "empty source expansion requires allow_empty"
            );
            for (index, item) in items.iter().enumerate() {
                let mut step = references(step, &document, Some(item), 0)?;
                step.as_object_mut().unwrap().remove("for_each");
                if step.get("hosts").is_none() {
                    step.as_object_mut().unwrap().remove("allow_empty");
                }
                step["id"] = json!(format!(
                    "{}-{index}",
                    step["id"].as_str().context("step id missing")?
                ));
                expanded_bytes += serde_json::to_vec(&step)?.len();
                ensure!(expanded_bytes <= 1024 * 1024, "expanded steps exceed 1 MiB");
                expanded.push(step);
            }
        } else {
            let resolved = references(step, &document, None, 0)?;
            expanded_bytes += serde_json::to_vec(&resolved)?.len();
            ensure!(expanded_bytes <= 1024 * 1024, "expanded steps exceed 1 MiB");
            expanded.push(resolved);
        }
    }
    ensure!(expanded.len() <= 4096, "source step expansion too large");
    for step in &mut expanded {
        let op = step
            .get_mut("operation")
            .context("step operation missing")?;
        if op["kind"] == "plugin" && op.get("call").is_none() {
            ensure!(
                op.as_object().is_some_and(|m| m
                    .keys()
                    .all(|k| ["kind", "id", "action", "spec"].contains(&k.as_str()))),
                "unknown plugin source field"
            );
            let plugin = op["id"].as_str().context("plugin id missing")?;
            let action = op["action"].as_str().context("plugin action missing")?;
            let call = crate::plugins::registry()
                .get(plugin)
                .context("plugin not built in")?
                .compile(action, op["spec"].clone())?;
            *op = json!({"kind":"plugin","call":call});
        }
        let source = if op["kind"] == "exec" {
            op.get("script")
        } else if op["kind"] == "file" {
            op.get("source")
        } else {
            None
        }
        .and_then(Value::as_str)
        .map(str::to_owned);
        let text = if let Some(path) = source {
            Some(content(root, &path)?)
        } else {
            op.get("content").and_then(Value::as_str).map(str::to_owned)
        };
        if let Some(text) = text {
            ensure!(
                op["kind"] == "exec" || op["kind"] == "file",
                "inline content kind mismatch"
            );
            let hash = hex::encode(Sha256::digest(text.as_bytes()));
            let field = if op["kind"] == "exec" {
                "script_sha256"
            } else {
                "sha256"
            };
            op[field] = json!(hash);
            for key in ["script", "source", "content"] {
                op.as_object_mut().unwrap().remove(key);
            }
            if value.get("definitions").is_none() {
                value["definitions"] = json!({});
            }
            value["definitions"][hash] = json!(text);
        }
    }
    value["playbook"]["steps"] = Value::Array(expanded);
    if let Some(inventory) = value.get("inventory").cloned() {
        value["inventory"] = references(&inventory, &document, None, 0)?;
    }
    if let Some(fixed) = value.get("fixed").cloned() {
        value["fixed"] = references(&fixed, &document, None, 0)?;
    }
    // Text and JSON files can be declared inline without a generated intermediary.
    let files = value
        .get("files")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (id, source) in files {
        if source.is_string() {
            continue;
        }
        let text = if let Some(pointer) = source.get("json").and_then(Value::as_str) {
            serde_json::to_string(&references(
                document.pointer(pointer).context("JSON source missing")?,
                &document,
                None,
                0,
            )?)?
        } else {
            source
                .get("text")
                .and_then(Value::as_str)
                .context("file source requires path, text or json")?
                .to_owned()
        };
        let hash = hex::encode(Sha256::digest(text.as_bytes()));
        let step = value["playbook"]["steps"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["id"] == id)
            .context("file step missing")?;
        ensure!(
            step["operation"]["kind"] == "file",
            "file source kind mismatch"
        );
        step["operation"]["sha256"] = json!(hash);
        if value.get("definitions").is_none() {
            value["definitions"] = json!({});
        }
        value["definitions"][&hash] = json!(text);
        value["files"].as_object_mut().unwrap().remove(&id);
    }
    let includes: Vec<String> = serde_json::from_value(
        value
            .get("package")
            .and_then(|p| p.get("include"))
            .cloned()
            .unwrap_or(json!([])),
    )?;
    if let Some(package) = value.get("package") {
        ensure!(
            package
                .as_object()
                .is_some_and(|p| p.len() == 1 && p.contains_key("include")),
            "package requires only an include list"
        );
    }
    let mut bundle = BTreeMap::new();
    for path in includes {
        collect(root, &path, &mut bundle, value)?;
    }
    if !bundle.is_empty() {
        value["bundle"] = serde_json::to_value(bundle)?;
    }
    value.as_object_mut().unwrap().remove("package");
    value.as_object_mut().unwrap().remove("artifacts");
    Ok(())
}

fn local_artifacts(
    root: &Path,
    value: &mut Value,
    payloads: &mut BTreeMap<String, std::path::PathBuf>,
    progress: &mut impl FnMut(&Path, u64, u64),
) -> Result<()> {
    if value
        .as_object()
        .is_some_and(|object| object.len() == 1 && object.contains_key("$literal"))
    {
        return Ok(());
    }
    if let Some(object) = value.as_object()
        && object.len() == 1
        && let Some(relative) = object.get("$artifact").and_then(Value::as_str)
    {
        ensure!(
            converyn_core::orchestration::inventory::valid_package_path(relative),
            "artifact path must stay inside workflow directory"
        );
        let mut path = root.to_path_buf();
        for part in relative.split('/') {
            path.push(part);
            ensure!(
                !std::fs::symlink_metadata(&path)
                    .with_context(|| format!("read local artifact {}", path.display()))?
                    .file_type()
                    .is_symlink(),
                "artifact symlinks are not accepted"
            );
        }
        ensure!(
            path.canonicalize()?.starts_with(root) && path.metadata()?.is_file(),
            "artifact outside workflow directory"
        );
        let (sha, size) =
            crate::artifacts::artifact::hash_file_with_progress(&path, |read, total| {
                progress(&path, read, total)
            })?;
        crate::artifacts::managed::validate(&sha, size)?;
        payloads.insert(sha.clone(), path);
        *value = json!({"sha256":sha,"size":size});
        return Ok(());
    }
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                local_artifacts(root, child, payloads, progress)?
            }
        }
        Value::Array(array) => {
            for child in array {
                local_artifacts(root, child, payloads, progress)?
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect(
    root: &Path,
    relative: &str,
    bundle: &mut BTreeMap<String, converyn_core::orchestration::inventory::PackageFile>,
    document: &mut Value,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    ensure!(
        converyn_core::orchestration::inventory::valid_package_path(relative),
        "invalid package path"
    );
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        ensure!(
            !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "package symlink rejected"
        );
    }
    if path.is_dir() {
        let mut entries = std::fs::read_dir(&path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            collect(
                root,
                &format!(
                    "{relative}/{}",
                    entry
                        .file_name()
                        .to_str()
                        .context("invalid package filename")?
                ),
                bundle,
                document,
            )?;
        }
    } else {
        ensure!(
            path.is_file() && bundle.len() < 4096,
            "invalid package entry or count"
        );
        let text = content(root, relative)?;
        let hash = hex::encode(Sha256::digest(text.as_bytes()));
        bundle.insert(
            relative.into(),
            converyn_core::orchestration::inventory::PackageFile {
                sha256: hash.clone(),
                executable: path.metadata()?.permissions().mode() & 0o111 != 0,
            },
        );
        if document.get("definitions").is_none() {
            document["definitions"] = json!({});
        }
        document["definitions"][hash] = json!(text);
    }
    Ok(())
}
