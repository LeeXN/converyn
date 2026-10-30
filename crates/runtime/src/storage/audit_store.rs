//! Bounded hot tails and immutable cold audit share the business transaction.
//! A small hash-chain head commits each stream without retaining its corpus in RAM.
use anyhow::{Result, ensure};
use converyn_core::{
    model::domain::digest,
    orchestration::machine::{AuditHead, State},
};
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
pub const HOT: usize = 128;

fn append(
    db: &Connection,
    head: &mut AuditHead,
    stream: &str,
    value: &impl Serialize,
) -> Result<()> {
    let uid = format!("{stream}/{:016x}", head.count);
    let data = serde_json::to_vec(value)?;
    use sha2::{Digest, Sha256};
    let sha = hex::encode(Sha256::digest(&data));
    db.execute(
        "INSERT INTO cold_history(kind,uid,sha256,data) VALUES('audit',?1,?2,?3)",
        params![uid, sha, data],
    )?;
    head.sha256 = digest(&(&head.sha256, head.count, &sha));
    head.count = head
        .count
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("audit sequence exhausted"))?;
    Ok(())
}
fn tail<T: Serialize>(
    db: &Connection,
    heads: &mut std::collections::BTreeMap<String, AuditHead>,
    stream: &str,
    values: &mut Vec<T>,
) -> Result<()> {
    let excess = values.len().saturating_sub(HOT);
    if excess > 0 {
        let head = heads.entry(stream.into()).or_default();
        for value in values.drain(..excess) {
            append(db, head, stream, &value)?;
        }
    }
    Ok(())
}
pub fn compact(db: &Connection, state: &mut State) -> Result<()> {
    // DeleteNodeGroup replay must remain valid after its old audit entry is cold.
    for change in &state.administration.audit {
        if let converyn_core::orchestration::administration::Change::DeleteNodeGroup {
            uid, ..
        } = change
        {
            state
                .retirement_proofs
                .entry(format!("node-group/{uid}"))
                .or_insert_with(|| digest(change));
        }
    }
    for row in &state.registry.audit {
        if row["action"] == "retired"
            && let (Some(uid), Some(evidence)) =
                (row["node_uid"].as_str(), row["evidence"].as_str())
            && let Some(node) = state.registry.nodes.get(uid)
        {
            state
                .retirement_proofs
                .entry(format!("node/{uid}"))
                .or_insert_with(|| digest(&(uid, &node.certificate.fingerprint, evidence)));
        }
    }
    tail(db, &mut state.cold_audit, "resolution", &mut state.audit)?;
    tail(
        db,
        &mut state.cold_audit,
        "identity",
        &mut state.registry.audit,
    )?;
    tail(
        db,
        &mut state.cold_audit,
        "security",
        &mut state.identities.audit,
    )?;
    tail(
        db,
        &mut state.cold_audit,
        "administration",
        &mut state.administration.audit,
    )?;
    tail(
        db,
        &mut state.cold_audit,
        "history",
        &mut state.history.audit,
    )?;
    Ok(())
}
pub fn validate(db: &Connection, state: &State) -> Result<()> {
    let mut expected = 0u64;
    for (stream, head) in &state.cold_audit {
        let mut actual = AuditHead::default();
        let mut q=db.prepare("SELECT uid,sha256,data FROM cold_history WHERE kind='audit' AND uid>=?1 AND uid<?2 ORDER BY uid")?;
        let rows = q.query_map(params![format!("{stream}/"), format!("{stream}0")], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        for row in rows {
            let (uid, sha, data) = row?;
            use sha2::{Digest, Sha256};
            ensure!(
                uid == format!("{stream}/{:016x}", actual.count)
                    && sha == hex::encode(Sha256::digest(data)),
                "audit gap or checksum mismatch"
            );
            actual.sha256 = digest(&(&actual.sha256, actual.count, &sha));
            actual.count += 1;
        }
        ensure!(&actual == head, "audit head mismatch");
        expected += head.count;
    }
    let count: u64 = db.query_row(
        "SELECT count(*) FROM cold_history WHERE kind='audit'",
        [],
        |r| r.get(0),
    )?;
    ensure!(count == expected, "unindexed audit rows");
    Ok(())
}
pub fn page<T: DeserializeOwned>(
    db: &Connection,
    stream: &str,
    after: u64,
    limit: usize,
) -> Result<Vec<T>> {
    filtered_page(db, stream, after, limit, "all")?
        .0
        .into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<_, _>>()
        .map_err(Into::into)
}

/// Filter before LIMIT. Cursors refer to immutable audit sequence positions, not
/// to the number of matches, so long runs of successful heartbeats cannot hide events.
pub fn filtered_page(
    db: &Connection,
    stream: &str,
    after: u64,
    limit: usize,
    view: &str,
) -> Result<(Vec<serde_json::Value>, Option<u64>)> {
    ensure!(
        matches!(view, "all" | "activity" | "observations"),
        "invalid audit view"
    );
    let mut q = db.prepare("SELECT uid,sha256,data FROM cold_history WHERE kind='audit' AND uid>=?1 AND uid<?2 AND (?4='all' OR (?4='activity' AND (json_extract(CAST(data AS TEXT),'$.type') IS NOT 'observe_node' OR json_extract(CAST(data AS TEXT),'$.ok') IS NOT 1)) OR (?4='observations' AND json_extract(CAST(data AS TEXT),'$.type')='observe_node')) ORDER BY uid LIMIT ?3")?;
    let rows = q
        .query_map(
            params![
                format!("{stream}/{after:016x}"),
                format!("{stream}0"),
                limit + 1,
                view
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let more = rows.len() > limit;
    let mut records = Vec::new();
    let mut next = None;
    for (index, (uid, sha, data)) in rows.into_iter().enumerate() {
        use sha2::{Digest, Sha256};
        ensure!(
            sha == hex::encode(Sha256::digest(&data)),
            "audit checksum mismatch"
        );
        if index < limit {
            let sequence = u64::from_str_radix(uid.rsplit('/').next().unwrap_or_default(), 16)?;
            next = sequence.checked_add(1);
            records.push(serde_json::from_slice(&data)?);
        }
    }
    Ok((records, if more { next } else { None }))
}
pub fn record(
    db: &Connection,
    state: &mut State,
    index: u64,
    command: &converyn_core::orchestration::machine::Command,
    reply: &converyn_core::orchestration::machine::Reply,
) -> Result<()> {
    use converyn_core::orchestration::machine::Command;
    let mut c = command;
    let mut principal = "internal";
    let mut fingerprint = "";
    let mut request_id = None;
    let mut observed_ms = None;
    loop {
        match c {
            Command::Authorized {
                authorization,
                command,
            } => {
                request_id = authorization.request_id.as_ref();
                observed_ms = authorization.observed_ms;
                principal = &authorization.principal;
                fingerprint = &authorization.fingerprint;
                c = command;
            }
            Command::Prepared { command, .. } => c = command,
            _ => break,
        }
    }
    let v = serde_json::to_value(c)?;
    // Store identity and immutable command digest, never script contents, resolved
    // secrets or credential values. Detailed evidence stays in typed audit streams.
    let project = match c {
        Command::CreateRun { plan, .. } | Command::ApplyDeployment { plan, .. } => {
            plan.input.project.as_deref()
        }
        Command::Report { run_id, .. }
        | Command::Claim { run_id, .. }
        | Command::Cancel { run_id }
        | Command::RetryInputs { run_id, .. }
        | Command::PauseRun { run_id, .. }
        | Command::Resolve { run_id, .. }
        | Command::MarkUncertain { run_id, .. }
        | Command::Materialize { run_id, .. } => state
            .runs
            .get(run_id)
            .and_then(|r| r.plan.input.project.as_deref()),
        Command::Administer { change } => {
            use converyn_core::orchestration::administration::Change;
            match change {
                Change::Target { target } => Some(target.project.as_str()),
                Change::LogFinal { manifest } => state
                    .runs
                    .get(&manifest.run_id)
                    .and_then(|r| r.plan.input.project.as_deref()),
                _ => None,
            }
        }
        _ => None,
    };
    let human = state.personnel.identities.get(principal);
    let credential = state
        .personnel
        .credentials
        .values()
        .find(|c| c.fingerprint == fingerprint);
    let execution = reply
        .run_id
        .as_ref()
        .and_then(|id| state.workflows.executions.get(id));
    let row = json!({"identity_name":human.map(|i| &i.name),"credential_uid":credential.map(|c| &c.uid),"execution":execution,"project":project,"request_id":request_id,"observed_ms":observed_ms,"log_index":index,"principal":principal,"fingerprint":fingerprint,"type":v["type"],"action":v.pointer("/change/action"),"command_sha256":digest(c),"ok":reply.ok,"code":reply.code,"run_id":reply.run_id});
    append(
        db,
        state.cold_audit.entry("commands".into()).or_default(),
        "commands",
        &row,
    )
}

#[cfg(test)]
mod view_tests {
    use super::*;
    #[test]
    fn filtering_keeps_failures_and_uses_real_sequence_cursors() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(crate::storage::history_store::TABLE_SQL)
            .unwrap();
        let mut head = AuditHead::default();
        for i in 0..220 {
            append(&db, &mut head, "commands", &json!({"type":if i == 105 {"cancel"} else {"observe_node"},"ok":i != 210,"sequence":i})).unwrap();
        }
        let (first, next) = filtered_page(&db, "commands", 0, 1, "activity").unwrap();
        assert_eq!(first[0]["sequence"], 105);
        assert_eq!(next, Some(106));
        let (last, next) = filtered_page(&db, "commands", next.unwrap(), 1, "activity").unwrap();
        assert_eq!(last[0]["sequence"], 210);
        assert_eq!(next, None);
        let (observations, next) = filtered_page(&db, "commands", 104, 2, "observations").unwrap();
        assert_eq!(observations[0]["sequence"], 104);
        assert_eq!(observations[1]["sequence"], 106);
        assert_eq!(next, Some(107));
        assert_eq!(
            page::<serde_json::Value>(&db, "commands", 0, 100)
                .unwrap()
                .len(),
            100
        );
        assert!(filtered_page(&db, "commands", 0, 2, "invalid").is_err());
        db.execute(
            "UPDATE cold_history SET sha256='invalid' WHERE uid='commands/0000000000000069'",
            [],
        )
        .unwrap();
        assert!(filtered_page(&db, "commands", 0, 1, "activity").is_err());
    }
}
