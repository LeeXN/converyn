//! Current-only stores cannot be upgraded by rewriting their schema marker.
use anyhow::{Result, ensure};
use std::path::Path;
pub fn migrate(directory: &Path) -> Result<()> {
    let db = rusqlite::Connection::open_with_flags(
        directory.join("state.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let bytes: Vec<u8> = db.query_row(
        "SELECT value FROM metadata WHERE key='schema_version'",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        serde_json::from_slice::<u32>(&bytes)? == super::store::SCHEMA_VERSION,
        "unsupported storage schema; provision a fresh installation with matching controllers and Agents"
    );
    Ok(())
}
