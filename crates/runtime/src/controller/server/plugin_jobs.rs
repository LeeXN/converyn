//! 控制端工作使用已提交的 grant 和 fence；外部 I/O 永不在状态机内执行。
use super::*;

/// 控制端插件共享核心的授权和回执。新 leader 只重入声明为可收敛的调用。
pub(super) async fn controller_plugins(a: App, directory: std::path::PathBuf) {
    use converyn_core::orchestration::machine::Status;
    let inflight = Arc::new(std::sync::Mutex::new(
        std::collections::BTreeSet::<String>::new(),
    ));
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if a.raft.metrics().borrow().current_leader != Some(a.id) {
            continue;
        }
        let Ok(m) = a.store.machine().await else {
            continue;
        };
        for run in m.state.runs.values().filter(|r| {
            !r.status.terminal()
                && !r.preparing
                && !r.pause_requested
                && r.status != Status::NeedsAttention
        }) {
            for work in run
                .plan
                .work
                .iter()
                .filter_map(|w| run.effective_work(&w.id))
            {
                let converyn_core::model::domain::Operation::Plugin { call } = &work.operation
                else {
                    continue;
                };
                if call.location != converyn_plugin_api::Location::Controller {
                    continue;
                }
                let attempt = &run.attempts[&work.id];
                if attempt.status.terminal() || attempt.status == Status::NeedsAttention {
                    continue;
                }
                if !run
                    .plan
                    .work
                    .iter()
                    .filter(|w| w.wave < work.wave)
                    .all(|w| run.attempts[&w.id].status == Status::Succeeded)
                {
                    continue;
                }
                // 名额覆盖实际外部操作的整个生命周期，不能在 HTTP 等待者超时后提前释放。
                let Ok(permit) = a.plugin_admission.clone().try_acquire_owned() else {
                    continue;
                };
                let key = format!("{}/{}", run.id, work.id);
                if !inflight.lock().unwrap().insert(key.clone()) {
                    continue;
                }
                let app = a.clone();
                let run = run.clone();
                let work = work.clone();
                let jobs = inflight.clone();
                let directory = directory.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    if let Err(error) = controller_work(&app, &directory, &run, &work).await {
                        tracing::warn!(run_id=%run.id,work_id=%work.id,error=%error,"controller plugin interrupted");
                        if app.raft.metrics().borrow().current_leader == Some(app.id)
                            && let Ok(view) = app.store.machine().await
                            && let Some(current) = view.state.runs.get(&run.id)
                            && matches!(
                                current.attempts[&work.id].status,
                                Status::Authorized | Status::Running | Status::CancelRequested
                            )
                        {
                            let _ = command(
                                State(app.clone()),
                                Json(Command::MarkUncertain {
                                    run_id: run.id.clone(),
                                    work_id: work.id.clone(),
                                    expected_revision: current.revision,
                                    reason: Some(error.to_string().chars().take(2048).collect()),
                                }),
                            )
                            .await;
                        }
                    }
                    jobs.lock().unwrap().remove(&key);
                });
            }
        }
    }
}

pub(super) async fn controller_work(
    a: &App,
    directory: &Path,
    run: &converyn_core::orchestration::machine::Run,
    work: &converyn_core::model::domain::Work,
) -> Result<()> {
    use converyn_core::orchestration::machine::{Command, Status};
    use converyn_plugin_api as api;
    let converyn_core::model::domain::Operation::Plugin { call } = &work.operation else {
        anyhow::bail!("not a plugin")
    };
    crate::plugins::validate_plan(&run.plan)?;
    let attempt = &run.attempts[&work.id];
    let cached_path = directory.join("plugin-output-receipts").join(format!(
        "{}.json",
        converyn_core::model::domain::digest(&format!("{}/{}", run.id, work.id))
    ));
    if !work.outputs.is_empty()
        && cached_path.exists()
        && matches!(attempt.status, Status::Authorized | Status::Running)
    {
        use std::io::Read;
        let receipt: crate::executors::runner::Receipt = serde_json::from_reader(
            std::fs::File::open(&cached_path)?
                .take((converyn_core::model::outputs::MAX_RECEIPT_BYTES + 1) as u64),
        )?;
        ensure!(
            receipt.operation_id == format!("{}/{}", run.id, work.id),
            "cached output receipt differs"
        );
        let response = command(
            State(a.clone()),
            Json(Command::Report {
                proof: None,
                run_id: run.id.clone(),
                work_id: work.id.clone(),
                node_uid: work.node_uid.clone(),
                grant: attempt.grant.clone().context("grant missing")?,
                sequence: 2,
                status: match receipt.outcome {
                    crate::executors::runner::Outcome::Succeeded => Status::Succeeded,
                    crate::executors::runner::Outcome::Failed => Status::Failed,
                    _ => Status::NeedsAttention,
                },
                receipt: Some(receipt),
            }),
        )
        .await;
        ensure!(
            response.status() == StatusCode::OK,
            "cached output report unavailable"
        );
        return Ok(());
    }
    if matches!(attempt.status, Status::Authorized | Status::Running) && !call.reconcilable {
        let _ = command(
            State(a.clone()),
            Json(Command::MarkUncertain {
                run_id: run.id.clone(),
                work_id: work.id.clone(),
                expected_revision: run.revision,
                reason: Some("controller transaction lost its terminal receipt".into()),
            }),
        )
        .await;
        return Ok(());
    }
    let target = run
        .plan
        .input
        .target
        .as_ref()
        .context("external target missing")?;
    let local = a
        .plugin_connections
        .get(&target.connection)
        .context("local plugin connection missing")?;
    ensure!(
        local.identity == call.plugin
            && local.projects.contains(&target.project)
            && local.targets.contains(&target.uid),
        "local plugin connection grant mismatch"
    );
    crate::plugins::registry().get(&call.plugin.id)?.authorize(
        call,
        &local.policy,
        &target.project,
    )?;
    let grant = attempt.grant.clone().unwrap_or_else(|| {
        converyn_core::model::domain::digest(&(run.id.as_str(), work.id.as_str(), run.sequence))
    });
    ensure!(confirm_read(a).await, "controller no longer holds quorum");
    if attempt.status == Status::Pending {
        if run.cancel_requested {
            return Ok(());
        }
        let response = command(
            State(a.clone()),
            Json(Command::Claim {
                input_sha256: work.input_sha256.clone(),
                proof: None,
                authorized_ms: Some(crate::executors::host::now_ms()),
                run_id: run.id.clone(),
                work_id: work.id.clone(),
                node_uid: work.node_uid.clone(),
                grant: grant.clone(),
                boot_id: "controller".into(),
                session_id: "controller".into(),
            }),
        )
        .await;
        ensure!(
            response.status() == StatusCode::OK,
            "controller grant was not committed"
        );
    }
    let current = a.store.machine().await?;
    let active = current.state.runs.get(&run.id).context("run missing")?;
    ensure!(
        !active.cancel_requested,
        "cancellation requested before controller operation"
    );
    let started = &active.attempts[&work.id];
    if started.status != Status::Running {
        let response = command(
            State(a.clone()),
            Json(Command::Report {
                proof: None,
                run_id: run.id.clone(),
                work_id: work.id.clone(),
                node_uid: work.node_uid.clone(),
                grant: grant.clone(),
                sequence: started.last_sequence + 1,
                status: Status::Running,
                receipt: None,
            }),
        )
        .await;
        ensure!(
            response.status() == StatusCode::OK,
            "controller start was not committed"
        );
    }
    let cancellation = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel = cancellation.clone();
    let call = call.clone();
    let operation = work.operation.clone();
    let owner = run.plan.input.owner_uid.clone();
    let target = target.clone();
    let policy = local.policy.clone();
    let state_dir = directory
        .join("plugin-work")
        .join(converyn_core::model::domain::digest(&(
            run.id.as_str(),
            work.id.as_str(),
        )));
    let fence = run.sequence;
    let worker =
        tokio::task::spawn_blocking(move || -> Result<crate::executors::native::Evidence> {
            std::fs::create_dir_all(&state_dir)?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&state_dir, std::fs::Permissions::from_mode(0o700))?;
            let native = crate::executors::native::Policy {
                runtime_project: Some(target.project.clone()),
                runtime_owner: Some(owner.clone()),
                runtime_state_dir: Some(state_dir.clone()),
                ..Default::default()
            };
            let host = crate::plugins::Services {
                policy: &native,
                call: &call,
                cancel: &cancel,
            };
            let context = api::Context {
                project: &target.project,
                owner: &owner,
                state_dir: &state_dir,
                policy: &policy,
                target: Some(&target.spec),
                fence,
                cancel: &cancel,
                host: &host,
            };
            let plugin = crate::plugins::registry().get(&call.plugin.id)?;
            let observed = if call.reconcilable {
                plugin.observe(&call, &context).ok().filter(|payload| {
                    plugin
                        .verify_bound(&call, payload, &owner, fence)
                        .unwrap_or(false)
                })
            } else {
                None
            };
            let result = if let Some(payload) = observed {
                api::Applied {
                    changed: false,
                    payload,
                }
            } else {
                plugin.apply(&call, &context)?
            };
            let verified = plugin.verify_bound(&call, &result.payload, &owner, fence)?;
            ensure!(verified, "plugin postcondition failed");
            let mut evidence = crate::executors::native::Evidence::empty(&operation);
            evidence.changed = result.changed;
            evidence.plugin = Some(api::Observation {
                owner: owner.clone(),
                fence,
                plugin: call.plugin.clone(),
                call_sha256: api::digest(&call),
                payload: result.payload,
                verified,
            });
            Ok(evidence)
        });
    tokio::pin!(worker);
    let began = std::time::Instant::now();
    let result = loop {
        tokio::select! {r=&mut worker=>break r?,_=tokio::time::sleep(Duration::from_millis(500))=>{
            let current=a.store.machine().await?;
            let canceled=current.state.runs.get(&run.id).is_none_or(|r|r.cancel_requested||r.status.terminal());
            if canceled||a.raft.metrics().borrow().current_leader!=Some(a.id)||began.elapsed().as_secs()>u64::from(match &work.operation{converyn_core::model::domain::Operation::Plugin{call}=>call.timeout_seconds,_=>1}){cancellation.store(true,Ordering::SeqCst);}
        }}
    };
    // 失去 leader 后不补写结果；接管者按已提交的原始 grant 和 fence 核对。
    ensure!(
        a.raft.metrics().borrow().current_leader == Some(a.id) && confirm_read(a).await,
        "leadership changed during plugin execution"
    );
    let m = a.store.machine().await?;
    let active = m.state.runs.get(&run.id).context("run missing")?;
    let at = &active.attempts[&work.id];
    if at.status.terminal() {
        return Ok(());
    }
    let (status, evidence) = match result {
        Ok(e) => (Status::Succeeded, e),
        Err(error) => {
            let mut e = crate::executors::native::Evidence::empty(&work.operation);
            e.error = Some(error.to_string());
            (Status::NeedsAttention, e)
        }
    };
    let receipt = crate::executors::runner::Receipt {
        outputs: Default::default(),
        diagnostic: None,
        execution: None,
        logs: None,
        native: Some(Box::new(evidence)),
        operation_id: format!("{}/{}", run.id, work.id),
        script_sha256: String::new(),
        timeout_seconds: 0,
        outcome: if status == Status::Succeeded {
            crate::executors::runner::Outcome::Succeeded
        } else {
            crate::executors::runner::Outcome::NeedsAttention
        },
        exit_code: None,
        boot_id: "controller".into(),
    };
    let output_context = crate::executors::outputs::Context {
        run_id: run.id.clone(),
        work_id: work.id.clone(),
        specifications: work.outputs.clone(),
        key: run.output_key.clone(),
    };
    let receipt = crate::executors::outputs::native_receipt(
        &directory.join("plugin-output-receipts"),
        &output_context,
        receipt,
    )?;
    // Evidence can be valid while its declared result selector is missing.
    // Commit that contract failure instead of reporting a mismatched success.
    let status = match receipt.outcome {
        crate::executors::runner::Outcome::Succeeded => Status::Succeeded,
        crate::executors::runner::Outcome::Failed => Status::Failed,
        _ => status,
    };
    let response = command(
        State(a.clone()),
        Json(Command::Report {
            proof: None,
            run_id: run.id.clone(),
            work_id: work.id.clone(),
            node_uid: work.node_uid.clone(),
            grant,
            sequence: at.last_sequence + 1,
            status,
            receipt: Some(receipt),
        }),
    )
    .await;
    ensure!(
        response.status() == StatusCode::OK,
        "controller receipt was not committed"
    );
    Ok(())
}

/// 入场检查不读取外部目标；实际身份仍由插件在每次执行前通过 TLS API 核对。
pub(super) fn validate_admission(
    a: &App,
    state: &converyn_core::orchestration::machine::State,
    plan: &converyn_core::model::domain::Plan,
) -> Result<()> {
    state.execution_ready(plan).map_err(anyhow::Error::msg)?;
    for work in &plan.work {
        if let Some(artifact) = work.operation.artifact()
            && artifact.locations.is_empty()
            && state.administration.artifacts.get(&artifact.sha256) != Some(&artifact.size)
        {
            anyhow::bail!("managed artifact {} is not uploaded", artifact.sha256);
        }
    }
    for work in &plan.work {
        {
            let environment = crate::credentials::secrets::environment(&work.operation)?;
            for value in environment.values() {
                if let converyn_core::model::variables::Value::Secret(reference) = value {
                    let sealed = state
                        .administration
                        .secrets
                        .get(&reference.secret)
                        .and_then(|v| v.get(&reference.version))
                        .with_context(|| {
                            format!(
                                "Secret {}/{} is missing; register it once before running",
                                reference.secret, reference.version
                            )
                        })?;
                    let keys = crate::credentials::outputs::Keyring::load(
                        a.output_keyring
                            .as_ref()
                            .context("secret encryption key unavailable")?,
                    )?;
                    crate::credentials::secrets::open(
                        &keys,
                        &reference.secret,
                        &reference.version,
                        sealed,
                    )?;
                }
            }
        }
    }
    crate::plugins::validate_plan(plan)?;
    for work in &plan.work {
        if let converyn_core::model::domain::Operation::Plugin { call } = &work.operation {
            match call.location {
                converyn_plugin_api::Location::Node => {
                    if let Some(observed) = state.observations.get(&work.node_uid) {
                        ensure!(
                            observed.capabilities.contains(&call.plugin),
                            "Node {} has not advertised the required plugin",
                            work.node_uid
                        );
                    }
                }
                converyn_plugin_api::Location::Controller => {
                    let target = plan
                        .input
                        .target
                        .as_ref()
                        .context("external target missing")?;
                    let grant = a
                        .plugin_connections
                        .get(&target.connection)
                        .context("local plugin connection missing")?;
                    ensure!(
                        grant.identity == call.plugin
                            && grant.projects.contains(&target.project)
                            && grant.targets.contains(&target.uid),
                        "local plugin connection scope differs from target"
                    );
                    crate::plugins::registry().get(&call.plugin.id)?.authorize(
                        call,
                        &grant.policy,
                        &target.project,
                    )?;
                }
            }
        }
    }
    Ok(())
}
