//! 确定性状态转换；时间、身份及纯插件契约由调用者显式传入。
use super::*;
use crate::domain::{digest, valid_id};
use std::sync::Arc;
impl State {
    /// Deterministic transition function. No clocks, randomness, network or host side effects here.
    pub fn apply(&mut self, command: Command) -> Reply {
        self.apply_with(command, &converyn_plugin_api::NoPlugins)
    }
    /// 所有副本使用已核对摘要的纯契约；此接口不持有运行时能力。
    pub fn apply_with(
        &mut self,
        command: Command,
        contracts: &dyn converyn_plugin_api::Contracts,
    ) -> Reply {
        if let Command::RetryInputs {
            run_id,
            expected_revision,
        } = &command
        {
            let Some(saved) = self.runs.get(run_id) else {
                return Reply::error("run_not_found");
            };
            if saved.revision != *expected_revision {
                return Reply::error("revision_conflict");
            }
            if !saved.can_retry_inputs() {
                return Reply::error("input_retry_requires_unstarted_work");
            }
            if let Err(code) = self.validate_host_scope(&saved.plan) {
                return Reply::error(code);
            }
            if self.runs.values().any(|other| {
                other.sequence > saved.sequence
                    && other.plan.input.owner_uid == saved.plan.input.owner_uid
                    && other.attempts.values().any(|a| a.grant.is_some())
            }) {
                return Reply::error("newer_run_executed");
            }
            let run = Arc::make_mut(self.runs.get_mut(run_id).unwrap());
            for a in run.attempts.values_mut() {
                if a.status != Status::Succeeded {
                    *Arc::make_mut(a) = Attempt::default();
                }
            }
            run.preparing = true;
            run.pause_requested = false;
            run.finished_ms = None;
            run.status = Status::Preparing;
            run.revision += 1;
            return Reply::run(run);
        }
        if let Command::Materialize {
            run_id,
            work_id,
            expected_revision,
            now_ms,
        } = &command
        {
            let Some(saved) = self.runs.get(run_id) else {
                return Reply::error("run_not_found");
            };
            if saved.revision != *expected_revision {
                return Reply::error("revision_conflict");
            }
            if saved
                .attempts
                .get(work_id)
                .is_some_and(|a| a.materialized.is_some())
            {
                return Reply::run(saved);
            }
            if !saved.materializable().contains(work_id) {
                return Reply::error("inputs_not_ready");
            }
            let resolved = saved.materialize(work_id, *now_ms, contracts).and_then(
                |(materialized, definitions)| {
                    let extra: usize = definitions
                        .iter()
                        .filter(|(sha, _)| !self.scripts.contains_key(*sha))
                        .map(|(_, v)| v.len())
                        .sum();
                    anyhow::ensure!(
                        self.scripts.values().map(String::len).sum::<usize>() + extra
                            <= 4 * 1024 * 1024
                            && self.scripts.len() + definitions.len() <= 65536,
                        "materialized_definition_quota"
                    );
                    Ok((materialized, definitions))
                },
            );
            let run = Arc::make_mut(self.runs.get_mut(run_id).unwrap());
            let attempt = Arc::make_mut(run.attempts.get_mut(work_id).unwrap());
            match resolved {
                Ok((materialized, definitions)) => {
                    for (sha, content) in definitions {
                        self.scripts.insert(sha, content);
                    }
                    attempt.materialized = Some(materialized);
                }
                Err(error) => {
                    attempt.status = Status::Failed;
                    attempt.reason = Some(
                        format!("input_materialization:{}", error)
                            .chars()
                            .take(256)
                            .collect(),
                    );
                }
            }
            run.revision += 1;
            progress::refresh_unlock(run, &mut self.execution_locks);
            return Reply::run(run);
        }
        if let Command::Workflow { change } = &command {
            let mut next = self.clone();
            return match next.workflow_change(change.clone(), contracts) {
                Ok(reply) => {
                    *self = next;
                    reply
                }
                Err(error) => Reply::error(&error.to_string()),
            };
        }
        if let Command::Access { change } = &command {
            return match self.personnel.access_change(change.clone()) {
                Ok(()) => Reply {
                    ok: true,
                    code: "access_updated".into(),
                    run_id: None,
                    revision: Some(self.personnel.access.revision),
                },
                Err(e) => Reply::error(&e.to_string()),
            };
        }
        if let Command::Personnel { change } = &command {
            let mut next = self.personnel.clone();
            return match next.change(change.clone()) {
                Ok(()) => {
                    self.personnel = next;
                    Reply {
                        ok: true,
                        code: "identity_updated".into(),
                        run_id: None,
                        revision: None,
                    }
                }
                Err(error) => Reply::error(&error.to_string()),
            };
        }
        if let Command::Prepared { command, contracts } = &command {
            if contracts.validate_size().is_err() {
                return Reply::error("invalid_contract_journal");
            }
            return self.apply_with((**command).clone(), contracts);
        }
        if let Command::Authorized {
            authorization,
            command,
        } = &command
        {
            if !self.access_allows(authorization, command) {
                return Reply::error("access_denied_before_commit");
            }
            if self
                .identities
                .bindings
                .get(&authorization.principal)
                .is_some_and(|binding| !binding.fingerprints.contains(&authorization.fingerprint))
            {
                return Reply::error("credential_revoked_before_commit");
            }
            let sequence = self.run_sequence;
            let prior_status = match command.unwrapped() {
                Command::RetryInputs { run_id, .. }
                | Command::Materialize { run_id, .. }
                | Command::Claim { run_id, .. }
                | Command::Report { run_id, .. }
                | Command::Cancel { run_id }
                | Command::PauseRun { run_id, .. }
                | Command::Resolve { run_id, .. }
                | Command::MarkUncertain { run_id, .. } => self.runs.get(run_id).map(|r| r.status),
                _ => None,
            };
            let result = self.apply_with((**command).clone(), contracts);
            // Persist the replicated request time only on creation or a new terminal
            // transition. Retries must not rewrite dates or invent old completion times.
            if result.ok
                && let Some(now) = authorization.observed_ms
                && let Some(id) = &result.run_id
                && let Some(run) = self.runs.get_mut(id)
            {
                let created = run.sequence > sequence;
                let finished = prior_status.is_some_and(|s| !s.terminal()) && run.status.terminal();
                if let Command::Report {
                    work_id, status, ..
                } = command.unwrapped()
                    && status.terminal()
                {
                    let run = Arc::make_mut(run);
                    if let Some(attempt) = run.attempts.get_mut(work_id) {
                        let attempt = Arc::make_mut(attempt);
                        if attempt.completed_ms.is_none() {
                            attempt.completed_ms = Some(now);
                        }
                    }
                }
                if created || finished {
                    let run = Arc::make_mut(run);
                    if created {
                        run.submitted_ms = Some(now);
                    }
                    if finished && run.finished_ms.is_none() {
                        run.finished_ms = Some(now);
                    }
                }
            }
            if result.ok {
                let control = match command.unwrapped() {
                    Command::Cancel { run_id } => Some((run_id, true)),
                    Command::PauseRun { run_id, paused, .. } => Some((run_id, *paused)),
                    _ => None,
                };
                if let Some((id, stop)) = control
                    && let Some(uid) = self
                        .workflows
                        .executions
                        .get(id)
                        .and_then(|e| e.deployment_uid.as_ref())
                        .cloned()
                    && let Some(policy) = self.drift.get_mut(&uid)
                {
                    policy.interval_seconds = if stop {
                        None
                    } else {
                        self.workflows
                            .executions
                            .get(id)
                            .and_then(|e| {
                                self.workflows.revisions.get(&e.request.workflow_revision)
                            })
                            .and_then(|r| r.reconcile_seconds)
                    };
                }
            }
            return result;
        }
        match &command {
            Command::Claim {
                node_uid,
                proof: Some(proof),
                ..
            }
            | Command::Report {
                node_uid,
                proof: Some(proof),
                ..
            } if !self.identity_current(node_uid, proof, false) => {
                return Reply::error("node_identity_fenced");
            }
            Command::ObserveNode {
                node_uid,
                proof: Some(proof),
                hardware_sha256,
                session_id,
                ..
            } if !self.identity_current(node_uid, proof, true)
                || proof.hardware_sha256 != *hardware_sha256
                || proof.session.as_ref().is_some_and(|s| s != session_id) =>
            {
                return Reply::error("node_identity_fenced");
            }
            _ => {}
        }
        if let Command::Controlplane { change } = &command {
            return match self.controlplane_change(change.clone()) {
                Ok(()) => Reply {
                    ok: true,
                    code: "member_operation_updated".into(),
                    run_id: None,
                    revision: None,
                },
                Err(code) => Reply::error(code),
            };
        }
        if let Command::Preparation { change } = &command {
            return match self.prepare(change.clone()) {
                Ok(()) => Reply {
                    ok: true,
                    code: "preparation_updated".into(),
                    run_id: None,
                    revision: None,
                },
                Err(code) => Reply::error(code),
            };
        }
        if let Command::History { change } = &command {
            return match self.history_change(change.clone()) {
                Ok(()) => Reply {
                    ok: true,
                    code: "history_updated".into(),
                    run_id: None,
                    revision: None,
                },
                Err(code) => Reply::error(code),
            };
        }
        if let Command::Identity { change } = &command {
            return match self.register(change.clone()) {
                Ok(()) => Reply {
                    ok: true,
                    code: "identity_updated".into(),
                    run_id: None,
                    revision: None,
                },
                Err(code) => Reply::error(code),
            };
        }
        if let Command::ConfigureDrift {
            uid,
            interval_seconds,
        } = &command
        {
            return match self.configure_drift(uid, *interval_seconds) {
                Ok(()) => Reply {
                    ok: true,
                    code: "drift_policy_updated".into(),
                    run_id: None,
                    revision: None,
                },
                Err(code) => Reply::error(code),
            };
        }
        if let Command::ReconcileDrift {
            uid,
            now_ms,
            expected_generation,
        } = &command
        {
            return match self.reconcile_drift_with(uid, *now_ms, *expected_generation, contracts) {
                Ok(()) => Reply {
                    ok: true,
                    code: "drift_reconciled".into(),
                    run_id: None,
                    revision: None,
                },
                Err(code) => Reply::error(code),
            };
        }
        if let Command::Administer { change } = &command {
            if let crate::administration::Change::Target { target } = change
                && contracts
                    .validate_target(&target.plugin, &target.spec)
                    .is_err()
            {
                return Reply::error("invalid_plugin_target");
            }
            return match self.administer(change.clone()) {
                Ok(()) => Reply {
                    ok: true,
                    code: "cluster_management_updated".into(),
                    run_id: None,
                    revision: None,
                },
                Err(code) => Reply::error(code),
            };
        }
        if let Command::SetCertificateBinding { change } = &command {
            return match self.identities.change(change.clone()) {
                Ok(revision) => Reply {
                    ok: true,
                    code: "certificate_binding_updated".into(),
                    run_id: None,
                    revision: Some(revision),
                },
                Err(code) => Reply::error(code),
            };
        }
        if let Command::ApplyDeployment { uid, plan } = &command {
            if !valid_id(uid)
                || plan.input.owner_uid != *uid
                || plan.verify_with(contracts).is_err()
            {
                return Reply::error("invalid_deployment");
            }
            if let Some(d) = self.deployments.get_mut(uid) {
                if d.plan.input.project != plan.input.project {
                    return Reply::error("deployment_project_immutable");
                }
                if d.plan.input.location_uid() != plan.input.location_uid()
                    || d.plan.input.target != plan.input.target
                {
                    return Reply::error("deployment_target_immutable");
                }
                if d.plan.sha256 != plan.sha256 {
                    d.generation += 1;
                    d.plan = Arc::new((**plan).clone());
                    d.condition = "pending".into();
                } else if d.active_run.is_none() && d.last_started_generation < d.generation {
                    // Explicitly resubmitting a rejected, never-started intent may
                    // retry admission. Failed scripts are never retried this way.
                    d.condition = "pending".into();
                }
            } else {
                if self.deployments.len() >= 1000 {
                    return Reply::error("deployment_quota");
                }
                self.deployments.insert(
                    uid.clone(),
                    Deployment {
                        uid: uid.clone(),
                        generation: 1,
                        applied_generation: None,
                        last_started_generation: 0,
                        plan: Arc::new((**plan).clone()),
                        active_run: None,
                        last_run: None,
                        condition: "pending".into(),
                    },
                );
            }
            return Reply {
                ok: true,
                code: "desired_state_saved".into(),
                run_id: None,
                revision: Some(self.deployments[uid].generation),
            };
        }
        if let Command::ReconcileDeployment { uid } = &command {
            let Some(mut d) = self.deployments.get(uid).cloned() else {
                return Reply::error("deployment_not_found");
            };
            if let Some(id) = &d.active_run {
                let Some(run) = self.runs.get(id) else {
                    return Reply::error("active_run_missing");
                };
                if run.status == Status::NeedsAttention {
                    d.condition = "needs_attention".into();
                } else if run.status.terminal() {
                    if run.status == Status::Succeeded {
                        d.applied_generation = Some(d.last_started_generation);
                        d.condition = "rollout_succeeded".into();
                    } else {
                        d.condition = if run.status == Status::Failed {
                            "failed"
                        } else {
                            "canceled"
                        }
                        .into();
                    }
                    d.active_run = None;
                }
            }
            if d.active_run.is_none() && d.last_started_generation < d.generation {
                let id = format!("deployment-{}-g{}", &digest(uid)[..24], d.generation);
                let reply = self.apply_with(
                    Command::CreateRun {
                        output_key: None,
                        run_id: id.clone(),
                        idempotency_key: id.clone(),
                        plan: Box::new((*d.plan).clone()),
                    },
                    contracts,
                );
                if reply.ok {
                    d.last_started_generation = d.generation;
                    d.applied_generation = None;
                    d.active_run = Some(id.clone());
                    d.last_run = Some(id);
                    d.condition = "progressing".into();
                } else {
                    d.condition = reply.code;
                }
            }
            let reply = Reply {
                ok: true,
                code: d.condition.clone(),
                run_id: d.last_run.clone(),
                revision: Some(d.generation),
            };
            self.deployments.insert(uid.clone(), d);
            return reply;
        }

        if let Command::ObserveNode {
            diagnostic,
            inventory,
            resources,
            inventory_checked_ms,
            capabilities,
            hardware_sha256,
            node_uid,
            boot_id,
            session_id,
            observed_ms,
            ..
        } = &command
        {
            if inventory
                .as_ref()
                .is_some_and(|v| !v.valid(crate::model::telemetry::INVENTORY_LIMIT))
                || resources
                    .as_ref()
                    .is_some_and(|v| !v.valid(crate::model::telemetry::RESOURCE_LIMIT))
            {
                return Reply::error("observation_size_limit");
            }
            if diagnostic.as_ref().is_some_and(|d| d.len() > 2048) {
                return Reply::error("observation_size_limit");
            }
            if capabilities.len() > 64
                || capabilities.iter().any(|c| {
                    !valid_id(&c.id)
                        || !valid_id(&c.version)
                        || !crate::registration::valid_sha(&c.sha256)
                        || c.contract != 1
                })
            {
                return Reply::error("invalid_capability_manifest");
            }
            if ![node_uid, boot_id, session_id].iter().all(|v| valid_id(v)) {
                return Reply::error("invalid_observation");
            }
            if !self.observations.contains_key(node_uid) && self.observations.len() >= 3000 {
                return Reply::error("observation_quota");
            }
            if hardware_sha256
                .as_ref()
                .is_some_and(|h| !crate::registration::valid_sha(h))
            {
                return Reply::error("invalid_hardware_identity");
            }
            if self
                .registry
                .nodes
                .get(node_uid)
                .is_some_and(|n| Some(&n.hardware_sha256) != hardware_sha256.as_ref() || n.retired)
            {
                return Reply::error("node_hardware_mismatch");
            }
            if let Some(old) = self.observations.get(node_uid)
                && old.hardware_sha256.is_some()
            {
                if old.hardware_sha256 != *hardware_sha256 {
                    return Reply::error("node_hardware_mismatch");
                }
                if old.session_id != *session_id
                    && old.boot_id == *boot_id
                    && *observed_ms < old.observed_ms.saturating_add(30000)
                {
                    return Reply::error("node_session_still_active");
                }
            }
            let old = self.observations.get(node_uid);
            let inventory = inventory
                .clone()
                .or_else(|| old.and_then(|o| o.inventory.clone()));
            let resources = resources
                .clone()
                .or_else(|| old.and_then(|o| o.resources.clone()));
            let inventory_checked_ms =
                (*inventory_checked_ms).or_else(|| old.and_then(|o| o.inventory_checked_ms));
            self.observations.insert(
                node_uid.clone(),
                NodeObservation {
                    diagnostic: diagnostic.clone(),
                    inventory,
                    resources,
                    inventory_checked_ms,
                    capabilities: capabilities.clone(),
                    hardware_sha256: hardware_sha256.clone(),
                    boot_id: boot_id.clone(),
                    session_id: session_id.clone(),
                    observed_ms: *observed_ms,
                },
            );
            return Reply {
                ok: true,
                code: "observed".into(),
                run_id: None,
                revision: None,
            };
        }

        if let Command::PutScript { sha256, content } = &command {
            use sha2::{Digest, Sha256};
            if content.len() > 65536 || hex::encode(Sha256::digest(content.as_bytes())) != *sha256 {
                return Reply::error("invalid_script");
            }
            if let Some(previous) = self.scripts.get(sha256) {
                return Reply {
                    ok: previous == content,
                    code: "script_exists".into(),
                    run_id: None,
                    revision: None,
                };
            }
            if self
                .history
                .get(crate::history::Kind::Definition, sha256)
                .is_some()
            {
                return Reply {
                    ok: true,
                    code: "script_exists".into(),
                    run_id: None,
                    revision: None,
                };
            }
            if self.scripts.len() >= 65536 {
                return Reply::error("definition_count_quota");
            }
            if self.scripts.values().map(String::len).sum::<usize>() + content.len()
                > 4 * 1024 * 1024
            {
                return Reply::error("script_definition_quota");
            }
            self.scripts.insert(sha256.clone(), content.clone());
            return Reply {
                ok: true,
                code: "script_stored".into(),
                run_id: None,
                revision: None,
            };
        }

        if let Command::CreateRun {
            output_key,
            run_id,
            idempotency_key,
            plan,
        } = command
        {
            if self
                .retirement_proofs
                .contains_key(&format!("run/{run_id}"))
            {
                return Reply::error("run_history_cleared");
            }
            if !valid_id(&run_id)
                || idempotency_key.is_empty()
                || idempotency_key.len() > 256
                || plan.verify_with(contracts).is_err()
            {
                return Reply::error("invalid_plan_or_identity");
            }
            if plan
                .work
                .iter()
                .any(|w| w.outputs.values().any(|s| s.secret))
                && output_key.as_ref().is_none_or(|key| {
                    !crate::domain::valid_sha(&key.id) || key.public_key.len() != 44
                })
            {
                return Reply::error("output_encryption_key_required");
            }
            let key = digest(&("create_run", &plan.input.owner_uid, &idempotency_key));
            if let Some((fingerprint, previous)) = self.idempotency.get(&key) {
                return if fingerprint == &plan.sha256 {
                    if let Some(run) = self.runs.get(previous) {
                        Reply::run(run)
                    } else if let Some(record) =
                        self.history.get(crate::history::Kind::Run, previous)
                    {
                        Reply {
                            ok: true,
                            code: "archived".into(),
                            run_id: Some(previous.clone()),
                            revision: Some(record.revision),
                        }
                    } else {
                        Reply::error("idempotency_history_missing")
                    }
                } else {
                    Reply::error("idempotency_conflict")
                };
            }
            if self.runs.contains_key(&run_id)
                || self
                    .history
                    .get(crate::history::Kind::Run, &run_id)
                    .is_some()
            {
                return Reply::error("run_exists");
            }
            if let Some(target) = &plan.input.target
                && self.targets.get(&target.uid) != Some(target)
            {
                return Reply::error("external_target_snapshot_conflict");
            }
            if let Err(code) = self.execution_ready(&plan) {
                return Reply::error(code);
            }
            if plan
                .work
                .iter()
                .any(|w| self.administration.maintenance.contains(&w.node_uid))
            {
                return Reply::error("node_in_maintenance");
            }
            if plan
                .lock_keys()
                .iter()
                .any(|key| self.execution_locks.contains_key(key))
            {
                return Reply::error("node_busy");
            }
            if self.workflow_nodes_busy(&plan, None) {
                return Reply::error("node_busy");
            }
            let previous_project = self
                .owner_projects
                .get(&plan.input.owner_uid)
                .cloned()
                .or_else(|| {
                    self.runs
                        .values()
                        .find(|r| r.plan.input.owner_uid == plan.input.owner_uid)
                        .map(|r| r.plan.input.project.clone())
                });
            if previous_project
                .as_ref()
                .is_some_and(|project| project != &plan.input.project)
            {
                return Reply::error("owner_project_conflict");
            }
            for work in &plan.work {
                for resource in work.operation.resource_keys(&work.node_uid) {
                    if self
                        .resource_owners
                        .get(&resource)
                        .is_some_and(|o| o != &plan.input.owner_uid)
                    {
                        return Reply::error("resource_owner_conflict");
                    }
                }
            }
            let Some(sequence) = self.run_sequence.checked_add(1) else {
                return Reply::error("run_sequence_exhausted");
            };
            let preparing = plan.work.iter().any(|w| w.operation.artifact().is_some());
            self.run_sequence = sequence;
            for work in &plan.work {
                for resource in work.operation.resource_keys(&work.node_uid) {
                    if !self.resource_owners.contains_key(&resource) {
                        let version = self
                            .administration
                            .owner_revisions
                            .entry(resource.clone())
                            .or_default();
                        *version = version.saturating_add(1);
                    }
                    self.resource_owners
                        .insert(resource, plan.input.owner_uid.clone());
                }
            }
            self.owner_projects
                .insert(plan.input.owner_uid.clone(), plan.input.project.clone());
            if !preparing {
                for key in plan.lock_keys() {
                    self.execution_locks.insert(key, run_id.clone());
                }
            }
            self.idempotency
                .insert(key, (plan.sha256.clone(), run_id.clone()));
            let attempts = plan
                .work
                .iter()
                .map(|w| (w.id.clone(), Arc::new(Attempt::default())))
                .collect();
            let artifact_nodes = Some(
                plan.work
                    .iter()
                    .filter(|w| w.operation.artifact().is_some())
                    .map(|w| w.node_uid.clone())
                    .collect(),
            );
            let run = Run {
                output_key,
                submitted_ms: None,
                finished_ms: None,
                artifact_nodes,
                preparing,
                prepared: Default::default(),
                preparation_errors: Default::default(),
                sequence,
                pause_requested: false,
                id: run_id.clone(),
                plan: Arc::from(plan),
                status: if preparing {
                    Status::Preparing
                } else {
                    Status::Pending
                },
                cancel_requested: false,
                revision: 1,
                attempts,
            };
            let reply = Reply::run(&run);
            self.runs.insert(run_id, Arc::new(run));
            return reply;
        }
        let run_id = match &command {
            Command::PauseRun { run_id, .. }
            | Command::Claim { run_id, .. }
            | Command::Report { run_id, .. }
            | Command::Cancel { run_id }
            | Command::MarkUncertain { run_id, .. }
            | Command::Resolve { run_id, .. } => run_id.clone(),
            Command::RetryInputs { .. }
            | Command::Materialize { .. }
            | Command::Workflow { .. }
            | Command::Access { .. }
            | Command::Personnel { .. }
            | Command::Prepared { .. }
            | Command::Authorized { .. }
            | Command::Controlplane { .. }
            | Command::Preparation { .. }
            | Command::History { .. }
            | Command::Identity { .. }
            | Command::ConfigureDrift { .. }
            | Command::ReconcileDrift { .. }
            | Command::Administer { .. }
            | Command::SetCertificateBinding { .. }
            | Command::CreateRun { .. }
            | Command::PutScript { .. }
            | Command::ObserveNode { .. }
            | Command::ApplyDeployment { .. }
            | Command::ReconcileDeployment { .. } => {
                unreachable!()
            }
        };
        if matches!(
            &command,
            Command::Claim { .. }
                | Command::PauseRun { paused: false, .. }
                | Command::Resolve {
                    status: Status::Pending,
                    ..
                }
        ) && let Some(run) = self.runs.get(&run_id)
            && let Err(code) = self.validate_host_scope(&run.plan)
        {
            return Reply::error(code);
        }
        let Some(run) = self.runs.get_mut(&run_id) else {
            return Reply::error("run_not_found");
        };
        let run = Arc::make_mut(run);
        let changed = match command {
            Command::PauseRun {
                paused,
                expected_revision,
                ..
            } => {
                if run.pause_requested == paused {
                    return Reply::run(run);
                }
                if run.revision != expected_revision {
                    return Reply::error("revision_conflict");
                }
                if run.status.terminal() || run.cancel_requested {
                    return Reply::error("run_not_pausable");
                }
                run.pause_requested = paused;
                true
            }
            Command::Claim {
                input_sha256,
                proof,
                authorized_ms,
                work_id,
                node_uid,
                grant,
                boot_id,
                session_id,
                ..
            } => {
                if !valid_id(&grant) || !valid_id(&boot_id) || !valid_id(&session_id) {
                    return Reply::error("invalid_claim");
                }
                let Some(work) = run.effective_work(&work_id).cloned() else {
                    return Reply::error("work_not_found");
                };
                if work.node_uid != node_uid {
                    return Reply::error("wrong_node");
                }
                if !work.inputs.is_empty() && work.input_sha256 != input_sha256 {
                    return Reply::error("input_digest_mismatch");
                }
                let attempt = &run.attempts[&work_id];
                if attempt.grant.as_ref() == Some(&grant)
                    && attempt.boot_id.as_ref() == Some(&boot_id)
                    && attempt.session_id.as_ref() == Some(&session_id)
                {
                    // A retry returns the existing grant, never authorizes another execution.
                    return Reply::run(run);
                }
                if proof.is_some()
                    && self.observations.get(&node_uid).is_some_and(|o| {
                        o.hardware_sha256.is_some()
                            && (o.boot_id != boot_id || o.session_id != session_id)
                    })
                {
                    return Reply::error("stale_agent_session");
                }
                if run.preparing
                    || attempt.status != Status::Pending
                    || run.cancel_requested
                    || run.pause_requested
                    || run.status.terminal()
                    || run.status == Status::NeedsAttention
                {
                    return Reply::error("not_claimable");
                }
                if !work.inputs.is_empty()
                    && let Some(now) = authorized_ms
                    && let Err(error) = run.materialize(&work_id, now, contracts)
                {
                    let attempt = Arc::make_mut(run.attempts.get_mut(&work_id).unwrap());
                    attempt.status = Status::Failed;
                    attempt.reason = Some(format!("input_materialization:{error}"));
                    run.revision += 1;
                    progress::refresh_unlock(run, &mut self.execution_locks);
                    return Reply::run(run);
                }
                if self
                    .administration
                    .window_until(&node_uid, authorized_ms)
                    .is_some()
                {
                    return Reply::error("node_in_maintenance_window");
                }
                if run
                    .plan
                    .work
                    .iter()
                    .any(|w| w.wave < work.wave && run.attempts[&w.id].status != Status::Succeeded)
                {
                    return Reply::error("dependency_not_ready");
                }
                if run.ordered_predecessor_pending(&work) {
                    return Reply::error("step_order_not_ready");
                }
                if let Some(limit) = run
                    .plan
                    .input
                    .playbook
                    .steps
                    .iter()
                    .find(|s| s.id == work.step_id)
                    .and_then(|s| s.max_parallel)
                {
                    let active = run
                        .plan
                        .work
                        .iter()
                        .filter(|w| {
                            w.step_id == work.step_id
                                && w.wave == work.wave
                                && matches!(
                                    run.attempts[&w.id].status,
                                    Status::Authorized
                                        | Status::Running
                                        | Status::CancelRequested
                                        | Status::NeedsAttention
                                )
                        })
                        .count();
                    if active >= limit {
                        return Reply::error("step_parallel_limit");
                    }
                }
                if run.plan.work.iter().any(|w| {
                    w.node_uid == node_uid
                        && w.id != work_id
                        && matches!(
                            run.attempts[&w.id].status,
                            Status::Authorized
                                | Status::Running
                                | Status::CancelRequested
                                | Status::NeedsAttention
                        )
                }) {
                    return Reply::error("node_busy");
                }
                if work.operation.location() == converyn_plugin_api::Location::Controller
                    && proof.is_some()
                {
                    return Reply::error("controller_work_rejects_node_proof");
                }
                let a = Arc::make_mut(run.attempts.get_mut(&work_id).unwrap());
                if let crate::domain::Operation::Plugin { call } = &work.operation
                    && call.location == converyn_plugin_api::Location::Node
                    && !self
                        .observations
                        .get(&node_uid)
                        .is_some_and(|o| o.capabilities.contains(&call.plugin))
                {
                    return Reply::error("node_plugin_capability_mismatch");
                }
                a.status = Status::Authorized;
                a.authorized_ms = authorized_ms;
                a.grant = Some(grant);
                a.boot_id = Some(boot_id);
                a.session_id = Some(session_id);
                true
            }
            Command::Report {
                proof,
                work_id,
                node_uid,
                grant,
                sequence,
                status,
                receipt,
                ..
            } => {
                let Some(work) = run.effective_work(&work_id).cloned() else {
                    return Reply::error("work_not_found");
                };
                if work.node_uid != node_uid {
                    return Reply::error("wrong_node");
                }
                if work.operation.location() == converyn_plugin_api::Location::Controller
                    && proof.is_some()
                {
                    return Reply::error("controller_work_rejects_node_proof");
                }
                let a = Arc::make_mut(run.attempts.get_mut(&work_id).unwrap());
                if a.grant.as_ref() != Some(&grant) {
                    return Reply::error("wrong_grant");
                }
                if status == Status::Succeeded && !work.outputs.is_empty() && receipt.is_none() {
                    return Reply::error("successful_output_producer_requires_receipt");
                }
                if let Some(r) = &receipt {
                    use crate::runner::Outcome;
                    let binding = match &work.operation {
                        crate::domain::Operation::Exec {
                            script_sha256,
                            timeout_seconds,
                            ..
                        } => {
                            r.native.is_none()
                                && a.boot_id.as_ref() == Some(&r.boot_id)
                                && *script_sha256 == r.script_sha256
                                && u64::from(*timeout_seconds) == r.timeout_seconds
                                && (status != Status::Succeeded || r.exit_code == Some(0))
                        }
                        crate::domain::Operation::Plugin { .. }
                        | crate::domain::Operation::File { .. }
                        | crate::domain::Operation::Service { .. }
                        | crate::domain::Operation::Binary { .. }
                        | crate::domain::Operation::Package { .. }
                        | crate::domain::Operation::Reboot { .. }
                        | crate::domain::Operation::HttpCheck { .. } => {
                            r.logs.is_none()
                                && r.execution.is_none()
                                && r.script_sha256.is_empty()
                                && r.timeout_seconds == 0
                                && r.exit_code.is_none()
                                && r.native.as_ref().is_some_and(|e| {
                                    e.operation == work.operation
                                        && e.plugin.as_ref().is_none_or(|p| {
                                            matches!(&work.operation,crate::domain::Operation::Plugin{call} if contracts.verify_observation(call,p).unwrap_or(false)) && p.owner == run.plan.input.owner_uid
                                                && p.fence == run.sequence
                                        })
                                        && e.reboot.as_ref().is_none_or(|reboot| {
                                            Some(&reboot.before_boot) == a.boot_id.as_ref()
                                                && (status != Status::Succeeded
                                                    || reboot.after_boot.as_ref()
                                                        == Some(&r.boot_id))
                                        })
                                        && (status != Status::Succeeded || e.verified())
                                })
                        }
                    };
                    let outcome = matches!(
                        (status, &r.outcome),
                        (Status::Succeeded, Outcome::Succeeded)
                            | (Status::Failed, Outcome::Failed)
                            | (Status::Canceled, Outcome::Canceled)
                            | (
                                Status::NeedsAttention,
                                Outcome::NeedsAttention | Outcome::TimedOut
                            )
                    );
                    if crate::model::outputs::validate_stored(
                        &work,
                        &r.outputs,
                        run.output_key.as_ref(),
                        status == Status::Succeeded,
                    )
                    .is_err()
                        || (crate::model::outputs::no_log(&work) && r.logs.is_some())
                        || serde_json::to_vec(r)
                            .map_or(true, |b| b.len() > crate::model::outputs::MAX_RECEIPT_BYTES)
                        || r.logs.as_ref().is_some_and(|logs| !logs.valid())
                        || !binding
                        || !outcome
                        || r.operation_id != format!("{}/{}", run.id, work.id)
                        || !valid_id(&r.boot_id)
                    {
                        return Reply::error("invalid_execution_receipt");
                    }
                }
                let hash = if receipt.is_some() {
                    digest(&(&node_uid, &grant, sequence, status, &receipt))
                } else {
                    digest(&(&node_uid, &grant, sequence, status))
                };
                if let Some(old) = a.reports.get(&sequence) {
                    return if old == &hash {
                        Reply::run(run)
                    } else {
                        Reply::error("event_conflict")
                    };
                }
                if sequence != a.last_sequence + 1 {
                    return Reply::error("event_out_of_order");
                }
                let allowed = match a.status {
                    Status::Authorized => matches!(
                        status,
                        Status::Running
                            | Status::Failed
                            | Status::Canceled
                            | Status::NeedsAttention
                    ),
                    Status::Running => matches!(
                        status,
                        Status::Succeeded | Status::Failed | Status::NeedsAttention
                    ),
                    Status::CancelRequested => matches!(
                        status,
                        Status::Canceled
                            | Status::Succeeded
                            | Status::Failed
                            | Status::NeedsAttention
                    ),
                    Status::NeedsAttention => matches!(
                        status,
                        Status::Succeeded
                            | Status::Failed
                            | Status::Canceled
                            | Status::NeedsAttention
                    ),
                    _ => false,
                };
                if !allowed {
                    return Reply::error("invalid_transition");
                }
                a.reason = if status == Status::NeedsAttention {
                    receipt
                        .as_ref()
                        .and_then(|r| r.diagnostic.clone())
                        .or_else(|| {
                            receipt
                                .as_ref()
                                .and_then(|r| r.native.as_ref())
                                .and_then(|e| e.error.clone())
                        })
                        .or(Some("execution_result_unconfirmed".into()))
                } else {
                    None
                };
                if receipt.is_some() {
                    a.receipt = receipt;
                }
                a.status = status;
                a.last_sequence = sequence;
                a.reports.insert(sequence, hash);
                true
            }
            Command::Cancel { .. } => {
                if run.status.terminal() || run.cancel_requested {
                    return Reply::run(run);
                }
                run.cancel_requested = true;
                true
            }
            Command::MarkUncertain {
                reason,
                work_id,
                expected_revision,
                ..
            } => {
                if run.revision != expected_revision {
                    return Reply::error("revision_conflict");
                }
                let Some(a) = run.attempts.get_mut(&work_id) else {
                    return Reply::error("work_not_found");
                };
                let a = Arc::make_mut(a);
                if !matches!(
                    a.status,
                    Status::Authorized | Status::Running | Status::CancelRequested
                ) {
                    return Reply::error("invalid_transition");
                }
                a.status = Status::NeedsAttention;
                a.reason = reason;
                true
            }
            Command::Resolve {
                work_id,
                expected_revision,
                status,
                actor,
                evidence,
                ..
            } => {
                if run.revision != expected_revision {
                    return Reply::error("revision_conflict");
                }
                if !status.terminal()
                    || !valid_id(&actor)
                    || evidence.trim().is_empty()
                    || evidence.len() > 4096
                {
                    return Reply::error("invalid_resolution");
                }
                let Some(a) = run.attempts.get_mut(&work_id) else {
                    return Reply::error("work_not_found");
                };
                let a = Arc::make_mut(a);
                if a.status != Status::NeedsAttention {
                    return Reply::error("invalid_transition");
                }
                if status == Status::Succeeded {
                    let work = run.plan.work.iter().find(|w| w.id == work_id).unwrap();
                    if !work.outputs.is_empty()
                        && a.receipt.as_ref().is_none_or(|r| {
                            crate::model::outputs::validate_stored(
                                work,
                                &r.outputs,
                                run.output_key.as_ref(),
                                true,
                            )
                            .is_err()
                        })
                    {
                        return Reply::error("successful_resolution_requires_original_outputs");
                    }
                }
                a.status = status;
                self.audit.push(Resolution {
                    run_id: run_id.clone(),
                    work_id,
                    actor,
                    evidence,
                    status,
                });
                true
            }
            Command::RetryInputs { .. }
            | Command::Materialize { .. }
            | Command::Workflow { .. }
            | Command::Access { .. }
            | Command::Personnel { .. }
            | Command::Prepared { .. }
            | Command::Authorized { .. }
            | Command::Controlplane { .. }
            | Command::Preparation { .. }
            | Command::History { .. }
            | Command::Identity { .. }
            | Command::ConfigureDrift { .. }
            | Command::ReconcileDrift { .. }
            | Command::Administer { .. }
            | Command::SetCertificateBinding { .. }
            | Command::CreateRun { .. }
            | Command::PutScript { .. }
            | Command::ObserveNode { .. }
            | Command::ApplyDeployment { .. }
            | Command::ReconcileDeployment { .. } => unreachable!(),
        };
        if changed {
            run.revision += 1;
            progress::refresh_unlock(run, &mut self.execution_locks);
        }
        Reply::run(run)
    }
}
