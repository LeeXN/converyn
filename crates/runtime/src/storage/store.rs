use std::{
    fmt::Debug,
    fs::{File, OpenOptions},
    ops::{Bound, RangeBounds},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use anyhow::{Result, ensure};
use fs2::FileExt;
use openraft::{
    BasicNode, Entry, EntryPayload, ErrorSubject, ErrorVerb, LogId, LogState, RaftLogReader,
    RaftSnapshotBuilder, Snapshot, SnapshotMeta, StorageError, StorageIOError, StoredMembership,
    Vote,
    storage::{LogFlushed, RaftLogStorage, RaftStateMachine},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::TypeConfig;
use converyn_core::orchestration::machine::{Reply, State};

pub const SCHEMA_VERSION: u32 = 23;

#[derive(Clone)]
pub struct Store {
    inner: Arc<Inner>,
}
struct Inner {
    directory: PathBuf,
    db: Mutex<Connection>,
    view: RwLock<Arc<Machine>>,
    _lock: File,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Machine {
    pub last_applied: Option<LogId<u64>>,
    pub membership: StoredMembership<u64, BasicNode>,
    pub state: State,
}

fn storage_error(error: impl std::fmt::Display) -> StorageError<u64> {
    StorageIOError::new(
        ErrorSubject::Store,
        ErrorVerb::Write,
        openraft::AnyError::new(&std::io::Error::other(error.to_string())),
    )
    .into()
}

fn read<T: DeserializeOwned>(db: &Connection, key: &str) -> Result<Option<T>> {
    let data: Option<Vec<u8>> = db
        .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    data.map(|b| serde_json::from_slice(&b).map_err(Into::into))
        .transpose()
}
fn write(db: &Connection, key: &str, value: &impl Serialize) -> Result<()> {
    db.execute("INSERT INTO metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, serde_json::to_vec(value)?])?;
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct Position {
    pub(crate) last_applied: Option<LogId<u64>>,
    pub(crate) membership: StoredMembership<u64, BasicNode>,
}
fn write_position(db: &Connection, machine: &Machine) -> Result<()> {
    write(
        db,
        "machine_position",
        &Position {
            last_applied: machine.last_applied,
            membership: machine.membership.clone(),
        },
    )
}
fn load_machine(db: &Connection) -> Result<Machine> {
    let position: Position = read(db, "machine_position")?
        .ok_or_else(|| anyhow::anyhow!("persisted applied position missing"))?;
    Ok(Machine {
        last_applied: position.last_applied,
        membership: position.membership,
        state: crate::storage::state_rows::load(db)?,
    })
}

impl Store {
    pub fn open(path: &Path, node_id: u64) -> Result<Self> {
        let existing = path.join("state.sqlite3");
        if existing.exists() {
            let old =
                Connection::open_with_flags(&existing, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            ensure!(
                read::<u32>(&old, "schema_version")? == Some(SCHEMA_VERSION),
                "unsupported storage schema; stop all controllers and run converyn admin migrate-state --data-dir PATH on each member"
            );
        }

        std::fs::create_dir_all(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.join("store.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|_| anyhow::anyhow!("data directory is already in use"))?;
        let mut db = Connection::open(path.join("state.sqlite3"))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL); CREATE TABLE IF NOT EXISTS logs(idx TEXT PRIMARY KEY,entry BLOB NOT NULL);")?;
        let version: Option<u32> = read(&db, "schema_version")?;
        ensure!(
            version.is_none_or(|value| value == SCHEMA_VERSION),
            "unsupported storage schema"
        );
        let stored_id: Option<u64> = read(&db, "node_id")?;
        ensure!(
            stored_id.is_none() || stored_id == Some(node_id),
            "data directory belongs to another node"
        );
        crate::storage::state_rows::create(&db)?;
        crate::storage::node_index::create(&db)?;
        crate::storage::history_store::create(&db)?;
        let machine = if version.is_some() {
            load_machine(&db)?
        } else {
            Machine::default()
        };
        {
            let tx = db.transaction()?;
            tx.execute("DELETE FROM node_work_index", [])?;
            crate::storage::node_index::update(&tx, &State::default(), &machine.state)?;
            if version.is_none() {
                write_position(&tx, &machine)?;
            }
            write(&tx, "schema_version", &SCHEMA_VERSION)?;
            write(&tx, "node_id", &node_id)?;
            tx.commit()?;
        }
        // Ensure a newly created database directory entry survives a host crash too.
        File::open(path)?.sync_all()?;
        Ok(Self {
            inner: Arc::new(Inner {
                directory: std::fs::canonicalize(path)?,
                db: Mutex::new(db),
                view: RwLock::new(Arc::new(machine)),
                _lock: lock,
            }),
        })
    }

    pub async fn restore_business_state(
        &self,
        state: State,
        backup_sha256: &str,
    ) -> Result<(), StorageError<u64>> {
        self.restore_state(state, backup_sha256, None).await
    }
    pub async fn restore_carrier_state(
        &self,
        state: State,
        backup_sha256: &str,
        source: &Path,
    ) -> Result<(), StorageError<u64>> {
        self.restore_state(state, backup_sha256, Some(source.to_path_buf()))
            .await
    }
    async fn restore_state(
        &self,
        state: State,
        backup_sha256: &str,
        source: Option<PathBuf>,
    ) -> Result<(), StorageError<u64>> {
        let inner = self.inner.clone();
        let backup_sha256 = backup_sha256.to_owned();
        self.io(move |db| {
            let old = load_machine(db)?;
            ensure!(
                old.last_applied.is_none() && old.state == State::default(),
                "restore destination is not empty"
            );
            let machine = Machine {
                state,
                ..Machine::default()
            };
            ensure!(
                source.is_some()
                    || (machine.state.history.records.is_empty()
                        && machine.state.cold_audit.is_empty()),
                "cold history requires a complete carrier backup"
            );
            if let Some(source) = &source {
                db.execute(
                    "ATTACH DATABASE ?1 AS incoming",
                    [source
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("invalid backup path"))?],
                )?;
            }
            let tx = db.transaction()?;
            crate::storage::state_rows::save(&tx, &State::default(), &machine.state)?;
            tx.execute("DELETE FROM node_work_index", [])?;
            crate::storage::node_index::update(&tx, &State::default(), &machine.state)?;
            write_position(&tx, &machine)?;
            write(&tx, "restored_from", &backup_sha256)?;
            if source.is_some() {
                tx.execute(
                    "INSERT INTO cold_history SELECT * FROM incoming.cold_history",
                    [],
                )?;
            }
            tx.commit()?;
            if source.is_some() {
                db.execute_batch("DETACH DATABASE incoming")?;
            }
            *inner
                .view
                .write()
                .map_err(|_| anyhow::anyhow!("poisoned committed view"))? = Arc::new(machine);
            Ok(())
        })
        .await
    }

    pub async fn history_value(
        &self,
        record: converyn_core::orchestration::history::Record,
    ) -> Result<serde_json::Value, StorageError<u64>> {
        self.io(move |db| crate::storage::history_store::load(db, &record))
            .await
    }
    pub async fn load_run(
        &self,
        uid: &str,
    ) -> Result<Option<Arc<converyn_core::orchestration::machine::Run>>, StorageError<u64>> {
        let view = self.machine().await?;
        if let Some(run) = view.state.runs.get(uid) {
            return Ok(Some(run.clone()));
        }
        let Some(record) = view
            .state
            .history
            .get(converyn_core::orchestration::history::Kind::Run, uid)
            .cloned()
        else {
            return Ok(None);
        };
        self.io(move |db| crate::storage::history_store::load(db, &record).map(Some))
            .await
    }
    /// 在当前副本视图上采集纯契约结果；只预演规则，不提交状态或外部副作用。
    /// 共识副本只重放这些固定结果，不能依赖当前安装的插件。
    pub async fn prepare_command(
        &self,
        command: converyn_core::orchestration::machine::Command,
    ) -> Result<converyn_core::orchestration::machine::Command, StorageError<u64>> {
        use converyn_core::orchestration::machine::Command;
        let mut inner = &command;
        while let Command::Authorized { command, .. } = inner {
            inner = command;
        }
        match inner {
            Command::Claim {
                run_id, work_id, ..
            } => {
                let view = self.machine().await?;
                let plugin_inputs = view
                    .state
                    .runs
                    .get(run_id)
                    .and_then(|r| r.effective_work(work_id))
                    .is_some_and(|w| {
                        !w.inputs.is_empty()
                            && matches!(
                                w.operation,
                                converyn_core::model::domain::Operation::Plugin { .. }
                            )
                    });
                if !plugin_inputs {
                    return Ok(command);
                }
            }
            Command::RetryInputs { .. } => return Ok(command),
            Command::ObserveNode { .. }
            | Command::Identity { .. }
            | Command::PutScript { .. }
            | Command::Cancel { .. }
            | Command::PauseRun { .. }
            | Command::Resolve { .. }
            | Command::MarkUncertain { .. }
            | Command::SetCertificateBinding { .. }
            | Command::History { .. }
            | Command::Controlplane { .. }
            | Command::Preparation { .. } => return Ok(command),
            Command::Report { receipt, .. }
                if receipt
                    .as_ref()
                    .and_then(|r| r.native.as_ref())
                    .and_then(|e| e.plugin.as_ref())
                    .is_none() =>
            {
                return Ok(command);
            }
            _ => {}
        }
        let state = self.machine().await?.state.clone();
        self.io(move |db| {
            let mut next = state;
            crate::storage::history_store::warm(db, &mut next, &command)?;
            let recorder =
                converyn_core::model::contracts::Recorder::new(crate::plugins::registry());
            next.apply_with(command.clone(), &recorder);
            let contracts = recorder.finish();
            contracts.validate_size()?;
            Ok(Command::Prepared {
                command: Box::new(command),
                contracts,
            })
        })
        .await
    }
    pub async fn preview(
        &self,
        command: converyn_core::orchestration::machine::Command,
    ) -> Result<(converyn_core::orchestration::machine::Reply, State), StorageError<u64>> {
        let state = self.machine().await?.state.clone();
        self.io(move |db| {
            let mut next = state;
            crate::storage::history_store::warm(db, &mut next, &command)?;
            let reply = next.apply_with(command, crate::plugins::registry());
            Ok((reply, next))
        })
        .await
    }
    pub async fn definition(&self, uid: &str) -> Result<Option<String>, StorageError<u64>> {
        let view = self.machine().await?;
        if let Some(content) = view.state.scripts.get(uid) {
            return Ok(Some(content.clone()));
        }
        let Some(record) = view
            .state
            .history
            .get(converyn_core::orchestration::history::Kind::Definition, uid)
            .cloned()
        else {
            return Ok(None);
        };
        self.io(move |db| crate::storage::history_store::load(db, &record).map(Some))
            .await
    }
    pub async fn node_runs(&self, node: String) -> Result<Vec<String>, StorageError<u64>> {
        self.io(move |db| {
            let mut q = db.prepare("SELECT run FROM node_work_index WHERE node=?1 ORDER BY run")?;
            let rows = q
                .query_map([node], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }
    pub async fn audit_page(
        &self,
        stream: String,
        after: u64,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StorageError<u64>> {
        self.io(move |db| crate::storage::audit_store::page(db, &stream, after, limit))
            .await
    }
    pub async fn audit_view_page(
        &self,
        stream: String,
        after: u64,
        limit: usize,
        view: String,
    ) -> Result<(Vec<serde_json::Value>, Option<u64>), StorageError<u64>> {
        self.io(move |db| {
            crate::storage::audit_store::filtered_page(db, &stream, after, limit, &view)
        })
        .await
    }
    pub async fn backup_carrier(&self) -> Result<(PathBuf, String), StorageError<u64>> {
        let directory = self.inner.directory.clone();
        tokio::task::spawn_blocking(move || -> Result<_> {
            let (path, _, sha) = crate::storage::carrier::build(
                &directory.join("state.sqlite3"),
                &directory.join("backups"),
                "backup",
            )?;
            crate::storage::carrier::append_footer(&path, &sha)?;
            Ok((path, sha))
        })
        .await
        .map_err(storage_error)?
        .map_err(storage_error)
    }

    async fn io<T: Send + 'static>(
        &self,
        op: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T, StorageError<u64>> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let mut db = inner
                .db
                .lock()
                .map_err(|_| anyhow::anyhow!("poisoned database lock"))?;
            op(&mut db)
        })
        .await
        .map_err(storage_error)?
        .map_err(storage_error)
    }

    pub async fn machine(&self) -> Result<Arc<Machine>, StorageError<u64>> {
        // Concurrent long polls share an immutable committed view rather than queue full JSON
        // reads behind Raft's disk writes. Callers still require a linearizable read barrier.
        self.inner
            .view
            .read()
            .map(|v| v.clone())
            .map_err(storage_error)
    }

    pub async fn bind_cluster(&self, cluster: &str) -> Result<(), StorageError<u64>> {
        let cluster = cluster.to_owned();
        self.io(move |db| {
            let previous: Option<String> = read(db, "cluster")?;
            ensure!(
                previous.is_none() || previous.as_ref() == Some(&cluster),
                "data belongs to another control plane"
            );
            write(db, "cluster", &cluster)
        })
        .await
    }
}

impl RaftLogReader<TypeConfig> for Store {
    async fn try_get_log_entries<R: RangeBounds<u64> + Clone + Debug + Send>(
        &mut self,
        range: R,
    ) -> Result<Vec<Entry<TypeConfig>>, StorageError<u64>> {
        let start = match range.start_bound() {
            Bound::Unbounded => None,
            Bound::Included(v) => Some(*v),
            Bound::Excluded(v) => match v.checked_add(1) {
                Some(v) => Some(v),
                None => return Ok(vec![]),
            },
        };
        let end = match range.end_bound() {
            Bound::Unbounded => None,
            Bound::Excluded(v) => Some(*v),
            Bound::Included(v) => v.checked_add(1),
        };
        self.io(move |db| {
            let mut q = db.prepare("SELECT entry FROM logs WHERE (?1 IS NULL OR idx>=?1) AND (?2 IS NULL OR idx<?2) ORDER BY idx")?;
            let rows = q.query_map(params![start.map(|n| format!("{n:016x}")), end.map(|n| format!("{n:016x}"))], |r| r.get::<_, Vec<u8>>(0))?;
            rows.map(|r| Ok(serde_json::from_slice(&r?)?)).collect()
        }).await
    }
}

impl RaftLogStorage<TypeConfig> for Store {
    type LogReader = Self;
    async fn get_log_reader(&mut self) -> Self {
        self.clone()
    }
    async fn get_log_state(&mut self) -> Result<LogState<TypeConfig>, StorageError<u64>> {
        self.io(|db| {
            let purged: Option<LogId<u64>> = read(db, "purged")?;
            let last: Option<Vec<u8>> = db
                .query_row(
                    "SELECT entry FROM logs ORDER BY idx DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let last = last
                .map(|b| serde_json::from_slice::<Entry<TypeConfig>>(&b))
                .transpose()?
                .map(|e| e.log_id);
            Ok(LogState {
                last_purged_log_id: purged,
                last_log_id: last.or(purged),
            })
        })
        .await
    }
    async fn save_vote(&mut self, vote: &Vote<u64>) -> Result<(), StorageError<u64>> {
        let vote = *vote;
        self.io(move |db| write(db, "vote", &vote)).await
    }
    async fn read_vote(&mut self) -> Result<Option<Vote<u64>>, StorageError<u64>> {
        self.io(|db| read(db, "vote")).await
    }
    async fn save_committed(
        &mut self,
        committed: Option<LogId<u64>>,
    ) -> Result<(), StorageError<u64>> {
        self.io(move |db| write(db, "committed", &committed)).await
    }
    async fn read_committed(&mut self) -> Result<Option<LogId<u64>>, StorageError<u64>> {
        self.io(|db| Ok(read(db, "committed")?.flatten())).await
    }
    async fn append<I>(
        &mut self,
        entries: I,
        callback: LogFlushed<TypeConfig>,
    ) -> Result<(), StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + Send,
        I::IntoIter: Send,
    {
        let entries: Vec<_> = entries.into_iter().collect();
        let result = self.io(move |db| {
            let tx = db.transaction()?;
            for e in entries {
                tx.execute("INSERT INTO logs(idx,entry) VALUES(?1,?2) ON CONFLICT(idx) DO UPDATE SET entry=excluded.entry", params![format!("{:016x}", e.log_id.index), serde_json::to_vec(&e)?])?;
            }
            tx.commit()?;
            Ok(())
        }).await;
        callback.log_io_completed(
            result
                .as_ref()
                .map(|_| ())
                .map_err(|e| std::io::Error::other(e.to_string())),
        );
        result
    }
    async fn truncate(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        self.io(move |db| {
            db.execute(
                "DELETE FROM logs WHERE idx>=?1",
                [format!("{:016x}", log_id.index)],
            )?;
            Ok(())
        })
        .await
    }
    async fn purge(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        self.io(move |db| {
            let tx = db.transaction()?;
            write(&tx, "purged", &log_id)?;
            tx.execute(
                "DELETE FROM logs WHERE idx<=?1",
                [format!("{:016x}", log_id.index)],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }
}

impl RaftSnapshotBuilder<TypeConfig> for Store {
    async fn build_snapshot(&mut self) -> Result<Snapshot<TypeConfig>, StorageError<u64>> {
        let directory = self.inner.directory.clone();
        let (path, machine, sha) = tokio::task::spawn_blocking(move || {
            crate::storage::carrier::build(
                &directory.join("state.sqlite3"),
                &directory.join("snapshots"),
                "snapshot",
            )
        })
        .await
        .map_err(storage_error)?
        .map_err(storage_error)?;
        let meta = SnapshotMeta {
            last_log_id: machine.last_applied,
            last_membership: machine.membership,
            snapshot_id: format!("sqlite-v1-{sha}"),
        };
        let saved = meta.clone();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let directory = self.inner.directory.clone();
        self.io(move |db| {
            let old = read::<SnapshotMeta<u64, BasicNode>>(db, "snapshot_meta")?;
            if old
                .as_ref()
                .is_none_or(|old| old.last_log_id <= saved.last_log_id)
            {
                let previous = read::<String>(db, "snapshot_path")?;
                let tx = db.transaction()?;
                write(&tx, "snapshot_meta", &saved)?;
                write(&tx, "snapshot_path", &name)?;
                tx.execute(
                    "DELETE FROM metadata WHERE key IN ('snapshot_data','snapshot')",
                    [],
                )?;
                tx.commit()?;
                if let Some(previous) = previous
                    && previous != name
                {
                    let _ = std::fs::remove_file(directory.join("snapshots").join(previous));
                }
            }
            Ok(())
        })
        .await?;
        Ok(Snapshot {
            meta,
            snapshot: Box::new(
                crate::storage::snapshot_file::SnapshotFile::open(&path)
                    .await
                    .map_err(storage_error)?,
            ),
        })
    }
}

impl RaftStateMachine<TypeConfig> for Store {
    type SnapshotBuilder = Self;
    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogId<u64>>, StoredMembership<u64, BasicNode>), StorageError<u64>> {
        let m = self.machine().await?;
        Ok((m.last_applied, m.membership.clone()))
    }
    async fn apply<I>(&mut self, entries: I) -> Result<Vec<Reply>, StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + Send,
        I::IntoIter: Send,
    {
        let entries: Vec<_> = entries.into_iter().collect();
        let inner = self.inner.clone();
        self.io(move |db| {
            let tx = db.transaction()?;
            let previous = inner
                .view
                .read()
                .map_err(|_| anyhow::anyhow!("poisoned committed view"))?
                .clone();
            let mut machine = (*previous).clone();
            let mut replies = Vec::new();
            for entry in entries {
                let reply = if machine
                    .last_applied
                    .is_some_and(|id| entry.log_id.index <= id.index)
                {
                    Reply {
                        ok: false,
                        code: "log_already_applied".into(),
                        run_id: None,
                        revision: None,
                    }
                } else {
                    machine.last_applied = Some(entry.log_id);
                    match entry.payload {
                        EntryPayload::Normal(command) => {
                            crate::storage::history_store::warm(&tx, &mut machine.state, &command)?;
                            let data =
                                crate::storage::history_store::prepare(&machine.state, &command)?;
                            let response = machine.state.apply(command.clone());
                            if response.ok {
                                crate::storage::history_store::persist(
                                    &tx,
                                    &machine.state,
                                    &command,
                                    data,
                                )?;
                            }
                            crate::storage::audit_store::record(
                                &tx,
                                &mut machine.state,
                                entry.log_id.index,
                                &command,
                                &response,
                            )?;
                            crate::storage::audit_store::compact(&tx, &mut machine.state)?;
                            response
                        }
                        EntryPayload::Membership(m) => {
                            machine.membership = StoredMembership::new(Some(entry.log_id), m);
                            Reply {
                                ok: true,
                                code: "membership".into(),
                                run_id: None,
                                revision: None,
                            }
                        }
                        EntryPayload::Blank => Reply {
                            ok: true,
                            code: "blank".into(),
                            run_id: None,
                            revision: None,
                        },
                    }
                };
                replies.push(reply);
            }
            crate::storage::node_index::update(&tx, &previous.state, &machine.state)?;
            // Applied position, business state and membership commit atomically.
            crate::storage::state_rows::save(&tx, &previous.state, &machine.state)?;
            write_position(&tx, &machine)?;
            tx.commit()?;
            // Publish only after durability, and before reporting apply completion to Raft.
            *inner
                .view
                .write()
                .map_err(|_| anyhow::anyhow!("poisoned committed view"))? = Arc::new(machine);
            Ok(replies)
        })
        .await
    }
    async fn get_snapshot_builder(&mut self) -> Self {
        self.clone()
    }
    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<crate::storage::snapshot_file::SnapshotFile>, StorageError<u64>> {
        let directory = self.inner.directory.join("snapshots");
        std::fs::create_dir_all(&directory).map_err(storage_error)?;
        let path = directory.join(format!(
            "receive-{}",
            crate::credentials::pki::random_token().map_err(storage_error)?
        ));
        Ok(Box::new(
            crate::storage::snapshot_file::SnapshotFile::create(&path)
                .await
                .map_err(storage_error)?,
        ))
    }
    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<u64, BasicNode>,
        mut snapshot: Box<crate::storage::snapshot_file::SnapshotFile>,
    ) -> Result<(), StorageError<u64>> {
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};
        let directory = self.inner.directory.join("snapshots");
        std::fs::create_dir_all(&directory).map_err(storage_error)?;
        let path = directory.join(format!(
            "installed-{}.snapshot",
            crate::credentials::pki::random_token().map_err(storage_error)?
        ));
        snapshot.file.flush().await.map_err(storage_error)?;
        snapshot
            .file
            .seek(std::io::SeekFrom::Start(0))
            .await
            .map_err(storage_error)?;
        let mut target = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .await
            .map_err(storage_error)?;
        tokio::io::copy(&mut snapshot.file, &mut target)
            .await
            .map_err(storage_error)?;
        target.sync_all().await.map_err(storage_error)?;
        drop(target);
        File::open(&directory)
            .and_then(|f| f.sync_all())
            .map_err(storage_error)?;
        let received = snapshot.path.clone();
        drop(snapshot);
        if received.parent() == Some(directory.as_path())
            && received
                .file_name()
                .is_some_and(|s| s.to_string_lossy().starts_with("receive-"))
        {
            let _ = std::fs::remove_file(received);
        }
        let meta = meta.clone();
        let inner = self.inner.clone();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.io(move |db| {
            ensure!(
                crate::storage::carrier::sqlite(&path)?,
                "snapshot must use a SQLite carrier"
            );
            ensure!(
                meta.snapshot_id
                    == format!(
                        "sqlite-v1-{}",
                        crate::artifacts::artifact::hash_file(&path)?.0
                    ),
                "snapshot checksum mismatch"
            );
            let (machine, _, kind) = crate::storage::carrier::validate(&path)?;
            ensure!(kind == "snapshot", "snapshot carrier kind mismatch");
            ensure!(
                machine.last_applied == meta.last_log_id
                    && machine.membership == meta.last_membership,
                "snapshot metadata mismatch"
            );
            let previous = read::<String>(db, "snapshot_path")?;
            db.execute(
                "ATTACH DATABASE ?1 AS incoming",
                [path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid snapshot path"))?],
            )?;
            let copied = (|| -> Result<()> {
                let tx = db.transaction()?;
                tx.execute("DELETE FROM state_objects", [])?;
                tx.execute("DELETE FROM cold_history", [])?;
                crate::storage::state_rows::save(&tx, &State::default(), &machine.state)?;
                tx.execute("DELETE FROM node_work_index", [])?;
                crate::storage::node_index::update(&tx, &State::default(), &machine.state)?;
                tx.execute(
                    "INSERT INTO cold_history SELECT * FROM incoming.cold_history",
                    [],
                )?;
                write_position(&tx, &machine)?;
                write(&tx, "snapshot_meta", &meta)?;
                write(&tx, "snapshot_path", &name)?;
                tx.execute(
                    "DELETE FROM metadata WHERE key IN ('snapshot_data','snapshot')",
                    [],
                )?;
                tx.commit()?;
                Ok(())
            })();
            db.execute_batch("DETACH DATABASE incoming")?;
            copied?;
            File::open(inner.directory.join("snapshots"))?.sync_all()?;
            *inner
                .view
                .write()
                .map_err(|_| anyhow::anyhow!("poisoned committed view"))? = Arc::new(machine);
            if let Some(previous) = previous
                && previous != name
            {
                let _ = std::fs::remove_file(inner.directory.join("snapshots").join(previous));
            }
            Ok(())
        })
        .await
    }
    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<TypeConfig>>, StorageError<u64>> {
        let directory = self.inner.directory.clone();
        let result = self
            .io(move |db| {
                let Some(meta) = read::<SnapshotMeta<u64, BasicNode>>(db, "snapshot_meta")? else {
                    return Ok(None);
                };
                let name = read::<String>(db, "snapshot_path")?
                    .ok_or_else(|| anyhow::anyhow!("snapshot carrier path missing"))?;
                ensure!(
                    Path::new(&name).file_name() == Some(std::ffi::OsStr::new(&name)),
                    "invalid snapshot filename"
                );
                Ok(Some((meta, directory.join("snapshots").join(name))))
            })
            .await?;
        match result {
            Some((meta, path)) => Ok(Some(Snapshot {
                meta,
                snapshot: Box::new(
                    crate::storage::snapshot_file::SnapshotFile::open(&path)
                        .await
                        .map_err(storage_error)?,
                ),
            })),
            None => Ok(None),
        }
    }
}
