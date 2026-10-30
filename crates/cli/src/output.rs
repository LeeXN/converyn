//! One stdout formatter for every command. JSON preserves the original response shape.
use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fmt::Write as _,
    io::{IsTerminal, Write},
    sync::OnceLock,
};

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub(crate) enum Format {
    #[default]
    Table,
    Json,
    Wide,
}
static FORMAT: OnceLock<Format> = OnceLock::new();
pub(crate) fn set_format(format: Format) {
    let _ = FORMAT.set(format);
}
pub(crate) fn is_json() -> bool {
    matches!(FORMAT.get(), Some(Format::Json))
}
pub(crate) fn print(value: &impl Serialize) -> Result<()> {
    let value = serde_json::to_value(value)?;
    let text = render(&value, *FORMAT.get().unwrap_or(&Format::Table), now_ms())?;
    writeln!(std::io::stdout().lock(), "{text}")?;
    if !matches!(FORMAT.get().unwrap_or(&Format::Table), Format::Json)
        && let Some(cursor) = value["next_cursor"].as_str()
    {
        eprintln!("More results: repeat with --limit and --cursor {cursor} (preserve --status).");
    }
    Ok(())
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub(crate) fn clean(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
fn cell(value: &Value) -> String {
    match value {
        Value::Null => "-".into(),
        Value::String(s) => clean(s),
        Value::Array(values) if values.is_empty() => "-".into(),
        Value::Object(fields) if fields.is_empty() => "-".into(),
        Value::Array(values) => values.iter().map(cell).collect::<Vec<_>>().join(","),
        Value::Object(fields) => fields
            .keys()
            .map(|s| clean(s))
            .collect::<Vec<_>>()
            .join(","),
        _ => value.to_string(),
    }
}
fn field(value: &Value, path: &str) -> String {
    let mut v = value;
    for key in path.split('.') {
        v = &v[key];
    }
    cell(v)
}
fn width(text: &str) -> usize {
    text.chars().map(|c| if matches!(c as u32, 0x1100..=0x115f | 0x2329..=0x232a | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xfe10..=0xfe6f | 0xff01..=0xff60 | 0xffe0..=0xffe6 | 0x1f300..=0x1faff | 0x20000..=0x3fffd) {2} else {1}).sum()
}
fn terminal_width() -> usize {
    #[cfg(unix)]
    {
        let mut size = std::mem::MaybeUninit::<libc::winsize>::zeroed();
        // SAFETY: ioctl writes a winsize into a correctly sized, initialized buffer.
        if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, size.as_mut_ptr()) } == 0 {
            let size = unsafe { size.assume_init() };
            if size.ws_col > 0 {
                return usize::from(size.ws_col);
            }
        }
    }
    100
}
pub(crate) fn truncate(s: &str, cap: usize) -> String {
    if width(s) <= cap {
        return s.into();
    }
    let mut result = String::new();
    for c in s.chars() {
        if width(&result) + width(&c.to_string()) + 1 > cap {
            break;
        }
        result.push(c);
    }
    result.push('…');
    result
}
fn color_cell(s: &str) -> String {
    if !std::io::stdout().is_terminal() || std::env::var_os("NO_COLOR").is_some() {
        return s.into();
    }
    let color = match s {
        "succeeded" | "online" => "32",
        "failed" | "offline" => "31",
        "needs_attention" | "paused" => "33",
        "running" | "authorized" => "36",
        _ => return s.into(),
    };
    format!("\x1b[{color}m{s}\x1b[0m")
}
fn table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    if rows.is_empty() {
        return "No resources found.".into();
    }
    let human_terminal = std::io::stdout().is_terminal();
    let budget = if human_terminal {
        terminal_width()
    } else {
        usize::MAX
    };
    let mut rows = rows;
    if human_terminal && !matches!(FORMAT.get(), Some(Format::Wide)) {
        let cap = budget.saturating_sub(headers.len().saturating_sub(1) * 2) / headers.len().max(1);
        let cap = cap.max(6);
        for row in &mut rows {
            for cell in row {
                *cell = truncate(cell, cap);
            }
        }
    }
    let mut widths = headers.iter().map(|h| width(h)).collect::<Vec<_>>();
    for row in &rows {
        for (i, text) in row.iter().enumerate() {
            widths[i] = widths[i].max(width(text));
        }
    }
    std::iter::once(headers.iter().map(|h| h.to_string()).collect::<Vec<_>>())
        .chain(rows)
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(i, text)| {
                    if i + 1 == row.len() {
                        color_cell(text)
                    } else {
                        format!(
                            "{}{}",
                            color_cell(text),
                            " ".repeat(widths[i] - width(text) + 2)
                        )
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn items(value: &Value) -> Vec<(String, &Value)> {
    match value {
        Value::Object(map) => map.iter().map(|(id, v)| (clean(id), v)).collect(),
        Value::Array(array) => array.iter().map(|v| (cell(&v["uid"]), v)).collect(),
        _ => Vec::new(),
    }
}
fn age(observed: &Value, now: u64) -> String {
    let Some(ms) = observed.as_u64() else {
        return "-".into();
    };
    let seconds = now.saturating_sub(ms) / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}
fn online(node: &Value, now: u64) -> bool {
    node["observed_ms"]
        .as_u64()
        .is_some_and(|ms| now.saturating_sub(ms) < 60_000)
}
fn timestamp(value: &Value) -> String {
    value
        .as_i64()
        .and_then(|ms| time::OffsetDateTime::from_unix_timestamp(ms / 1000).ok())
        .and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| "-".into())
}
fn workflow_table(value: &Value) -> String {
    table(
        &["ID", "NAME", "ENABLED", "TARGETS", "PARAMETERS", "REVISION"],
        items(value)
            .into_iter()
            .map(|(_, v)| {
                let mut variables = v["definition"]["values"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                variables.extend(
                    v["definition"]["parameters"]
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                );
                vec![
                    field(v, "workflow.uid"),
                    field(v, "definition.name"),
                    field(v, "workflow.enabled"),
                    field(v, "definition.targets"),
                    cell(&Value::Object(variables)),
                    field(v, "workflow.revision"),
                ]
            })
            .collect(),
    )
}
fn run_table(value: &Value) -> String {
    let mut rows = items(value);
    rows.sort_by(|(_, a), (_, b)| {
        b["execution"]["submitted_ms"]
            .as_u64()
            .cmp(&a["execution"]["submitted_ms"].as_u64())
            .then_with(|| cell(&a["id"]).cmp(&cell(&b["id"])))
    });
    table(
        &[
            "ID",
            "WORKFLOW",
            "TARGET",
            "STATUS",
            "PROGRESS",
            "ACTOR",
            "SUBMITTED",
            "ARCHIVED",
        ],
        rows.into_iter()
            .map(|(_, v)| {
                vec![
                    field(v, "id"),
                    field(v, "execution.request.workflow_uid"),
                    field(v, "execution.request.target"),
                    field(v, "status"),
                    format!("{}/{}", cell(&v["completed"]), cell(&v["total"])),
                    field(v, "execution.actor.name"),
                    timestamp(&v["execution"]["submitted_ms"]),
                    v["archived"].as_bool().unwrap_or(false).to_string(),
                ]
            })
            .collect(),
    )
}
fn node_table(value: &Value, now: u64, wide: bool) -> String {
    let mut headers = vec![
        "NAME",
        "HEARTBEAT",
        "LAST SEEN",
        "IP",
        "CPU",
        "MEMORY",
        "GPU",
        "OS",
        "ASSETS",
    ];
    if wide {
        headers.extend(["ARCH", "MODEL", "SERIAL", "BOOT ID"]);
    }
    table(
        &headers,
        items(value)
            .into_iter()
            .map(|(id, v)| {
                let i = &v["inventory"]["data"];
                let mut row = vec![
                    id,
                    if v["observed_ms"].as_u64().is_none() {
                        "Unknown"
                    } else if online(v, now) {
                        "Online"
                    } else {
                        "Offline"
                    }
                    .into(),
                    age(&v["observed_ms"], now),
                    i["addresses"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|v| field(v, "address"))
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_else(|| "-".into()),
                    format!("{} / {}", field(i, "cpu_model"), field(i, "cpu_logical")),
                    bytes(&i["memory_total_bytes"]),
                    i["gpus"]
                        .as_array()
                        .map(|a| {
                            if a.is_empty() {
                                "None".into()
                            } else {
                                a.iter()
                                    .map(|v| field(v, "model"))
                                    .collect::<Vec<_>>()
                                    .join(",")
                            }
                        })
                        .unwrap_or_else(|| "Unknown".into()),
                    field(i, "os"),
                    freshness(v, "inventory", now),
                ];
                if wide {
                    row.extend([
                        field(i, "architecture"),
                        field(i, "model"),
                        field(i, "serial"),
                        field(v, "boot_id"),
                    ]);
                }
                row
            })
            .collect(),
    )
}
fn bytes(v: &Value) -> String {
    v.as_u64()
        .map(|n| format!("{:.1} GiB", n as f64 / (1024. * 1024. * 1024.)))
        .unwrap_or_else(|| "-".into())
}
fn freshness(v: &Value, kind: &str, now: u64) -> String {
    if v[kind].is_null() {
        return "Missing".into();
    }
    let time = if kind == "inventory" {
        v["inventory_checked_ms"]
            .as_u64()
            .or(v[kind]["collected_ms"].as_u64())
    } else {
        v[kind]["collected_ms"].as_u64()
    };
    if time.is_none_or(|t| now.saturating_sub(t) > if kind == "inventory" { 600000 } else { 60000 })
    {
        return "Stale".into();
    }
    if v[kind]["truncated"] == true {
        "Truncated"
    } else if v[kind]["errors"].as_object().is_some_and(|m| !m.is_empty()) {
        "Partial / errors"
    } else {
        "Fresh"
    }
    .into()
}

fn detail(value: &Value, indent: usize) -> String {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                let k = clean(k);
                if v.is_object()
                    || v.is_array()
                        && v.as_array()
                            .unwrap()
                            .iter()
                            .any(|v| v.is_object() || v.is_array())
                {
                    format!("{pad}{k}:\n{}", detail(v, indent + 2))
                } else {
                    format!("{pad}{k}: {}", cell(v))
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Array(values) => values
            .iter()
            .map(|v| format!("{pad}- {}", detail(v, indent + 2).trim_start()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => format!("{pad}{}", cell(value)),
    }
}
fn credential_table(value: &Value) -> String {
    table(
        &[
            "ID",
            "IDENTITY",
            "NAME",
            "ACTIVE",
            "REVOKED",
            "EXPIRES",
            "FINGERPRINT",
        ],
        items(value)
            .into_iter()
            .map(|(_, v)| {
                vec![
                    field(v, "uid"),
                    field(v, "identity_uid"),
                    field(v, "identity_name"),
                    field(v, "active"),
                    (!v["revoked_ms"].is_null()).to_string(),
                    timestamp(&v["expires_ms"]),
                    field(v, "fingerprint"),
                ]
            })
            .collect(),
    )
}
fn resource_detail(value: &Value, now: u64) -> String {
    let meta = &value["detail_resource"];
    let id = field(meta, "id");
    let kind = meta["kind"].as_str().unwrap_or_default();
    match kind {
        "nodes" => items(&value["nodes"])
            .into_iter()
            .map(|(id, n)| {
                format!(
                    "Node: {id}\nInventory: {} · Resources: {}\n{}",
                    freshness(n, "inventory", now),
                    freshness(n, "resources", now),
                    detail(n, 0)
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        "identities" => {
            let identity = &value["identities"][meta["id"].as_str().unwrap_or_default()];
            let credential = value["credentials"]
                .as_array()
                .and_then(|v| v.iter().find(|v| v["active"] == true).or(v.first()));
            let next = credential
                .map(|c| {
                    format!(
                        "\n\nCertificate details:\n  converyn get credentials {}",
                        field(c, "uid")
                    )
                })
                .unwrap_or_default();
            format!(
                "Identity: {id}\n{}\n\nRegistered certificates:\n{}\n\nLocal credential location:\n{}\n\nPublic certificates are stored in the replicated controller registry. Private keys and original client directories are client-held.{}",
                detail(identity, 0),
                credential_table(&value["credentials"]),
                detail(&value["local_credential"], 0),
                next
            )
        }
        "credentials" => {
            let mut c = value["credential"].clone();
            if let Some(v) = c.as_object_mut() {
                v.remove("certificate_pem");
            }
            for key in [
                "not_before_ms",
                "expires_ms",
                "revoked_ms",
                "overlap_until_ms",
            ] {
                if !c[key].is_null() {
                    c[key] = json!(timestamp(&c[key]));
                }
            }
            let local = if value["local_credential"]["matches_selected_config"] == true {
                "\n  converyn export credentials --current --output ./browser.p12\n  converyn config export --output ./personal-context.json"
            } else {
                "\nPKCS#12 requires the matching local --context or original --directory; it cannot be recovered from the controller."
            };
            format!(
                "Credential: {id}\n{}\n\nController storage:\n{}\n\nLocal credential location:\n{}\n\nExport public certificate (PEM; no private key):\n  converyn export credentials --id {id} --output ./{id}.crt\n{local}",
                detail(&c, 0),
                detail(&value["storage"], 0),
                detail(&value["local_credential"], 0)
            )
        }
        "node_groups" | "targets" => format!(
            "{kind}: {id}\n{}",
            detail(&value[kind][meta["id"].as_str().unwrap_or_default()], 0)
        ),
        "plugins" => format!("Plugin: {id}\n{}", detail(&value["plugins"][0], 0)),
        "members" => {
            format!("Member: {id}\n{}", detail(&value["member"], 0))
        }
        _ => detail(value, 0),
    }
}
fn work_log_commands(value: &Value) -> String {
    let run = field(value, "id");
    let mut lines = Vec::new();
    if let Some(work) = value["plan"]["work"].as_array() {
        for w in work {
            let id = w["id"].as_str().unwrap_or_default();
            if w["outputs"]
                .as_object()
                .is_some_and(|o| o.values().any(|v| v["secret"] == true))
            {
                continue;
            }
            if w["operation"]["kind"] == "exec"
                || value["attempts"][id]["receipt"]["logs"].is_object()
            {
                lines.push(format!(
                    "  converyn logs runs {run} --step {} --node {}",
                    field(w, "step_id"),
                    field(w, "node_uid")
                ));
            }
        }
    }
    if lines.is_empty() {
        return "No text logs for these native operations; inspect original receipts with -o json."
            .into();
    }
    format!(
        "Logs by Work (last 100 lines per stream):\n{}\nAppend --follow for live output; --full for all saved bytes; --stream stderr to select a stream.\nFull Work IDs: converyn get runs {run} -o wide\nReceipts: converyn get runs {run} -o json\nUse --work-id WORK_ID for an exact Work. Native operations without text logs retain their execution evidence in receipts.",
        lines.join("\n")
    )
}
fn render(value: &Value, format: Format, now: u64) -> Result<String> {
    if matches!(format, Format::Json) {
        return Ok(serde_json::to_string_pretty(value)?);
    }
    if value["detail_resource"].is_object() {
        return Ok(resource_detail(value, now));
    }
    if value.as_object().is_some_and(|v| v.is_empty())
        || value.as_array().is_some_and(|v| v.is_empty())
    {
        return Ok("No resources found.".into());
    }
    if value["overview"] == true {
        let nodes = items(&value["nodes"]);
        let workflows = items(&value["workflows"]);
        let mut states = std::collections::BTreeMap::<String, usize>::new();
        for (_, run) in items(&value["runs"]) {
            *states.entry(cell(&run["status"])).or_default() += 1;
        }
        let summary = json!({"Cluster":value["cluster"],"API reads":value["business_reads"],"Servers":value["servers"],
            "Leader (local observation)":value["controller_observation"]["metrics"]["current_leader"],
            "Controller voters":value["members"]["voters"],
            "Node heartbeats":format!("{} online / {} observed",nodes.iter().filter(|(_,n)|online(n,now)).count(),nodes.len()),
            "Workflows":format!("{} enabled / {} registered",workflows.iter().filter(|(_,w)|w["workflow"]["enabled"]==true).count(),workflows.len()),"Runs by state":states});
        return Ok(format!(
            "{}\n\nNODES\n{}\n\nWORKFLOWS\n{}\n\nUse `converyn get runs` for execution history; `converyn get runs ID` for receipts.",
            detail(&summary, 0),
            node_table(&value["nodes"], now, matches!(format, Format::Wide)),
            workflow_table(&value["workflows"])
        ));
    }
    if let Some(bindings) = value.get("bindings") {
        let summary = table(
            &["BINDING", "SUBJECT", "ROLE", "WORKFLOW", "PRESETS"],
            items(bindings)
                .into_iter()
                .map(|(id, b)| {
                    vec![
                        id,
                        format!(
                            "{}:{}",
                            cell(&b["subject"]["kind"]),
                            cell(&b["subject"]["uid"])
                        ),
                        cell(&b["role"]),
                        b["workflow"].as_str().unwrap_or("*").into(),
                        if b["presets"].as_array().is_some_and(|v| v.is_empty()) {
                            "*".into()
                        } else {
                            cell(&b["presets"])
                        },
                    ]
                })
                .collect(),
        );
        let groups = value
            .get("groups")
            .map(|g| {
                format!(
                    "\n\nGROUPS\n{}",
                    table(
                        &["NAME", "UID", "MEMBERS"],
                        items(g)
                            .into_iter()
                            .map(|(uid, g)| vec![cell(&g["name"]), uid, cell(&g["members"])])
                            .collect()
                    )
                )
            })
            .unwrap_or_default();
        return Ok(format!(
            "Access revision: {}\n{}{groups}\n\nCheck a capability: converyn auth can-i execute --workflow WORKFLOW --preset PRESET",
            cell(&value["revision"]),
            summary
        ));
    }
    if let Some(v) = value.get("identities") {
        let summary = table(
            &["NAME", "UID", "DISABLED", "REVISION"],
            items(v)
                .into_iter()
                .map(|(id, v)| {
                    vec![
                        field(v, "name"),
                        id,
                        field(v, "disabled"),
                        field(v, "revision"),
                    ]
                })
                .collect(),
        );
        let next=items(v).first().map(|(id,_)|format!("\n\nDetails and certificates: converyn get identities {id}\nFilter certificates: converyn get credentials --identity {id}\nExport current local credentials: converyn export credentials --current --output ./browser.p12\nExport CA: converyn export credentials --ca --output ./ca.crt\nGUI instructions for selected identity: converyn auth gui-login --output-dir ./gui-login")).unwrap_or_default();
        return Ok(format!("{summary}{next}"));
    }
    if let Some(v) = value.get("nodes")
        && v.is_object()
    {
        if value["node_detail"] == true {
            let sections = items(v)
                .into_iter()
                .map(|(id, n)| {
                    format!(
                        "Node: {id}\nInventory: {} · Resources: {}\n{}",
                        freshness(n, "inventory", now),
                        freshness(n, "resources", now),
                        detail(n, 0)
                    )
                })
                .collect::<Vec<_>>();
            return Ok(sections.join("\n\n"));
        }
        return Ok(node_table(v, now, matches!(format, Format::Wide)));
    }
    if let Some(v) = value.get("runs") {
        return Ok(run_table(v));
    }
    if let Some(v) = value.get("workflows") {
        if items(v).is_empty() {
            return Ok("No workflows are available. Check your grants with `converyn auth status`; ask a maintainer to register a workflow.".into());
        }
        return Ok(workflow_table(v));
    }
    if let Some(v) = value.get("workflow")
        && v.is_object()
    {
        return Ok(format!(
            "{}\n\n{}",
            workflow_table(&json!([value])),
            detail(
                &json!({"Parameters":value["definition"]["parameters"],"Values":value["definition"]["values"],"Immutable":value["definition"]["immutable"],"Node values":value["definition"]["node_values"],"Inventory":value["definition"]["inventory"],"Targets":value["definition"]["targets"]}),
                0
            )
        ));
    }
    if let Some(v) = value.get("credentials") {
        let summary = credential_table(v);
        let next=items(v).first().map(|(_,c)|format!("\n\nDetails, storage and export: converyn get credentials {}\nPKCS#12 from selected local config: converyn export credentials --current --output ./browser.p12\nCA: converyn export credentials --ca --output ./ca.crt\nGUI instructions: converyn auth gui-login --output-dir ./gui-login",field(c,"uid"))).unwrap_or_default();
        return Ok(format!("{summary}{next}"));
    }
    if let Some(v) = value.get("node_groups") {
        return Ok(table(
            &["NAME", "REVISION", "NODES"],
            items(v)
                .into_iter()
                .map(|(id, v)| {
                    vec![
                        id,
                        field(v, "revision"),
                        v["node_uids"]
                            .as_array()
                            .map(|a| a.iter().map(cell).collect::<Vec<_>>().join(","))
                            .unwrap_or_else(|| "-".into()),
                    ]
                })
                .collect(),
        ));
    }
    if let Some(v) = value.get("contexts")
        && v.is_array()
    {
        return Ok(table(
            &["CURRENT", "NAME", "CLUSTER", "SERVERS"],
            items(v)
                .into_iter()
                .map(|(_, v)| {
                    vec![
                        if v["current"] == true { "*" } else { "" }.into(),
                        field(v, "name"),
                        field(v, "cluster"),
                        field(v, "servers"),
                    ]
                })
                .collect(),
        ));
    }
    if let Some(v) = value.get("plugins") {
        return Ok(table(
            &["ID", "VERSION", "CONTRACT", "ACTIONS"],
            items(v)
                .into_iter()
                .map(|(_, v)| {
                    vec![
                        field(v, "identity.id"),
                        field(v, "identity.version"),
                        field(v, "identity.contract"),
                        field(v, "actions"),
                    ]
                })
                .collect(),
        ));
    }
    if let Some(v) = value.get("targets") {
        return Ok(table(
            &["NAME", "PLUGIN", "CONNECTION", "REVISION"],
            items(v)
                .into_iter()
                .map(|(id, v)| {
                    vec![
                        id,
                        field(v, "plugin.id"),
                        field(v, "connection"),
                        field(v, "revision"),
                    ]
                })
                .collect(),
        ));
    }
    if let Some(v) = value.get("membership") {
        return Ok(table(
            &["ID", "ADDRESS", "VOTER"],
            items(&v["membership"]["nodes"])
                .into_iter()
                .map(|(id, n)| {
                    let voter = value["voters"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| cell(v) == id));
                    vec![id, field(n, "addr"), voter.to_string()]
                })
                .collect(),
        ));
    }
    if let Some(attempts) = value.get("attempts") {
        let summary = json!({"Run":value["id"],"Status":value["status"],"Revision":value["revision"],"Workflow":value["execution"]["request"]["workflow_uid"],"Target":value["execution"]["request"]["target"],"Actor":value["execution"]["actor"]["name"],"Submitted":timestamp(&value["execution"]["submitted_ms"]),"Parameters":value["execution"]["parameters"],"Platform node group":value["plan"]["input"]["hosts"]["node_group_uid"],"Cancel requested":value["cancel_requested"],"Pause requested":value["pause_requested"],"Preparation errors":value["preparation_errors"]});
        let mut headers = vec![
            "STEP",
            "NODE",
            "PHASE",
            "OPERATION",
            "STATUS",
            "EXIT",
            "REASON",
        ];
        let wide = matches!(format, Format::Wide);
        if wide {
            headers.push("WORK ID");
        }
        let rows = value["plan"]["work"]
            .as_array()
            .map(|work| {
                work.iter()
                    .map(|w| {
                        let id = w["id"].as_str().unwrap_or_default();
                        let v = &attempts[id];
                        let mut row = vec![
                            field(w, "step_id"),
                            field(w, "node_uid"),
                            field(w, "phase"),
                            if w["operation"]["execution"] == "host" {
                                format!("{} (root)", field(w, "operation.kind"))
                            } else {
                                field(w, "operation.kind")
                            },
                            field(v, "status"),
                            if v["receipt"]["exit_code"].is_null() {
                                field(v, "receipt.native.package.transaction_exit")
                            } else {
                                field(v, "receipt.exit_code")
                            },
                            [
                                "reason",
                                "receipt.diagnostic",
                                "receipt.native.package.diagnostic",
                                "receipt.native.error",
                            ]
                            .into_iter()
                            .find(|path| {
                                let value = field(v, path);
                                value != "-" && !value.is_empty()
                            })
                            .map(|path| field(v, path))
                            .unwrap_or_else(|| "-".into()),
                        ];
                        if wide {
                            row.push(clean(id));
                        }
                        row
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Ok(format!(
            "{}\n\n{}\n\n{}\n\n{}",
            detail(&summary, 0),
            table(&headers, rows),
            work_log_commands(value),
            step_outputs(value)
        ));
    }

    if let Some(logs) = value.as_object().filter(|rows| {
        !rows.is_empty()
            && rows
                .values()
                .all(|v| v.get("logs").is_some() && v.get("status").is_some())
    }) {
        let mut sections = Vec::new();
        for (id, work) in logs {
            let mut text = format!("Work: {}  Status: {}", clean(id), field(work, "status"));
            for stream in ["stdout", "stderr"] {
                let log = &work["logs"][stream];
                writeln!(
                    &mut text,
                    "\n{stream} (tail; seen={}, stored={}, truncated={}):",
                    cell(&log["seen_bytes"]),
                    cell(&log["stored_bytes"]),
                    cell(&log["truncated"])
                )?;
                text.push_str(
                    &log["tail"]
                        .as_str()
                        .map(|s| s.lines().map(clean).collect::<Vec<_>>().join("\n"))
                        .unwrap_or_else(|| "No committed log tail.".into()),
                );
            }
            sections.push(text);
        }
        return Ok(sections.join("\n\n"));
    }
    let mut text = String::new();
    write!(&mut text, "{}", detail(value, 0))?;
    Ok(text)
}

fn step_outputs(value: &Value) -> String {
    let mut rows = Vec::new();
    if let Some(work) = value["plan"]["work"].as_array() {
        for w in work {
            let id = w["id"].as_str().unwrap_or_default();
            if let Some(outputs) = value["attempts"][id]["receipt"]["outputs"].as_object() {
                for (name, output) in outputs {
                    rows.push(vec![
                        field(w, "step_id"),
                        field(w, "node_uid"),
                        clean(name),
                        if output["kind"] == "secret" {
                            "[secret]".into()
                        } else {
                            cell(&output["value"])
                        },
                    ]);
                }
            }
        }
    }
    if rows.is_empty() {
        String::new()
    } else {
        format!(
            "Step outputs:\n{}",
            table(&["STEP", "NODE", "NAME", "VALUE"], rows)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_tables_keep_ids_and_escape_terminal_controls() {
        let v = json!({"identities":{"user-1":{"name":"操作员\n\u{1b}[31m","role":"admin","disabled":false,"revision":1}}});
        let out = render(&v, Format::Table, 0).unwrap();
        assert!(out.contains("NAME") && out.contains("user-1") && out.contains("DISABLED"));
        assert!(!out.contains('\u{1b}'));
        assert!(out.contains("\\n"));
        assert_eq!(
            serde_json::from_str::<Value>(&render(&v, Format::Json, 0).unwrap()).unwrap(),
            v
        );
    }
    #[test]
    fn node_heartbeat_boundary_and_missing_observation() {
        let v = json!({"nodes":{"fresh":{"observed_ms":99999},"stale":{"observed_ms":40000},"unknown":{}}});
        let out = render(&v, Format::Table, 100000).unwrap();
        assert!(
            out.lines()
                .find(|l| l.starts_with("fresh"))
                .unwrap()
                .contains("Online")
        );
        assert!(
            out.lines()
                .find(|l| l.starts_with("stale"))
                .unwrap()
                .contains("Offline")
        );
        assert!(
            out.lines()
                .find(|l| l.starts_with("unknown"))
                .unwrap()
                .contains("Unknown")
        );
    }
    #[test]
    fn workflow_discovery_and_empty_results() {
        let v = json!({"workflows":[{"workflow":{"uid":"deploy","enabled":false,"revision":3},"definition":{"name":"发布","targets":{"prod":{}},"parameters":{"VERSION":{}}}}]});
        let out = render(&v, Format::Table, 0).unwrap();
        for text in ["deploy", "发布", "false", "prod", "VERSION"] {
            assert!(out.contains(text));
        }
        assert_eq!(
            render(&json!({"nodes":{}}), Format::Table, 0).unwrap(),
            "No resources found."
        );
    }
    #[test]
    fn run_details_offer_one_log_command_per_textual_work_in_plan_order() {
        let run = json!({"id":"run-a","attempts":{"w1":{"status":"succeeded"},"w2":{"status":"succeeded"}},"plan":{"work":[{"id":"w1","step_id":"write","node_uid":"n1","operation":{"kind":"file"}},{"id":"w2","step_id":"verify","node_uid":"n1","phase":"verify","operation":{"kind":"exec"}}]}});
        let text = render(&run, Format::Table, 0).unwrap();
        assert!(text.contains("converyn logs runs run-a --step verify --node n1"));
        assert!(text.contains("--full") && text.contains("--follow") && text.contains("-o wide"));
        assert!(!text.contains("logs run-a --step write"));
        assert_eq!(
            serde_json::from_str::<Value>(&render(&run, Format::Json, 0).unwrap()).unwrap(),
            run
        );
    }
    #[test]
    fn identity_and_certificate_details_show_location_without_pem_or_secret_values() {
        let identity = json!({"detail_resource":{"kind":"identities","id":"u"},"identities":{"u":{"uid":"u","name":"OP","role":"admin","revision":1}},"credentials":[{"uid":"c","identity_uid":"u","active":true}],"local_credential":{"matches_selected_config":true,"config_file":"/secure/config.json","private_key":"credentials.private_key_pem"}});
        let text = render(&identity, Format::Table, 0).unwrap();
        assert!(
            text.contains("Identity: u")
                && text.contains("Registered certificates:")
                && text.contains("/secure/config.json")
        );
        let cert = json!({"detail_resource":{"kind":"credentials","id":"c"},"credential":{"uid":"c","certificate_pem":"PUBLIC_PEM_DO_NOT_DUMP"},"storage":{"private_key":"client-held"},"local_credential":{"matches_selected_config":false}});
        let text = render(&cert, Format::Table, 0).unwrap();
        assert!(
            text.contains("export credentials --id c")
                && text.contains("not be recovered")
                && !text.contains("PUBLIC_PEM_DO_NOT_DUMP")
        );
    }
    #[test]
    fn json_is_pretty_and_preserves_pagination() {
        let v = json!({"runs":[],"next_cursor":"abc"});
        let out = render(&v, Format::Json, 0).unwrap();
        assert!(out.contains("\n  \"next_cursor\""));
        assert_eq!(serde_json::from_str::<Value>(&out).unwrap(), v);
    }
    #[test]
    fn runs_sort_by_submission_and_unknown_times_last() {
        let v = json!({"runs":[{"id":"old","execution":{"submitted_ms":1000}},{"id":"archive","archived":true},{"id":"new","execution":{"submitted_ms":2000}}]});
        let out = render(&v, Format::Table, 0).unwrap();
        let ids = out
            .lines()
            .skip(1)
            .map(|l| l.split_whitespace().next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["new", "old", "archive"]);
    }
    #[test]
    fn credentials_use_api_active_and_revocation_fields() {
        let v = json!({"credentials":[{"uid":"expired","active":false,"revoked_ms":null},{"uid":"revoked","active":false,"revoked_ms":1000}]});
        let out = render(&v, Format::Table, 0).unwrap();
        let rows = out
            .lines()
            .skip(1)
            .map(|l| l.split_whitespace().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(&rows[0][3..5], &["false", "false"]);
        assert_eq!(&rows[1][3..5], &["false", "true"]);
    }
    #[test]
    fn human_logs_keep_lines_and_escape_terminal_controls() {
        let v = json!({"work-1":{"status":"failed","logs":{"stdout":{"tail":"one\ntwo\n\u{1b}[31m","seen_bytes":16,"stored_bytes":12,"truncated":true}}}});
        let out = render(&v, Format::Table, 0).unwrap();
        assert!(out.contains("one\ntwo\n"));
        assert!(out.contains("truncated=true"));
        assert!(!out.contains('\u{1b}'));
    }
    #[test]
    fn package_failures_show_transaction_exit_and_real_diagnostic() {
        let value = json!({"id":"run-package","status":"failed","plan":{"work":[{"id":"work","step_id":"install","node_uid":"node","phase":"rollout","operation":{"kind":"package"}}]},"attempts":{"work":{"status":"failed","receipt":{"exit_code":null,"diagnostic":null,"native":{"error":"package failed","package":{"transaction_exit":1,"diagnostic":"scriptlet interpreter denied"}}}}}});
        let text = render(&value, Format::Table, 0).unwrap();
        assert!(text.contains("scriptlet interpreter denied"));
        assert!(
            text.lines()
                .any(|line| line.contains("install") && line.contains('1'))
        );
    }
}
