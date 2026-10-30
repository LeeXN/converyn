//! Immutable cold rows share the state machine's transaction and travel in
//! disk snapshots. A missing/corrupt row is an I/O failure, never empty history.
use anyhow::{Context, Result, ensure};
use converyn_core::orchestration::history::{Kind, Record};
use converyn_core::orchestration::machine::{Command, State};
use rusqlite::{Connection, OptionalExtension, params};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
pub const TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS cold_history(kind TEXT NOT NULL,uid TEXT NOT NULL,sha256 TEXT NOT NULL,data BLOB NOT NULL,PRIMARY KEY(kind,uid)) WITHOUT ROWID;";
pub fn create(db: &Connection) -> Result<()> {
    db.execute_batch(TABLE_SQL)?;
    Ok(())
}
fn inner(command: &Command) -> &Command {
    match command {
        Command::Prepared { command, .. } | Command::Authorized { command, .. } => inner(command),
        other => other,
    }
}
pub fn prepare(state: &State, command: &Command) -> Result<Option<Vec<u8>>> {
    let Command::History {
        change: converyn_core::orchestration::history::Change::Archive { kind, uid, .. },
    } = inner(command)
    else {
        return Ok(None);
    };
    Ok(match kind {
        Kind::Run => state.runs.get(uid).map(serde_json::to_vec).transpose()?,
        Kind::Definition => state.scripts.get(uid).map(serde_json::to_vec).transpose()?,
    })
}
pub fn persist(
    db: &Connection,
    state: &State,
    command: &Command,
    data: Option<Vec<u8>>,
) -> Result<()> {
    if matches!(
        inner(command),
        Command::History {
            change: converyn_core::orchestration::history::Change::ClearRuns { .. }
        }
    ) {
        let mut query = db.prepare("SELECT uid FROM cold_history WHERE kind='run'")?;
        let ids = query
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(query);
        for uid in ids {
            // A retried clear must not remove newly archived Runs.
            if state.history.get(Kind::Run, &uid).is_none() {
                db.execute(
                    "DELETE FROM cold_history WHERE kind='run' AND uid=?1",
                    [&uid],
                )?;
            }
        }
        return Ok(());
    }
    let Some(data) = data else { return Ok(()) };
    let Command::History {
        change: converyn_core::orchestration::history::Change::Archive { kind, uid, .. },
    } = inner(command)
    else {
        return Ok(());
    };
    let record = state
        .history
        .get(*kind, uid)
        .context("committed history reference missing")?;
    ensure!(
        data.len() as u64 == record.bytes && hex::encode(Sha256::digest(&data)) == record.sha256,
        "history data differs from committed reference"
    );
    let existing: Option<String> = db
        .query_row(
            "SELECT sha256 FROM cold_history WHERE kind=?1 AND uid=?2",
            params![kind.name(), uid],
            |r| r.get(0),
        )
        .optional()?;
    ensure!(
        existing.as_ref().is_none_or(|sha| sha == &record.sha256),
        "immutable cold history conflict"
    );
    if existing.is_none() {
        db.execute(
            "INSERT INTO cold_history(kind,uid,sha256,data) VALUES(?1,?2,?3,?4)",
            params![kind.name(), uid, record.sha256, data],
        )?;
    }
    Ok(())
}
pub fn raw(db: &Connection, record: &Record) -> Result<Vec<u8>> {
    ensure!(
        record.bytes <= 900_000_000,
        "history object exceeds SQLite size limit"
    );
    let (sha, data): (String, Vec<u8>) = db
        .query_row(
            "SELECT sha256,data FROM cold_history WHERE kind=?1 AND uid=?2",
            params![record.kind.name(), record.uid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .context("cold history object missing")?;
    ensure!(
        sha == record.sha256
            && data.len() as u64 == record.bytes
            && hex::encode(Sha256::digest(&data)) == record.sha256,
        "cold history checksum mismatch"
    );
    Ok(data)
}
pub fn load<T: DeserializeOwned>(db: &Connection, record: &Record) -> Result<T> {
    Ok(serde_json::from_slice(&raw(db, record)?)?)
}
pub fn validate(db: &Connection, state: &State) -> Result<()> {
    for record in state.history.records.values() {
        raw(db, record)?;
    }
    Ok(())
}
/// Pure domain transitions may need an old source release or a duplicate final
/// report. Rehydrate exactly that immutable object inside the apply transaction.
pub fn warm(db: &Connection, state: &mut State, command: &Command) -> Result<()> {
    let command = inner(command);
    let run = match command {
        Command::Claim { run_id, .. }
        | Command::Report { run_id, .. }
        | Command::Cancel { run_id }
        | Command::RetryInputs { run_id, .. }
        | Command::PauseRun { run_id, .. }
        | Command::Resolve { run_id, .. }
        | Command::MarkUncertain { run_id, .. }
        | Command::Materialize { run_id, .. } => Some(run_id),
        Command::Administer {
            change: converyn_core::orchestration::administration::Change::LogFinal { manifest },
        } => Some(&manifest.run_id),
        _ => None,
    };
    if let Some(uid) = run
        && !state.runs.contains_key(uid)
        && let Some(r) = state.history.get(Kind::Run, uid)
    {
        let value = load(db, r)?;
        state.runs.insert(uid.clone(), value);
    }
    Ok(())
}
