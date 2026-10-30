//! Rebuildable local index; the authoritative assignment still lives in Plan.
use anyhow::Result;
use converyn_core::orchestration::machine::State;
use rusqlite::{Connection, params};
use std::collections::BTreeSet;
pub fn create(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS node_work_index(node TEXT NOT NULL,run TEXT NOT NULL,PRIMARY KEY(node,run)) WITHOUT ROWID; CREATE INDEX IF NOT EXISTS node_work_by_run ON node_work_index(run);")?;
    Ok(())
}
pub fn update(db: &Connection, old: &State, new: &State) -> Result<()> {
    for (id, run) in &new.runs {
        if old
            .runs
            .get(id)
            .is_some_and(|v| std::sync::Arc::ptr_eq(v, run))
        {
            continue;
        }
        db.execute("DELETE FROM node_work_index WHERE run=?1", [id])?;
        if !run.status.terminal() {
            let nodes = run
                .plan
                .work
                .iter()
                .filter(|w| !run.attempts[&w.id].status.terminal())
                .map(|w| &w.node_uid)
                .collect::<BTreeSet<_>>();
            for node in nodes {
                db.execute(
                    "INSERT INTO node_work_index(node,run) VALUES(?1,?2)",
                    params![node, id],
                )?;
            }
        }
    }
    for id in old.runs.keys().filter(|id| !new.runs.contains_key(*id)) {
        db.execute("DELETE FROM node_work_index WHERE run=?1", [id])?;
    }
    Ok(())
}
