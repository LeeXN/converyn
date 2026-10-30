//! Result-file ownership, bounded schema validation and exactly-once sealing.
use crate::executors::runner::{Outcome, Receipt};
use anyhow::{Context as _, Result, ensure};
use converyn_core::model::outputs::{self as model, PublicKey, Specifications, StoredValue};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub run_id: String,
    pub work_id: String,
    pub specifications: Specifications,
    pub key: Option<PublicKey>,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    context: Context,
    file: PathBuf,
    uid: u32,
}
impl Context {
    pub fn no_log(&self) -> bool {
        self.specifications.values().any(|s| s.secret)
    }
    fn encode(&self, values: BTreeMap<String, Value>) -> Result<model::Values> {
        model::validate_plain(&self.specifications, &values)?;
        values
            .into_iter()
            .map(|(name, value)| {
                let spec = &self.specifications[&name];
                let stored = if spec.secret {
                    StoredValue::Secret {
                        sealed: crate::credentials::outputs::seal(
                            self.key.as_ref().context("output encryption key missing")?,
                            &value,
                            &crate::credentials::outputs::binding(
                                &self.run_id,
                                &self.work_id,
                                &name,
                            ),
                        )?,
                    }
                } else {
                    StoredValue::Public { value }
                };
                Ok((name, stored))
            })
            .collect()
    }
}
pub fn prepare(dir: &Path, work: &Path, uid: u32, gid: u32, context: &Context) -> Result<PathBuf> {
    model::validate_specs(&context.specifications)?;
    let output = work.join(".converyn-outputs.json");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&output)?;
    file.write_all(b"{}")?;
    file.sync_all()?;
    let path = std::ffi::CString::new(output.as_os_str().as_encoded_bytes())?;
    // SAFETY: the newly created regular file is under this protected Work and uid is local policy.
    ensure!(
        unsafe { libc::chown(path.as_ptr(), uid, gid) } == 0,
        "cannot assign result file"
    );
    crate::nodes::agent::durable(
        &dir.join("output-context.json"),
        &serde_json::to_vec(&Journal {
            context: context.clone(),
            file: output.clone(),
            uid,
        })?,
    )?;
    Ok(output)
}
fn read(journal: &Journal) -> Result<BTreeMap<String, Value>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&journal.file)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file()
            && meta.uid() == journal.uid
            && meta.nlink() == 1
            && meta.mode() & 0o077 == 0
            && meta.len() <= model::MAX_DOCUMENT_BYTES as u64,
        "unsafe result file"
    );
    serde_json::from_reader(file.take((model::MAX_DOCUMENT_BYTES + 1) as u64))
        .map_err(|_| anyhow::anyhow!("invalid result JSON"))
}
fn load(dir: &Path) -> Result<Option<Journal>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("output-context.json"))
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(Some(serde_json::from_reader(file.take(256 * 1024))?))
}
pub fn no_log(dir: &Path) -> bool {
    load(dir).ok().flatten().is_some_and(|j| j.context.no_log())
}
pub fn finish(dir: &Path, receipt: &mut Receipt) -> Result<()> {
    let Some(journal) = load(dir)? else {
        return Ok(());
    };
    if receipt.outcome == Outcome::Succeeded {
        let result = if journal
            .file
            .parent()
            .unwrap()
            .join(".converyn-output-error")
            .exists()
        {
            Err(anyhow::anyhow!("output writer rejected input"))
        } else {
            read(&journal).and_then(|v| journal.context.encode(v))
        };
        match result {
            Ok(values) => receipt.outputs = values,
            Err(_) => {
                receipt.outcome = Outcome::Failed;
                receipt.diagnostic = Some("declared_output_missing_invalid_or_unsafe".into());
                receipt.outputs.clear();
            }
        }
    } else {
        receipt.outputs.clear();
    }
    if let Ok(files) = fs::read_dir(journal.file.parent().unwrap()) {
        for file in files.flatten() {
            if file
                .file_name()
                .to_string_lossy()
                .starts_with(".converyn-context-")
            {
                let _ = fs::remove_file(file.path());
            }
        }
    }
    if journal.context.no_log() {
        receipt.logs = None;
        for name in [
            "stdout.log",
            "stderr.log",
            "supervisor.stdout.log",
            "supervisor.stderr.log",
            "task-logs.json",
        ] {
            if let Ok(file) = OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(dir.join(name))
            {
                file.set_len(0)?;
            }
        }
    }
    // Commit the exact sealed receipt before removing the only producer document.
    crate::nodes::agent::durable(&dir.join("completion.json"), &serde_json::to_vec(receipt)?)?;
    if receipt.outcome == Outcome::NeedsAttention {
        quarantine(dir)?;
    } else if journal.file.exists() {
        fs::remove_file(&journal.file)?;
        File::open(journal.file.parent().unwrap())?.sync_all()?;
    }
    Ok(())
}
/// Cleanup is safe to repeat after recovering the committed completion.
pub fn cleanup(dir: &Path) -> Result<()> {
    let Some(journal) = load(dir)? else {
        return Ok(());
    };
    if let Ok(files) = fs::read_dir(journal.file.parent().context("output parent missing")?) {
        for file in files.flatten() {
            if file
                .file_name()
                .to_string_lossy()
                .starts_with(".converyn-context-")
            {
                let _ = fs::remove_file(file.path());
            }
        }
    }
    if journal.context.no_log() {
        for name in [
            "stdout.log",
            "stderr.log",
            "supervisor.stdout.log",
            "supervisor.stderr.log",
            "task-logs.json",
        ] {
            if let Ok(file) = OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(dir.join(name))
            {
                file.set_len(0)?;
            }
        }
    }
    if journal.file.exists() {
        fs::remove_file(&journal.file)?;
    }
    Ok(())
}
/// Unknown output is evidence, not an input for later work. Remove its task-UID
/// copy and retain at most one root-private document for explicit investigation.
pub fn quarantine(dir: &Path) -> Result<()> {
    let Some(journal) = load(dir)? else {
        return Ok(());
    };
    if journal.file.exists() {
        if let Ok(values) = read(&journal) {
            crate::nodes::agent::durable(
                &dir.join("private-output-evidence.json"),
                &serde_json::to_vec(&values)?,
            )?;
        }
        fs::remove_file(&journal.file)?;
        File::open(journal.file.parent().context("output parent missing")?)?.sync_all()?;
    }
    cleanup(dir)?;
    Ok(())
}
pub fn native_receipt(
    directory: &Path,
    context: &Context,
    mut receipt: Receipt,
) -> Result<Receipt> {
    if context.specifications.is_empty() {
        return Ok(receipt);
    }
    fs::create_dir_all(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let name = converyn_core::model::domain::digest(&receipt.operation_id);
    let path = directory.join(format!("{name}.json"));
    if path.exists() {
        let saved: Receipt = serde_json::from_reader(
            File::open(&path)?.take((model::MAX_RECEIPT_BYTES + 1) as u64),
        )?;
        ensure!(
            saved.operation_id == receipt.operation_id && saved.native == receipt.native,
            "output receipt differs from original evidence"
        );
        return Ok(saved);
    }
    if receipt.outcome == Outcome::Succeeded {
        let evidence = serde_json::to_value(&receipt.native)?;
        let values = context
            .specifications
            .iter()
            .map(|(name, spec)| {
                let value = spec
                    .source
                    .as_ref()
                    .map_or_else(|| evidence.get(name), |p| evidence.pointer(p))
                    .context("native result selector missing")?;
                Ok((name.clone(), value.clone()))
            })
            .collect::<Result<BTreeMap<_, _>>>();
        match values.and_then(|v| context.encode(v)) {
            Ok(values) => receipt.outputs = values,
            Err(_) => {
                receipt.outcome = Outcome::Failed;
                receipt.diagnostic = Some("native_output_contract_invalid".into());
            }
        }
    }
    crate::nodes::agent::durable(&path, &serde_json::to_vec(&receipt)?)?;
    Ok(receipt)
}
/// Local-only writer used inside a managed task. It never loads a personal
/// context, accepts legacy APIs or contacts a controller.
pub fn emit(input: impl Read) -> Result<()> {
    let path = PathBuf::from(
        std::env::var("CONVERYN_OUTPUT_FILE").context("task output file unavailable")?,
    );
    let mut document: BTreeMap<String, Value> =
        serde_json::from_reader(input.take((model::MAX_DOCUMENT_BYTES + 1) as u64))
            .map_err(|_| anyhow::anyhow!("output input must be a JSON object"))?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)?;
    file.lock_exclusive()?;
    let meta = file.metadata()?;
    // SAFETY: geteuid only reads the current process identity.
    ensure!(
        meta.is_file()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.nlink() == 1
            && meta.mode() & 0o077 == 0,
        "unsafe task output file"
    );
    let mut original: BTreeMap<String, Value> =
        serde_json::from_reader((&mut file).take((model::MAX_DOCUMENT_BYTES + 1) as u64))?;
    original.append(&mut document);
    let data = serde_json::to_vec(&original)?;
    ensure!(
        data.len() <= model::MAX_DOCUMENT_BYTES,
        "task output document too large"
    );
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(&data)?;
    file.sync_all()?;
    Ok(())
}

pub fn mark_invalid() {
    if let Ok(path) = std::env::var("CONVERYN_OUTPUT_FILE")
        && let Some(parent) = Path::new(&path).parent()
        && let Ok(mut file) = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(parent.join(".converyn-output-error"))
    {
        let _ = file.write_all(b"output writer rejected input");
        let _ = file.sync_all();
    }
}
fn object(value: Value) -> Result<BTreeMap<String, Value>> {
    let object = value
        .as_object()
        .context("task context must be a JSON object")?;
    let mut result = BTreeMap::new();
    for (key, value) in object {
        ensure!(
            (converyn_core::model::variables::name_valid(key)
                && converyn_core::orchestration::workflow::safe_environment(key)),
            "context contains a protected environment name"
        );
        result.insert(key.clone(), value.clone());
    }
    ensure!(result.len() <= 512, "context has too many fields");
    Ok(result)
}
fn read_context(path: &Path) -> Result<BTreeMap<String, Value>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    // SAFETY: geteuid only reads current identity; no privilege change occurs.
    ensure!(
        meta.is_file()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.nlink() == 1
            && meta.mode() & 0o077 == 0,
        "unsafe task context"
    );
    let value = serde_json::from_reader(file.take(128 * 1024 + 1))
        .map_err(|_| anyhow::anyhow!("invalid task context JSON"))?;
    object(value)
}
pub fn emit_context(input: impl Read) -> Result<()> {
    let patch: Value = serde_json::from_reader(input.take((model::MAX_DOCUMENT_BYTES + 1) as u64))
        .map_err(|_| anyhow::anyhow!("context update must be JSON"))?;
    let patch = object(patch)?;
    let path = PathBuf::from(
        std::env::var("CONVERYN_CONTEXT_FILE").context("no managed invocation context")?,
    );
    let mut value = read_context(&path)?;
    let output = PathBuf::from(std::env::var("CONVERYN_OUTPUT_FILE")?);
    let existing: Value =
        serde_json::from_reader(File::open(&output)?.take((model::MAX_DOCUMENT_BYTES + 1) as u64))?;
    if let Some(context) = existing.get("context") {
        value.extend(object(context.clone())?);
    }
    value.extend(patch);
    emit(serde_json::to_vec(&serde_json::json!({"context":value}))?.as_slice())
}
pub fn context_value(name: &str) -> Result<Option<Value>> {
    ensure!(
        converyn_core::model::variables::name_valid(name),
        "invalid context field"
    );
    let path = PathBuf::from(
        std::env::var("CONVERYN_CONTEXT_FILE").context("no managed invocation context")?,
    );
    let mut context = read_context(&path)?;
    if let Ok(path) = std::env::var("CONVERYN_OUTPUT_FILE") {
        let value: Value = serde_json::from_reader(
            File::open(path)?.take((model::MAX_DOCUMENT_BYTES + 1) as u64),
        )?;
        if let Some(value) = value.get("context") {
            context.extend(object(value.clone())?);
        }
    }
    Ok(context.remove(name))
}
/// A declared object input can drive ordinary application variables. Invocation
/// never changes UID, cgroup, filesystem grants, interpreter or the registered script.
pub fn invoke(
    environments: &[String],
    fixed: &[String],
    script: &Path,
    publish: bool,
) -> Result<i32> {
    let mut merged = BTreeMap::new();
    for name in environments {
        let text = std::env::var(name)
            .with_context(|| format!("missing declared context variable {name}"))?;
        ensure!(text.len() <= 128 * 1024, "context input too large");
        let value: Value = serde_json::from_str(&text)
            .map_err(|_| anyhow::anyhow!("context input is not JSON"))?;
        if let Value::Array(entries) = value {
            let mut simultaneous = BTreeMap::new();
            for entry in entries {
                for (key, value) in object(
                    entry
                        .get("value")
                        .context("invalid collected context")?
                        .clone(),
                )? {
                    ensure!(
                        simultaneous.get(&key).is_none_or(|old| old == &value),
                        "parallel context fields conflict: {key}"
                    );
                    simultaneous.insert(key, value);
                }
            }
            merged.extend(simultaneous);
        } else {
            merged.extend(object(value)?);
        }
    }
    let output = PathBuf::from(
        std::env::var("CONVERYN_OUTPUT_FILE").context("managed result file is unavailable")?,
    );
    let path = output
        .parent()
        .context("output parent missing")?
        .join(format!(
            ".converyn-context-{}.json",
            crate::credentials::pki::random_token()?
        ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    file.write_all(&serde_json::to_vec(&merged)?)?;
    file.sync_all()?;
    if publish {
        emit(serde_json::to_vec(&serde_json::json!({"context":merged}))?.as_slice())?;
    }
    for name in fixed {
        let text = std::env::var(name).context("fixed task metadata unavailable")?;
        merged.extend(object(
            serde_json::from_str(&text)
                .map_err(|_| anyhow::anyhow!("fixed metadata is not JSON"))?,
        )?);
    }
    let values: BTreeMap<String, String> = merged
        .into_iter()
        .map(|(key, value)| {
            (
                key,
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            )
        })
        .collect();
    ensure!(
        values.len() <= 512
            && values.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() <= 128 * 1024,
        "invocation environment exceeds limit"
    );
    let status = std::process::Command::new("/bin/bash")
        .arg(script)
        .envs(values)
        .env("CONVERYN_CONTEXT_FILE", &path)
        .status();
    let _ = fs::remove_file(&path);
    Ok(status?.code().unwrap_or(1))
}
