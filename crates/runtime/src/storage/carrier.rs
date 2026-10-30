//! SQLite carriers stream snapshots and cold history without materializing the
//! full corpus in memory. They contain no local identity, votes or Raft logs.
use crate::storage::store::{Machine, Position};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
pub const FOOTER: &[u8] = b"CONVERYN-BACKUP-V2\0";
fn read<T: DeserializeOwned>(db: &Connection, key: &str) -> Result<T> {
    let b: Vec<u8> = db.query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
        r.get(0)
    })?;
    Ok(serde_json::from_slice(&b)?)
}
fn write(db: &Connection, key: &str, value: &impl Serialize) -> Result<()> {
    db.execute(
        "INSERT INTO carrier.metadata(key,value) VALUES(?1,?2)",
        params![key, serde_json::to_vec(value)?],
    )?;
    Ok(())
}
pub fn build(database: &Path, directory: &Path, kind: &str) -> Result<(PathBuf, Machine, String)> {
    ensure!(
        matches!(kind, "snapshot" | "backup"),
        "invalid carrier kind"
    );
    std::fs::create_dir_all(directory)?;
    let path = directory.join(format!(
        "carrier-{}.sqlite",
        crate::credentials::pki::random_token()?
    ));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    let mut db = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute(
        "ATTACH DATABASE ?1 AS carrier",
        [path.to_str().context("invalid carrier path")?],
    )?;
    db.execute_batch("PRAGMA carrier.journal_mode=DELETE; PRAGMA carrier.synchronous=FULL; CREATE TABLE carrier.metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL); CREATE TABLE carrier.state_objects(collection TEXT NOT NULL,object_id TEXT NOT NULL,kind TEXT NOT NULL,value BLOB NOT NULL,PRIMARY KEY(collection,object_id)) WITHOUT ROWID; CREATE TABLE carrier.cold_history(kind TEXT NOT NULL,uid TEXT NOT NULL,sha256 TEXT NOT NULL,data BLOB NOT NULL,PRIMARY KEY(kind,uid)) WITHOUT ROWID;")?;
    let tx = db.transaction()?;
    // The read transaction captures applied position, hot objects and cold
    // bodies together while the main connection continues appending to WAL.
    let position: Position = read(&tx, "machine_position")?;
    let cluster: Option<Vec<u8>> = tx
        .query_row("SELECT value FROM metadata WHERE key='cluster'", [], |r| {
            r.get(0)
        })
        .optional()?;
    write(&tx, "machine_position", &position)?;
    write(
        &tx,
        "carrier_schema",
        &crate::storage::store::SCHEMA_VERSION,
    )?;
    write(&tx, "carrier_kind", &kind)?;
    write(
        &tx,
        "cluster",
        &cluster
            .map(|b| serde_json::from_slice::<String>(&b))
            .transpose()?
            .unwrap_or_else(|| "unbound".into()),
    )?;
    write(&tx, "created_ms", &crate::executors::host::now_ms())?;
    tx.execute(
        "INSERT INTO carrier.state_objects SELECT * FROM main.state_objects",
        [],
    )?;
    tx.execute(
        "INSERT INTO carrier.cold_history SELECT * FROM main.cold_history",
        [],
    )?;
    tx.commit()?;
    db.execute_batch("DETACH DATABASE carrier")?;
    drop(db);
    File::open(&path)?.sync_all()?;
    File::open(directory)?.sync_all()?;
    let (machine, _, _) = validate(&path)?;
    let sha = crate::artifacts::artifact::hash_file(&path)?.0;
    Ok((path, machine, sha))
}
pub fn sqlite(path: &Path) -> Result<bool> {
    let mut p = [0; 16];
    let n = File::open(path)?.read(&mut p)?;
    Ok(n == 16 && &p == b"SQLite format 3\0")
}
pub fn validate(path: &Path) -> Result<(Machine, String, String)> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.execute_batch("PRAGMA trusted_schema=OFF;")?;
    let objects = db
        .prepare(
            "SELECT name,type FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
        )?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        objects
            == vec![
                ("cold_history".into(), "table".into()),
                ("metadata".into(), "table".into()),
                ("state_objects".into(), "table".into())
            ],
        "unexpected carrier schema objects"
    );
    ensure!(
        db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? == "ok",
        "carrier integrity check failed"
    );
    let schema: u32 = read(&db, "carrier_schema")?;
    ensure!(
        schema == crate::storage::store::SCHEMA_VERSION,
        "unsupported carrier schema"
    );
    let position: Position = read(&db, "machine_position")?;
    let state = crate::storage::state_rows::load(&db)?;
    crate::storage::history_store::validate(&db, &state)?;
    let count: u64 = db.query_row(
        "SELECT count(*) FROM cold_history WHERE kind!='audit'",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        count == state.history.records.len() as u64,
        "carrier cold history index mismatch"
    );
    crate::storage::audit_store::validate(&db, &state)?;
    for run in state.runs.values() {
        run.plan.verify()?;
    }
    Ok((
        Machine {
            state,
            last_applied: position.last_applied,
            membership: position.membership,
        },
        read(&db, "cluster")?,
        read(&db, "carrier_kind")?,
    ))
}
pub fn append_footer(path: &Path, sha: &str) -> Result<()> {
    let mut f = OpenOptions::new().append(true).open(path)?;
    f.write_all(FOOTER)?;
    f.write_all(&hex::decode(sha)?)?;
    f.sync_all()?;
    Ok(())
}
pub fn verify_footer(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let footer = FOOTER.len() as u64 + 32;
    ensure!(size > footer, "backup is truncated");
    f.seek(SeekFrom::End(-(footer as i64)))?;
    let mut tag = vec![0; FOOTER.len()];
    f.read_exact(&mut tag)?;
    ensure!(tag == FOOTER, "backup footer missing");
    let mut expected = [0; 32];
    f.read_exact(&mut expected)?;
    f.seek(SeekFrom::Start(0))?;
    let mut remaining = size - footer;
    let mut hash = Sha256::new();
    let mut buf = [0; 256 * 1024];
    while remaining > 0 {
        let n = f.read(&mut buf[..usize::try_from(remaining.min(256 * 1024))?])?;
        ensure!(n > 0, "backup truncated while hashing");
        hash.update(&buf[..n]);
        remaining -= n as u64;
    }
    let actual = hash.finalize();
    ensure!(actual.as_slice() == expected, "backup checksum mismatch");
    Ok(hex::encode(actual))
}
