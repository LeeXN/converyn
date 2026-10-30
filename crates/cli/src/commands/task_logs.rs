//! Read actual stored bytes; receipts remain available as an explicitly labelled fallback.
use super::*;
use base64::Engine;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Write, os::unix::fs::OpenOptionsExt};

#[derive(clap::Args, Default)]
pub(crate) struct Options {
    /// Exact Work ID; mutually exclusive with --step and --node.
    #[arg(long, conflicts_with_all=["step","node"])]
    pub work_id: Option<String>,
    /// Select a named plan step, optionally restricted by --node.
    #[arg(long)]
    pub step: Option<String>,
    /// Select a node, optionally restricted by --step.
    #[arg(long)]
    pub node: Option<String>,
    /// Recent lines per Work and stream (default: 100).
    #[arg(long, value_parser=clap::value_parser!(u32).range(1..), conflicts_with="full")]
    pub tail: Option<u32>,
    /// Select output streams.
    #[arg(long, value_parser=["stdout","stderr","both"], default_value="both")]
    pub stream: String,
    /// Read all saved bytes, verifying the final manifest. Retention and truncation still apply.
    #[arg(long)]
    pub full: bool,
    /// Follow using acknowledged byte cursors; JSON is NDJSON. Ctrl+C stops viewing only.
    #[arg(long)]
    pub follow: bool,
    /// Export complete saved streams into separate Work/stream files; refuses overwrites.
    #[arg(long, requires = "full", conflicts_with = "follow")]
    pub output_dir: Option<PathBuf>,
}
#[derive(Default)]
struct Cursor {
    offset: u64,
    bytes: Vec<u8>,
    decoded: usize,
    done: bool,
    initialized: bool,
}
fn safe_text(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
fn tail(s: &str, n: usize) -> String {
    let lines = s.split_inclusive('\n').collect::<Vec<_>>();
    lines[lines.len().saturating_sub(n)..].concat()
}
fn decode(cursor: &mut Cursor, complete: bool) -> String {
    let pending = &cursor.bytes[cursor.decoded..];
    let count = if complete {
        pending.len()
    } else {
        match std::str::from_utf8(pending) {
            Ok(_) => pending.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => pending.len(),
        }
    };
    let text = String::from_utf8_lossy(&pending[..count]).into_owned();
    cursor.decoded += count;
    text
}
fn selected(run: &Value, options: &Options) -> Result<Vec<Value>> {
    let work = run["plan"]["work"]
        .as_array()
        .context("Run has no readable plan")?
        .iter()
        .filter(|w| {
            options.work_id.as_ref().is_none_or(|v| w["id"] == *v)
                && options.step.as_ref().is_none_or(|v| w["step_id"] == *v)
                && options.node.as_ref().is_none_or(|v| w["node_uid"] == *v)
        })
        .cloned()
        .collect::<Vec<_>>();
    ensure!(
        !work.is_empty(),
        "no Work matches the requested step/node/Work ID"
    );
    Ok(work)
}
async fn read(api: &converyn_runtime::nodes::agent::Client, path: &str) -> Result<Value> {
    converyn_runtime::interface::client::request(api, reqwest::Method::GET, path, None).await
}
fn emit(work: &Value, stream: &str, text: &str, v: &Value, options: &Options) -> Result<()> {
    if options.output_dir.is_some() {
        return Ok(());
    }
    if options.follow && output::is_json() {
        writeln!(
            std::io::stdout().lock(),
            "{}",
            json!({"work":work,"work_id":work["id"],"stream":stream,"text":text,"read":v})
        )?;
    } else if !output::is_json() {
        print!("{}", safe_text(text));
        std::io::stdout().flush()?;
    }
    Ok(())
}
pub(super) async fn run(source: ClientSource, run_id: String, options: Options) -> Result<()> {
    if options.follow {
        tokio::select! { r=read_logs(source, run_id, options)=>r, _=tokio::signal::ctrl_c()=>Ok(()) }
    } else {
        read_logs(source, run_id, options).await
    }
}
async fn read_logs(source: ClientSource, run_id: String, options: Options) -> Result<()> {
    ensure!(
        converyn_core::model::domain::valid_id(&run_id),
        "invalid Run id"
    );
    for id in [&options.work_id, &options.step, &options.node]
        .into_iter()
        .flatten()
    {
        ensure!(
            converyn_core::model::domain::valid_id(id),
            "invalid Work/step/node id"
        );
    }
    let api = source.load()?.client()?;
    let mut run = read(&api, &format!("/api/v1/runs/{run_id}")).await?;
    let mut work = selected(&run, &options)?;
    if options.work_id.is_none() && options.step.is_none() {
        work.retain(|w| {
            !w["outputs"]
                .as_object()
                .is_some_and(|o| o.values().any(|v| v["secret"] == true))
        });
    }
    ensure!(
        !work.is_empty(),
        "logs_suppressed: sensitive output producers do not publish stdout/stderr"
    );
    ensure!(
        !work.iter().any(|w| w["outputs"]
            .as_object()
            .is_some_and(|o| o.values().any(|v| v["secret"] == true))),
        "logs_suppressed: sensitive output producers do not publish stdout/stderr"
    );
    let streams: Vec<&str> = if options.stream == "stdout" {
        vec!["stdout"]
    } else if options.stream == "stderr" {
        vec!["stderr"]
    } else {
        vec!["stdout", "stderr"]
    };
    // Validate every export destination before fetching or writing any files.
    if let Some(dir) = &options.output_dir {
        std::fs::create_dir_all(dir)?;
        for w in &work {
            for stream in &streams {
                ensure!(
                    !dir.join(format!(
                        "{}.{stream}.log",
                        w["id"].as_str().context("invalid Work ID")?
                    ))
                    .exists(),
                    "export destination exists; refusing overwrite"
                );
            }
        }
    }
    let mut cursors = BTreeMap::<(String, String), Cursor>::new();
    let mut result = json!({});
    let mut exports = Vec::new();
    loop {
        let mut all_done = true;
        for w in &work {
            let id = w["id"].as_str().context("Work ID missing")?;
            let attempt = &run["attempts"][id];
            if result[id].is_null() {
                result[id] = json!({"work":w,"status":attempt["status"],"logs":attempt["receipt"]["logs"],"execution":attempt["receipt"]["execution"],"read":{}});
            }
            result[id]["status"] = attempt["status"].clone();
            for stream in &streams {
                let cursor = cursors.entry((id.into(), (*stream).into())).or_default();
                if cursor.done {
                    continue;
                }
                if attempt["receipt"].is_null()
                    && matches!(
                        attempt["status"].as_str(),
                        Some("pending" | "preparing" | "canceled")
                    )
                {
                    if attempt["status"] == "canceled" {
                        cursor.done = true;
                    } else {
                        all_done = false;
                    }
                    continue;
                }
                if w["operation"]["kind"] != "exec"
                    && attempt["receipt"].is_object()
                    && attempt["receipt"]["logs"].is_null()
                {
                    let package = attempt["receipt"]["native"]["package"]["diagnostic"]
                        .as_str()
                        .unwrap_or_default();
                    ensure!(
                        !options.full || package.is_empty(),
                        "full package logs are not stored; use the default view for its bounded receipt diagnostic"
                    );
                    let text = if *stream == "stderr" { package } else { "" };
                    let state = json!({"source":"native_receipt","condition":if package.is_empty(){"native_no_text"}else{"package_diagnostic"},"complete":true});
                    result[id]["read"][*stream] = json!({"text":text,"state":state});
                    if !output::is_json()
                        && (!text.is_empty() || options.work_id.is_some() || options.step.is_some())
                    {
                        println!(
                            "{} / {} [{stream}]: {}",
                            safe_text(w["step_id"].as_str().unwrap_or(id)),
                            safe_text(w["node_uid"].as_str().unwrap_or_default()),
                            if text.is_empty() {
                                "native operation has no textual log.".into()
                            } else {
                                safe_text(text)
                            }
                        );
                    }
                    if options.follow && output::is_json() {
                        emit(w, stream, text, &state, &options)?;
                    }
                    cursor.done = true;
                    continue;
                }
                let mut state = json!({"source":"stored_chunks","condition":"upload_pending"});
                let mut failure = None;
                let start = cursor.offset;
                // Bounded by the existing 8 MiB/stream retention limit.
                for _ in 0..130 {
                    match read(
                        &api,
                        &format!(
                            "/api/v1/runs/{run_id}/logs/{id}/{stream}?offset={}",
                            cursor.offset
                        ),
                    )
                    .await
                    {
                        Err(e) => {
                            if e.downcast_ref::<converyn_runtime::nodes::agent::AuthenticationRejected>().is_some(){return Err(e);}
                            failure = Some(e.to_string());
                            break;
                        }
                        Ok(v) => {
                            state = v;
                            if state["expired"] == true || state["condition"] == "upload_incomplete"
                            {
                                failure = Some(
                                    if state["expired"] == true {
                                        "saved log expired"
                                    } else {
                                        "log upload has missing ranges"
                                    }
                                    .into(),
                                );
                                break;
                            }
                            if options.full && state["verified"] != true {
                                failure = Some(
                                    "final log manifest is not available; upload pending".into(),
                                );
                                break;
                            }
                            let bytes = base64::engine::general_purpose::STANDARD
                                .decode(state["data"].as_str().unwrap_or_default())?;
                            if bytes.is_empty()
                                && state["complete"] != true
                                && cursor.offset == 0
                                && attempt["receipt"]["logs"][*stream]["tail"].is_string()
                            {
                                failure = Some(
                                    "saved bytes are not yet available; upload pending or missing"
                                        .into(),
                                );
                                break;
                            }
                            let next = state["next_offset"]
                                .as_u64()
                                .context("missing log cursor")?;
                            ensure!(
                                state["offset"].as_u64() == Some(cursor.offset)
                                    && next == cursor.offset + bytes.len() as u64
                                    && next <= converyn_core::model::logs::LOG_LIMIT,
                                "invalid log byte cursor"
                            );
                            cursor.bytes.extend_from_slice(&bytes);
                            cursor.offset = next;
                            cursor.done = state["complete"] == true;
                            if cursor.done {
                                let expected = state["sha256"]
                                    .as_str()
                                    .context("completed log has no digest")?;
                                ensure!(
                                    hex::encode(Sha256::digest(&cursor.bytes)) == expected,
                                    "complete log checksum mismatch for {id}/{stream}"
                                );
                            }
                            if bytes.is_empty() || cursor.done {
                                break;
                            }
                        }
                    }
                }
                if let Some(reason) = failure {
                    if options.full {
                        anyhow::bail!(
                            "{id}/{stream}: {reason}; full reads cannot use receipt tails"
                        );
                    }
                    state = json!({"source":"receipt_tail","condition":"storage_unavailable_or_upload_missing","reason":reason});
                    if cursor.offset == 0 {
                        let fallback = attempt["receipt"]["logs"][*stream]["tail"].as_str();
                        let text = fallback.unwrap_or(if w["operation"]["kind"] == "exec" {
                            "No output available; awaiting upload.\n"
                        } else {
                            "Native operation has no textual log.\n"
                        });
                        if !cursor.initialized {
                            if !output::is_json() {
                                println!(
                                    "\n{} / {} / {} [{stream}] — receipt tail: {}",
                                    safe_text(w["step_id"].as_str().unwrap_or(id)),
                                    safe_text(w["node_uid"].as_str().unwrap_or_default()),
                                    attempt["status"],
                                    safe_text(state["reason"].as_str().unwrap_or_default())
                                );
                            }
                            emit(
                                w,
                                stream,
                                &tail(text, options.tail.unwrap_or(100) as usize),
                                &state,
                                &options,
                            )?;
                        }
                        result[id]["read"][*stream] = json!({"text":tail(text,options.tail.unwrap_or(100) as usize),"state":state});
                        cursor.initialized = true;
                        cursor.done = !options.follow
                            || (w["operation"]["kind"] != "exec"
                                && attempt["receipt"]["logs"].is_null()
                                && !attempt["receipt"].is_null());
                    }
                } else {
                    let mut text = decode(cursor, cursor.done);
                    if !cursor.initialized && !options.full {
                        text = tail(&text, options.tail.unwrap_or(100) as usize);
                    }
                    if !output::is_json()
                        && (!cursor.initialized || (options.follow && cursor.offset > start))
                    {
                        println!(
                            "\n{} / {} / {} [{stream}] — {}{}",
                            safe_text(w["step_id"].as_str().unwrap_or(id)),
                            safe_text(w["node_uid"].as_str().unwrap_or_default()),
                            attempt["status"],
                            state["source"].as_str().unwrap_or("stored"),
                            if state["truncated"] == true {
                                " (truncated)"
                            } else {
                                ""
                            }
                        );
                    }
                    if !cursor.initialized || cursor.offset > start {
                        emit(w, stream, &text, &state, &options)?;
                    }
                    result[id]["read"][*stream] = json!({"text":if options.full{String::from_utf8_lossy(&cursor.bytes).into_owned()}else{tail(&String::from_utf8_lossy(&cursor.bytes),options.tail.unwrap_or(100) as usize)},"state":state});
                    if cursor.done && cursor.bytes.is_empty() && !output::is_json() {
                        println!("No output was produced.");
                    }
                    cursor.initialized = true;
                    if cursor.done
                        && let Some(dir) = &options.output_dir
                    {
                        exports
                            .push((dir.join(format!("{id}.{stream}.log")), cursor.bytes.clone()));
                    }
                }
                all_done &= cursor.done;
            }
        }
        if !options.follow || all_done {
            break;
        }
        tokio::select! { _=tokio::signal::ctrl_c()=>return Ok(()), _=tokio::time::sleep(std::time::Duration::from_secs(2))=>{} }
        run = read(&api, &format!("/api/v1/runs/{run_id}")).await?;
    }
    // create_new is still required after preflight, including races with other exporters.
    let mut created = Vec::new();
    for (path, data) in exports {
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&data)?;
                file.sync_all()?;
                created.push(path);
            }
            Err(e) => {
                for p in created {
                    let _ = std::fs::remove_file(p);
                }
                return Err(e).context("export refused; destination exists or cannot be created");
            }
        }
    }
    if output::is_json() && !options.follow {
        output::print(&result)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_and_unicode_cursor() {
        let run = json!({"plan":{"work":[{"id":"a","step_id":"verify","node_uid":"n1"},{"id":"b","step_id":"verify","node_uid":"n2"},{"id":"c","step_id":"apply","node_uid":"n1"}]}});
        let o = Options {
            step: Some("verify".into()),
            ..Default::default()
        };
        assert_eq!(selected(&run, &o).unwrap().len(), 2);
        assert!(
            selected(
                &run,
                &Options {
                    node: Some("missing".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        let mut c = Cursor::default();
        c.bytes.extend_from_slice(&[0xe4, 0xb8]);
        assert_eq!(decode(&mut c, false), "");
        c.bytes.extend_from_slice(&[0xad, b'\n']);
        assert_eq!(decode(&mut c, false), "中\n");
        assert_eq!(tail("one\ntwo\nthree\n", 2), "two\nthree\n");
    }
}
