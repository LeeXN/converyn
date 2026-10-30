mod support;
use converyn_core::{
    model::domain::digest,
    orchestration::{
        machine::{Command, State},
        workflow::{self, Change, Revision},
    },
};
use serde_json::json;
use std::collections::BTreeMap;

fn state() -> State {
    crate::support::state()
}

fn workflow() -> Revision {
    serde_json::from_value(json!({"uid":"health","name":"Health check","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"playbook":{"api_version":"converyn/v1","name":"health","steps":[{"id":"check","phase":"verify","scope":"per_node","operation":{"kind":"http_check","url":"https://example.invalid/","expected_status":200,"timeout_seconds":5}}]},"parameters":{"STATUS":{"kind":"integer","default":200,"minimum":200,"maximum":299,"bindings":[{"step_id":"check","path":"/expected_status"}]}},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}})).unwrap()
}

#[test]
fn workflow_values_are_node_specific_immutable_and_frozen_per_execution() {
    use converyn_core::model::domain::Operation;
    use converyn_core::orchestration::values::{JSON_ENV, render};
    use sha2::{Digest, Sha256};
    let mut s = state();
    let script = "printf '%s:%s' \"$APP_VERSION\" \"$PORT\"\n";
    let hash = hex::encode(Sha256::digest(script));
    let source: Revision = serde_json::from_value(json!({"uid":"values","name":"Values","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"values":{"APP_VERSION":"one","PORT":80,"image":{"tag":"v1","repository":"fixed"}},"immutable":["image.repository"],"node_values":{"second":{"PORT":81}},"definitions":{&hash:script},"playbook":{"api_version":"converyn/v1","name":"values","steps":[{"id":"exec","phase":"rollout","scope":"per_node","operation":{"kind":"exec","script_sha256":hash,"timeout_seconds":30}}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}})).unwrap();
    let mut source = source;
    source.inventory.as_mut().unwrap().nodes.push(
        serde_json::from_value(json!({"uid":"second","hostname":"second","address":"192.0.2.2"}))
            .unwrap(),
    );
    let revision = register(&mut s, source.clone());
    let overrides = BTreeMap::from([
        ("APP_VERSION".into(), json!("two'$(touch /tmp/never)")),
        ("NEW_VARIABLE".into(), json!(true)),
        ("image".into(), json!({"tag":"v2"})),
    ]);
    let (plan, parameters, target_sha256) = s
        .workflow_plan(
            "values",
            &revision,
            "qa",
            &overrides,
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    plan.verify().unwrap();
    assert_eq!(
        parameters["image"],
        json!({"tag":"v2","repository":"fixed"})
    );
    for work in &plan.work {
        let Operation::Exec { environment, .. } = &work.operation else {
            panic!()
        };
        let values: serde_json::Value = serde_json::from_str(match &environment[JSON_ENV] {
            converyn_core::model::variables::Value::Literal(s) => s,
            _ => panic!(),
        })
        .unwrap();
        assert_eq!(
            values["PORT"],
            if work.node_uid == "second" { 81 } else { 80 }
        );
        assert_eq!(values["NEW_VARIABLE"], true);
        assert!(render(script, environment).contains("'\\''$(touch /tmp/never)"));
    }
    for bad in [
        json!({"image":{"repository":"changed"}}),
        json!({"image":"replaced"}),
        json!({"PATH":"/evil"}),
        json!({"CONVERYN_RUN_ID":"fake"}),
        json!({"KUBECONFIG":"/evil"}),
    ] {
        assert!(
            s.workflow_plan(
                "values",
                &revision,
                "qa",
                &serde_json::from_value(bad).unwrap(),
                &converyn_plugin_api::NoPlugins
            )
            .is_err()
        );
    }
    let result = execute(
        &mut s,
        workflow::Request {
            workflow_uid: "values".into(),
            workflow_revision: revision.clone(),
            target: "qa".into(),
            parameters: overrides,
            request_id: "values".into(),
            plan_sha256: plan.sha256,
            target_sha256,
        },
    );
    assert!(result.ok, "{}", result.code);
    assert_eq!(*s.workflows.revisions[&revision], source);
    assert_eq!(
        s.workflows.executions["run-values"].parameters["NEW_VARIABLE"],
        true
    );
    let (next, _, _) = s
        .workflow_plan(
            "values",
            &revision,
            "qa",
            &BTreeMap::new(),
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&next.work[0].operation).unwrap()["environment"]["APP_VERSION"],
        "one"
    );
}

#[test]
fn workflow_values_cannot_shadow_secret_references() {
    use sha2::{Digest, Sha256};
    let mut s = state();
    let script = "echo check\n";
    let hash = hex::encode(Sha256::digest(script));
    let mut source:Revision=serde_json::from_value(json!({"uid":"secrets","name":"Secrets","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"values":{},"definitions":{&hash:script},"playbook":{"api_version":"converyn/v1","name":"secrets","steps":[{"id":"exec","phase":"rollout","scope":"per_node","operation":{"kind":"exec","script_sha256":hash,"timeout_seconds":30}}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}})).unwrap();
    source.fixed.insert(
        "PASSWORD".into(),
        converyn_core::model::variables::Value::Secret(
            converyn_core::model::variables::Reference {
                secret: "password".into(),
                version: "v1".into(),
            },
        ),
    );
    let revision = register(&mut s, source);
    assert!(
        s.workflow_plan(
            "secrets",
            &revision,
            "qa",
            &BTreeMap::from([("PASSWORD".into(), json!("public"))]),
            &converyn_plugin_api::NoPlugins
        )
        .is_err()
    );
}

#[test]
fn package_freezes_yaml_or_json_defaults_and_rejects_escape_paths() {
    use converyn_runtime::orchestration::workflows::package;
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("defaults.yaml"),
        "APP_VERSION: one\nreplicas: 2\n",
    )
    .unwrap();
    let mut manifest = serde_json::to_value(workflow()).unwrap();
    manifest["values"] = json!("defaults.yaml");
    let file = d.path().join("workflow.yaml");
    std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(package(&file).unwrap().values["replicas"], 2);
    manifest["values"] = json!("../defaults.yaml");
    std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(package(&file).is_err());
}
fn register(state: &mut State, r: Revision) -> String {
    let sha = digest(&r);
    let result = state.apply(Command::Workflow {
        change: Change::Register {
            workflow: Box::new(r),
            expected_revision: 0,
        },
    });
    assert!(result.ok, "{}", result.code);
    sha
}
fn request(state: &State, sha: &str, id: &str) -> workflow::Request {
    let (plan, _, target) = state
        .workflow_plan(
            "health",
            sha,
            "qa",
            &BTreeMap::new(),
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    workflow::Request {
        workflow_uid: "health".into(),
        workflow_revision: sha.into(),
        target: "qa".into(),
        parameters: BTreeMap::new(),
        request_id: id.into(),
        plan_sha256: plan.sha256,
        target_sha256: target,
    }
}
fn execute(
    state: &mut State,
    r: workflow::Request,
) -> converyn_core::orchestration::machine::Reply {
    state.apply(Command::Workflow {
        change: Change::Execute {
            output_key: None,
            request: Box::new(r),
            actor: workflow::Actor {
                uid: "user-alice".into(),
                name: "Alice".into(),
                fingerprint: "a".repeat(64),
                credential_uid: "credential-alice".into(),
            },
            now_ms: 1000,
        },
    })
}
#[test]
fn server_compiles_typed_overrides_and_rejects_invalid_values() {
    let mut state = state();
    let sha = register(&mut state, workflow());
    let values = BTreeMap::from([("STATUS".into(), json!(204))]);
    let (plan, parameters, _) = state
        .workflow_plan(
            "health",
            &sha,
            "qa",
            &values,
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    assert_eq!(parameters["STATUS"], 204);
    assert_eq!(
        serde_json::to_value(&plan.work[0].operation).unwrap()["expected_status"],
        204
    );
    for values in [
        BTreeMap::from([("STATUS".into(), json!(500))]),
        BTreeMap::from([("STATUS".into(), json!("204"))]),
        BTreeMap::from([("PATH".into(), json!("other"))]),
    ] {
        assert!(
            state
                .workflow_plan(
                    "health",
                    &sha,
                    "qa",
                    &values,
                    &converyn_plugin_api::NoPlugins
                )
                .is_err()
        );
    }
    assert!(
        state
            .workflow_plan(
                "health",
                &sha,
                "arbitrary",
                &BTreeMap::new(),
                &converyn_plugin_api::NoPlugins
            )
            .is_err()
    );
}
#[test]
fn immutable_runs_idempotence_and_stale_preview() {
    let mut state = state();
    let sha = register(&mut state, workflow());
    let r = request(&state, &sha, "first");
    let reply = execute(&mut state, r.clone());
    assert!(reply.ok, "{}", reply.code);
    assert!(execute(&mut state, r.clone()).ok);
    assert_eq!(state.runs.len(), 1);
    let mut changed = workflow();
    changed.name = "New title".into();
    let update = state.apply(Command::Workflow {
        change: Change::Register {
            workflow: Box::new(changed),
            expected_revision: 1,
        },
    });
    assert!(update.ok, "{}", update.code);
    assert!(execute(&mut state, r.clone()).ok);
    let mut stale = r;
    stale.request_id = "second".into();
    assert!(!execute(&mut state, stale).ok);
    assert_eq!(
        state.workflows.executions["run-first"]
            .request
            .workflow_revision,
        sha
    );
}
#[test]
fn protected_bindings_and_failed_registration_leave_state_unchanged() {
    let mut state = state();
    let original = state.clone();
    let mut r = workflow();
    r.parameters.get_mut("STATUS").unwrap().bindings[0].path = "/script_sha256".into();
    assert!(
        !state
            .apply(Command::Workflow {
                change: Change::Register {
                    workflow: Box::new(r),
                    expected_revision: 0
                }
            })
            .ok
    );
    assert_eq!(state, original);
    let mut r = workflow();
    r.targets.get_mut("qa").unwrap().node_group_uid = Some("missing".into());
    assert!(
        !state
            .apply(Command::Workflow {
                change: Change::Register {
                    workflow: Box::new(r),
                    expected_revision: 0
                }
            })
            .ok
    );
    assert_eq!(state, original);
}
#[test]
fn script_binding_beats_node_and_fixed_defaults_without_editing_source() {
    use sha2::{Digest, Sha256};
    let mut state = state();
    let script = "printf '%s' \"$VERSION\"\n";
    let sha = hex::encode(Sha256::digest(script));
    let r:Revision=serde_json::from_value(json!({"uid":"script","name":"Script","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"definitions":{&sha:script},"playbook":{"api_version":"converyn/v1","name":"script","steps":[{"id":"exec","phase":"rollout","scope":"per_node","operation":{"kind":"exec","script_sha256":sha,"timeout_seconds":30}}]},"parameters":{"VERSION":{"kind":"string","default":"one","bindings":[{"step_id":"exec","path":"/environment/VERSION"}]}},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}})).unwrap();
    let revision = register(&mut state, r);
    let (plan, _, _) = state
        .workflow_plan(
            "script",
            &revision,
            "qa",
            &BTreeMap::from([("VERSION".into(), json!("two"))]),
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&plan.work[0].operation).unwrap()["environment"]["VERSION"],
        "two"
    );
    assert_eq!(
        state.workflows.revisions[&revision].parameters["VERSION"].default,
        Some(json!("one"))
    );
}

#[test]
fn configuration_overrides_are_content_addressed_run_snapshots() {
    use sha2::{Digest, Sha256};
    let sha = |s: &str| hex::encode(Sha256::digest(s.as_bytes()));
    let mut state = state();
    let original = "{\"replicas\":1,\"note\":\"\"}\n";
    let script = "printf '%s  %s\\n' \"$CONFIG_SHA256\" /opt/app/values.json | sha256sum -c -\n";
    let source: Revision = serde_json::from_value(json!({"uid":"config","name":"Config","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"batch_size":1,"definitions":{sha(original):original,sha(script):script},"playbook":{"api_version":"converyn/v1","name":"config","steps":[{"id":"file","phase":"rollout","scope":"per_node","operation":{"kind":"file","path":"/opt/app/values.json","sha256":sha(original),"mode":420}},{"id":"apply","phase":"rollout","scope":"per_node","operation":{"kind":"exec","script_sha256":sha(script),"timeout_seconds":30,"environment":{"CONFIG_SHA256":sha(original)}}}]},"parameters":{"VALUES":{"kind":"string","max_length":65536,"default":original,"bindings":[{"step_id":"file","path":"/content","allowed_json_paths":["/replicas","/note"],"scalar_rules":{"/replicas":{"kind":"integer","minimum":1,"maximum":10}}},{"step_id":"apply","path":"/environment/CONFIG_SHA256","transform":"sha256"}]}},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}})).unwrap();
    let revision = register(&mut state, source.clone());
    let before = serde_json::to_value(&state).unwrap();
    let changed = "{\"replicas\":2,\"note\":\"中文\"}\n";
    let values = BTreeMap::from([("VALUES".into(), json!(changed))]);
    let (plan, parameters, target) = state
        .workflow_plan(
            "config",
            &revision,
            "qa",
            &values,
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&state).unwrap(),
        before,
        "preview mutated state"
    );
    assert_eq!(parameters["VALUES"], changed);
    assert_eq!(
        plan.work[0].operation.definition_sha(),
        Some(sha(changed).as_str())
    );
    assert_eq!(
        serde_json::to_value(&plan.work[1].operation).unwrap()["environment"]["CONFIG_SHA256"],
        sha(changed)
    );
    let request = workflow::Request {
        workflow_uid: "config".into(),
        workflow_revision: revision.clone(),
        target: "qa".into(),
        parameters: values,
        request_id: "temporary-config".into(),
        plan_sha256: plan.sha256.clone(),
        target_sha256: target,
    };
    let result = execute(&mut state, request.clone());
    assert!(result.ok, "{}", result.code);
    assert_eq!(state.scripts[&sha(changed)].as_str(), changed);
    assert_eq!(*state.workflows.revisions[&revision], source);
    assert_eq!(state.runs["run-temporary-config"].plan.sha256, plan.sha256);
    assert_eq!(
        state.workflows.executions["run-temporary-config"].parameters["VALUES"],
        changed
    );
    assert!(execute(&mut state, request.clone()).ok);
    let mut stale = request;
    stale.request_id = "tampered-config".into();
    stale.parameters.insert("VALUES".into(), json!(original));
    let before = serde_json::to_value(&state).unwrap();
    assert!(!execute(&mut state, stale).ok);
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    assert!(
        state
            .workflow_plan(
                "config",
                &revision,
                "qa",
                &BTreeMap::from([("VALUES".into(), json!("{broken"))]),
                &converyn_plugin_api::NoPlugins
            )
            .is_err()
    );
    assert!(
        state
            .workflow_plan(
                "config",
                &revision,
                "qa",
                &BTreeMap::from([(
                    "VALUES".into(),
                    json!("{\"replicas\":2,\"note\":\"ok\",\"serviceAccount\":\"other\"}")
                )]),
                &converyn_plugin_api::NoPlugins
            )
            .is_err()
    );
    for value in [
        json!({"replicas":2,"note":"{{ lookup secret }}"}),
        json!({"replicas":2,"note":"first\nsecond"}),
        json!({"replicas":1000,"note":"ok"}),
        json!({"replicas":1.5,"note":"ok"}),
    ] {
        assert!(
            state
                .workflow_plan(
                    "config",
                    &revision,
                    "qa",
                    &BTreeMap::from([("VALUES".into(), json!(value.to_string()))]),
                    &converyn_plugin_api::NoPlugins
                )
                .is_err()
        );
    }
    let (default_plan, _, _) = state
        .workflow_plan(
            "config",
            &revision,
            "qa",
            &BTreeMap::new(),
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    assert_eq!(
        default_plan.work[0].operation.definition_sha(),
        Some(sha(original).as_str())
    );
}
#[tokio::test]
async fn snapshots_and_cold_audit_include_workflows_and_execution_identity() {
    use converyn_runtime::storage::store::Store;
    use openraft::{CommittedLeaderId, Entry, EntryPayload, LogId, storage::RaftStateMachine};
    let d = tempfile::tempdir().unwrap();
    let mut store = Store::open(d.path(), 1).unwrap();
    let mut state = state();
    let sha = register(&mut state, workflow());
    store
        .restore_business_state(state.clone(), "workflow")
        .await
        .unwrap();
    let r = request(&state, &sha, "persisted");
    let command = Command::Workflow {
        change: Change::Execute {
            output_key: None,
            request: Box::new(r),
            actor: workflow::Actor {
                uid: "user-alice".into(),
                name: "Alice".into(),
                fingerprint: "a".repeat(64),
                credential_uid: "credential-alice".into(),
            },
            now_ms: 1000,
        },
    };
    let replies = store
        .apply([Entry {
            log_id: LogId::new(CommittedLeaderId::new(1, 1), 1),
            payload: EntryPayload::Normal(command),
        }])
        .await
        .unwrap();
    assert!(replies[0].ok, "{}", replies[0].code);
    let audit = store.audit_page("commands".into(), 0, 10).await.unwrap();
    assert_eq!(audit[0]["execution"]["actor"]["uid"], "user-alice");
    let (backup, _) = store.backup_carrier().await.unwrap();
    converyn_runtime::storage::carrier::validate(&backup).unwrap();
    drop(store);
    let store = Store::open(d.path(), 1).unwrap();
    state = store.machine().await.unwrap().state.clone();
    assert_eq!(
        state.workflows.executions["run-persisted"].actor.name,
        "Alice"
    );
}

#[test]
fn required_business_parameter_registers_from_valid_template_but_execution_requires_input() {
    let mut state = state();
    let mut source = workflow();
    source.parameters.clear();
    source.parameters.insert(
        "URL".into(),
        serde_json::from_value(
            json!({"kind":"string","bindings":[{"step_id":"check","path":"/url"}]}),
        )
        .unwrap(),
    );
    let sha = register(&mut state, source);
    assert!(
        state
            .workflow_plan(
                "health",
                &sha,
                "qa",
                &BTreeMap::new(),
                &converyn_plugin_api::NoPlugins
            )
            .is_err()
    );
    assert!(
        state
            .workflow_plan(
                "health",
                &sha,
                "qa",
                &BTreeMap::from([("URL".into(), json!("https://example.invalid/ready"))]),
                &converyn_plugin_api::NoPlugins
            )
            .is_ok()
    );
}

#[test]
fn own_task_controls_and_reconciliation_policy_are_checked_inside_consensus() {
    use converyn_core::identities::{personnel as people, registration::Authorization};
    use converyn_runtime::credentials::pki;
    let temp = tempfile::tempdir().unwrap();
    let ca = pki::create_authority(&temp.path().join("ca"), "test").unwrap();
    let now = converyn_runtime::executors::host::now_ms();
    let mut s = state();
    let mut authorizations = Vec::new();
    for (name, role) in [
        ("alice", people::Role::Executor),
        ("bob", people::Role::Executor),
        ("op", people::Role::SuperAdmin),
    ] {
        let uid = format!("user-{name}");
        let signed = ca.issue_personal(&temp.path().join(name), &uid).unwrap();
        let info = pki::info(&signed.certificate_pem).unwrap();
        let hash = digest(&name);
        let c = people::Credential {
            uid: format!("credential-{name}"),
            identity_uid: uid.clone(),
            certificate_pem: signed.certificate_pem,
            fingerprint: info.fingerprint.clone(),
            token_sha256: hash.clone(),
            not_before_ms: info.not_before_ms,
            expires_ms: info.expires_ms,
            revoked_ms: None,
            overlap_until_ms: None,
        };
        s.personnel
            .change(people::Change::Create {
                super_admin: role == people::Role::SuperAdmin,
                identity: people::Identity {
                    uid: uid.clone(),
                    name: name.into(),
                    disabled: false,
                    revision: 1,
                },
                credential: c,
                request_id: format!("create-{name}"),
                now_ms: now,
            })
            .unwrap();
        if role == people::Role::Executor {
            s.personnel.grant_initial(&uid, role);
        }
        authorizations.push(Authorization {
            request_id: None,
            observed_ms: Some(now),
            token_sha256: Some(hash),
            principal: uid,
            fingerprint: info.fingerprint,
        });
    }
    let mut source = workflow();
    source.reconcile_seconds = Some(30);
    let sha = register(&mut s, source);
    let input = request(&s, &sha, "own");
    let actor = workflow::Actor {
        uid: "user-alice".into(),
        name: "alice".into(),
        fingerprint: authorizations[0].fingerprint.clone(),
        credential_uid: "credential-alice".into(),
    };
    let result = s.apply(Command::Authorized {
        authorization: authorizations[0].clone(),
        command: Box::new(Command::Workflow {
            change: Change::Execute {
                output_key: None,
                request: Box::new(input),
                actor,
                now_ms: now,
            },
        }),
    });
    assert!(result.ok, "{}", result.code);
    let deployment = s.workflows.executions["run-own"]
        .deployment_uid
        .clone()
        .unwrap();
    let pause = Command::PauseRun {
        run_id: "run-own".into(),
        paused: true,
        expected_revision: s.runs["run-own"].revision,
    };
    assert!(
        !s.apply(Command::Authorized {
            authorization: authorizations[1].clone(),
            command: Box::new(pause.clone())
        })
        .ok
    );
    assert!(
        s.apply(Command::Authorized {
            authorization: authorizations[0].clone(),
            command: Box::new(pause)
        })
        .ok
    );
    assert_eq!(s.drift[&deployment].interval_seconds, None);
    let resume = Command::PauseRun {
        run_id: "run-own".into(),
        paused: false,
        expected_revision: s.runs["run-own"].revision,
    };
    assert!(
        s.apply(Command::Authorized {
            authorization: authorizations[0].clone(),
            command: Box::new(resume)
        })
        .ok
    );
    assert_eq!(s.drift[&deployment].interval_seconds, Some(30));
    assert!(
        s.apply(Command::Authorized {
            authorization: authorizations[2].clone(),
            command: Box::new(Command::Cancel {
                run_id: "run-own".into()
            })
        })
        .ok
    );
    assert_eq!(s.drift[&deployment].interval_seconds, None);
}

#[test]
fn repair_uses_final_parameters_and_preserves_default_and_original_attribution() {
    use converyn_core::orchestration::machine::Status;
    let mut s = state();
    let mut source = workflow();
    source.reconcile_seconds = Some(30);
    let sha = register(&mut s, source);
    let values = BTreeMap::from([("STATUS".into(), json!(204))]);
    let (plan, _, target_sha) = s
        .workflow_plan(
            "health",
            &sha,
            "qa",
            &values,
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    let input = workflow::Request {
        workflow_uid: "health".into(),
        workflow_revision: sha.clone(),
        target: "qa".into(),
        parameters: values,
        request_id: "desired".into(),
        plan_sha256: plan.sha256,
        target_sha256: target_sha,
    };
    assert!(execute(&mut s, input).ok);
    let deployment = s.workflows.executions["run-desired"]
        .deployment_uid
        .clone()
        .unwrap();
    let run = std::sync::Arc::make_mut(s.runs.get_mut("run-desired").unwrap());
    run.status = Status::Succeeded;
    for attempt in run.attempts.values_mut() {
        std::sync::Arc::make_mut(attempt).status = Status::Succeeded;
    }
    s.execution_locks.clear();
    assert!(
        s.apply(Command::ReconcileDeployment {
            uid: deployment.clone()
        })
        .ok
    );
    let result = s.apply(Command::ReconcileDrift {
        uid: deployment.clone(),
        now_ms: 30000,
        expected_generation: 1,
    });
    assert!(result.ok, "{}", result.code);
    let repair = s.drift[&deployment].active_run.as_ref().unwrap();
    assert_eq!(
        serde_json::to_value(&s.runs[repair].plan.work[0].operation).unwrap()["expected_status"],
        204
    );
    assert!(s.workflows.executions[repair].system);
    assert_eq!(s.workflows.executions[repair].actor.uid, "user-alice");
    assert_eq!(
        s.workflows.revisions[&sha].parameters["STATUS"].default,
        Some(json!(200))
    );
}

#[test]
fn publishing_new_content_does_not_reenable_disabled_workflows() {
    let mut s = state();
    register(&mut s, workflow());
    assert!(
        s.apply(Command::Workflow {
            change: Change::Enable {
                uid: "health".into(),
                enabled: false,
                expected_revision: 1
            }
        })
        .ok
    );
    let mut source = workflow();
    source.name = "Updated health check".into();
    let result = s.apply(Command::Workflow {
        change: Change::Register {
            workflow: Box::new(source),
            expected_revision: 2,
        },
    });
    assert!(result.ok, "{}", result.code);
    let item = &s.workflows.workflows["health"];
    assert!(!item.enabled);
    assert!(
        s.workflow_plan(
            "health",
            &item.current,
            "qa",
            &BTreeMap::new(),
            &converyn_plugin_api::NoPlugins
        )
        .is_err()
    );
}

#[test]
fn execution_parameters_cannot_replace_secret_or_connection_references() {
    use sha2::{Digest, Sha256};
    let script = "echo verified\n";
    let hash = hex::encode(Sha256::digest(script));
    for (name, secret) in [
        ("PASSWORD", true),
        ("KUBECONFIG", false),
        ("DOCKER_HOST", false),
    ] {
        let mut source = workflow();
        source.definitions.insert(hash.clone(), script.into());
        let environment = if secret {
            json!({"PASSWORD":{"secret":"password","version":"one"}})
        } else {
            json!({})
        };
        source.playbook.steps[0].operation=serde_json::from_value(json!({"kind":"exec","script_sha256":hash,"timeout_seconds":30,"environment":environment})).unwrap();
        source.parameters.clear();
        source.parameters.insert("INPUT".into(),serde_json::from_value(json!({"kind":"string","default":"value","bindings":[{"step_id":"check","path":format!("/environment/{name}")}]})).unwrap());
        assert!(source.validate().is_err(), "protected binding {name}");
    }
}
