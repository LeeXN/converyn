//! Object-indexed state persistence. SQLite commits changed objects and the
//! Raft applied position together; immutable historical objects are not rewritten
//! for heartbeats. Full snapshots remain portable canonical JSON.
use anyhow::{Context, Result, ensure};
use converyn_core::orchestration::machine::State;
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
pub fn create(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS state_objects(collection TEXT NOT NULL, object_id TEXT NOT NULL, kind TEXT NOT NULL, value BLOB NOT NULL, PRIMARY KEY(collection,object_id)) WITHOUT ROWID;")?;
    Ok(())
}
fn put(
    db: &Connection,
    collection: &str,
    id: &str,
    kind: &str,
    value: &impl Serialize,
) -> Result<()> {
    db.execute("INSERT INTO state_objects(collection,object_id,kind,value) VALUES(?1,?2,?3,?4) ON CONFLICT(collection,object_id) DO UPDATE SET kind=excluded.kind,value=excluded.value",params![collection,id,kind,serde_json::to_vec(value)?])?;
    Ok(())
}
fn map<T: Serialize + PartialEq>(
    db: &Connection,
    path: &str,
    old: &BTreeMap<String, T>,
    new: &BTreeMap<String, T>,
) -> Result<()> {
    for (id, value) in new {
        if old.get(id) != Some(value) {
            put(db, path, id, "map", value)?;
        }
    }
    for id in old.keys() {
        if !new.contains_key(id) {
            db.execute(
                "DELETE FROM state_objects WHERE collection=?1 AND object_id=?2",
                params![path, id],
            )?;
        }
    }
    Ok(())
}
fn sequence<T: Serialize + PartialEq>(
    db: &Connection,
    path: &str,
    old: &[T],
    new: &[T],
) -> Result<()> {
    for (index, value) in new.iter().enumerate() {
        if old.get(index) != Some(value) {
            put(db, path, &format!("{index:016x}"), "sequence", value)?;
        }
    }
    if new.len() < old.len() {
        db.execute(
            "DELETE FROM state_objects WHERE collection=?1 AND object_id>=?2",
            params![path, format!("{:016x}", new.len())],
        )?;
    }
    Ok(())
}
pub fn save(db: &Connection, old: &State, new: &State) -> Result<()> {
    macro_rules! maps { ($($($field:ident).+),+ $(,)?) => { $(map(db,&concat!("/",stringify!($($field).+)).replace(" . ","/").replace('.',"/"),&old.$($field).+,&new.$($field).+)?;)+ }; }
    maps!(
        workflows.workflows,
        workflows.revisions,
        workflows.executions,
        personnel.identities,
        personnel.credentials,
        personnel.operations,
        cold_audit,
        retirement_proofs,
        log_manifests,
        log_pruned,
        targets,
        controlplane.operations,
        history.records,
        drift,
        owner_projects,
        deployments,
        observations,
        scripts,
        node_groups,
        runs,
        execution_locks,
        resource_owners,
        idempotency,
        registry.bootstraps,
        registry.nodes,
        registry.token_index,
        administration.owner_revisions,
        administration.artifacts,
        administration.secrets,
        administration.windows,
        administration.retired_node_groups,
        identities.bindings,
    );
    if old.personnel.access != new.personnel.access {
        put(db, "/personnel/access", "", "value", &new.personnel.access)?;
    }
    if old.log_store_id != new.log_store_id {
        put(db, "/log_store_id", "", "value", &new.log_store_id)?;
    }
    sequence(db, "/audit", &old.audit, &new.audit)?;
    if (
        old.history.keep_runs,
        old.history.definition_hot_bytes,
        old.history.configuration_revision,
    ) != (
        new.history.keep_runs,
        new.history.definition_hot_bytes,
        new.history.configuration_revision,
    ) {
        put(
            db,
            "/history/keep_runs",
            "",
            "value",
            &new.history.keep_runs,
        )?;
        put(
            db,
            "/history/definition_hot_bytes",
            "",
            "value",
            &new.history.definition_hot_bytes,
        )?;
        put(
            db,
            "/history/configuration_revision",
            "",
            "value",
            &new.history.configuration_revision,
        )?;
    }
    sequence(db, "/history/audit", &old.history.audit, &new.history.audit)?;

    sequence(
        db,
        "/registry/audit",
        &old.registry.audit,
        &new.registry.audit,
    )?;
    sequence(
        db,
        "/administration/audit",
        &old.administration.audit,
        &new.administration.audit,
    )?;
    sequence(
        db,
        "/identities/audit",
        &old.identities.audit,
        &new.identities.audit,
    )?;
    if old.administration.maintenance != new.administration.maintenance {
        put(
            db,
            "/administration/maintenance",
            "",
            "value",
            &new.administration.maintenance,
        )?;
    }
    if old.controlplane.active != new.controlplane.active {
        put(
            db,
            "/controlplane/active",
            "",
            "value",
            &new.controlplane.active,
        )?;
    }
    if old.run_sequence != new.run_sequence {
        put(db, "/run_sequence", "", "value", &new.run_sequence)?;
    }

    Ok(())
}
pub fn load(db: &Connection) -> Result<State> {
    let mut value = serde_json::to_value(State::default())?;
    value["run_sequence"] = 0.into();
    value["controlplane"] =
        serde_json::to_value(converyn_core::orchestration::controlplane::Catalog::default())?;
    value["history"] =
        serde_json::to_value(converyn_core::orchestration::history::History::default())?;
    value["administration"]["secrets"] = serde_json::json!({});
    value["administration"]["windows"] = serde_json::json!({});
    value["administration"]["retired_node_groups"] = serde_json::json!({});
    let mut query = db.prepare(
        "SELECT collection,object_id,kind,value FROM state_objects ORDER BY collection,object_id",
    )?;
    let rows = query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    for row in rows {
        let (path, id, kind, data) = row?;
        let entry: Value = serde_json::from_slice(&data)?;
        let target = value
            .pointer_mut(&path)
            .context("unknown persisted state collection")?;
        match kind.as_str() {
            "map" => {
                target
                    .as_object_mut()
                    .context("state map required")?
                    .insert(id, entry);
            }
            "sequence" => {
                let array = target.as_array_mut().context("state sequence required")?;
                ensure!(
                    usize::from_str_radix(&id, 16)? == array.len(),
                    "state sequence gap"
                );
                array.push(entry);
            }
            "value" => {
                ensure!(id.is_empty(), "invalid scalar state key");
                *target = entry;
            }
            _ => anyhow::bail!("unknown persisted state record kind"),
        }
    }
    Ok(serde_json::from_value(value)?)
}
