mod support;
// Public plugin contracts exercised through registered workflows; no claim of
// a real Kubernetes cluster or VM operation is made by this pure fixture.
#[cfg(feature = "plugin-kubernetes")]
mod kubernetes {
    use converyn_core::{
        model::{
            domain::*,
            evidence::Evidence,
            receipt::{Outcome, Receipt},
            target::ExternalTarget,
        },
        orchestration::{
            administration,
            machine::{Command, State, Status},
            workflow::{self, Change},
        },
    };
    use converyn_plugin_api::{self as api, Contracts};
    use serde_json::json;
    fn target(uid: &str, namespace: &str) -> ExternalTarget {
        ExternalTarget {
            uid: uid.into(),
            revision: 1,
            project: "default".into(),
            plugin: converyn_runtime::plugins::registry()
                .get("converyn.kubernetes")
                .unwrap()
                .descriptor()
                .identity,
            connection: "local".into(),
            spec: json!({"namespace":namespace,"identity":{"cluster_uid":"cluster-real","namespace_uid":format!("uid-{namespace}"),"version":"v1.37.0"}}),
        }
    }
    fn apply(state: &mut State, command: Command) {
        let r = state.apply_with(command, converyn_runtime::plugins::registry());
        assert!(r.ok, "{}", r.code);
    }
    fn setup() -> State {
        let registry = converyn_runtime::plugins::registry();
        let mut state = crate::support::state();
        for value in [target("stage", "stage"), target("prod", "prod")] {
            apply(
                &mut state,
                Command::Administer {
                    change: administration::Change::Target { target: value },
                },
            );
        }
        let call=registry.get("converyn.kubernetes").unwrap().compile("apply",json!({"target":target("stage","stage").spec,"objects":[{"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"config","namespace":"stage"},"data":{"value":"one"}}],"timeout_seconds":60})).unwrap();
        let playbook = Playbook {
            api_version: "converyn/v1".into(),
            name: "publish".into(),
            steps: vec![Step {
                hosts: None,
                allow_empty: false,
                node_order: Vec::new(),
                outputs: Default::default(),
                inputs: Default::default(),
                id: "apply".into(),
                phase: Phase::Rollout,
                scope: Scope::PerNode,
                max_parallel: None,
                operation: Operation::Plugin {
                    call: Box::new(call),
                },
            }],
        };
        let source:workflow::Revision=serde_json::from_value(json!({"uid":"app","name":"ConfigMap workflow","playbook":playbook,"targets":{"stage":{"external_target_uid":"stage"},"prod":{"external_target_uid":"prod"}},"parameters":{"VALUE":{"kind":"string","default":"one","bindings":[{"step_id":"apply","path":"/call/spec/objects/0/data/value"}]}}})).unwrap();
        apply(
            &mut state,
            Command::Workflow {
                change: Change::Register {
                    workflow: Box::new(source),
                    expected_revision: 0,
                },
            },
        );
        state
    }
    fn execution(state: &State, id: &str, target: &str) -> Command {
        let revision = &state.workflows.workflows["app"].current;
        let (plan, _, target_sha) = state
            .workflow_plan(
                "app",
                revision,
                target,
                &Default::default(),
                converyn_runtime::plugins::registry(),
            )
            .unwrap();
        Command::Workflow {
            change: Change::Execute {
                output_key: None,
                request: Box::new(workflow::Request {
                    workflow_uid: "app".into(),
                    workflow_revision: revision.clone(),
                    target: target.into(),
                    parameters: Default::default(),
                    request_id: id.into(),
                    plan_sha256: plan.sha256,
                    target_sha256: target_sha,
                }),
                actor: workflow::Actor {
                    uid: "user-operator".into(),
                    name: "Operator".into(),
                    fingerprint: "a".repeat(64),
                    credential_uid: "credential-operator".into(),
                },
                now_ms: 1,
            },
        }
    }
    fn finish(state: &mut State, id: &str) {
        let id = format!("run-{id}");
        let run = state.runs[&id].clone();
        let w = run.plan.work[0].clone();
        let Operation::Plugin { call } = &w.operation else {
            panic!()
        };
        apply(
            state,
            Command::Claim {
                input_sha256: None,
                proof: None,
                authorized_ms: Some(1),
                run_id: id.clone(),
                work_id: w.id.clone(),
                node_uid: w.node_uid.clone(),
                grant: "grant".into(),
                boot_id: "controller".into(),
                session_id: "controller".into(),
            },
        );
        apply(
            state,
            Command::Report {
                proof: None,
                run_id: id.clone(),
                work_id: w.id.clone(),
                node_uid: w.node_uid.clone(),
                grant: "grant".into(),
                sequence: 1,
                status: Status::Running,
                receipt: None,
            },
        );
        let payload = json!({"identity":call.spec["target"]["identity"],"fence":run.sequence,"resources":[{"uid":"actual-resource","resource_version":"2","generation":0,"observed_generation":0,"ready":true}]});
        let mut evidence = Evidence::empty(&w.operation);
        evidence.plugin = Some(api::Observation {
            owner: run.plan.input.owner_uid.clone(),
            fence: run.sequence,
            plugin: call.plugin.clone(),
            call_sha256: api::digest(call),
            payload,
            verified: true,
        });
        let receipt = Receipt {
            outputs: Default::default(),
            diagnostic: None,
            execution: None,
            logs: None,
            native: Some(Box::new(evidence)),
            operation_id: format!("{id}/{}", w.id),
            script_sha256: String::new(),
            timeout_seconds: 0,
            outcome: Outcome::Succeeded,
            exit_code: None,
            boot_id: "controller".into(),
        };
        apply(
            state,
            Command::Report {
                proof: None,
                run_id: id,
                work_id: w.id,
                node_uid: w.node_uid,
                grant: "grant".into(),
                sequence: 2,
                status: Status::Succeeded,
                receipt: Some(receipt),
            },
        );
    }
    #[test]
    fn recorded_workflow_binding_replays_without_any_installed_plugin() {
        let base = setup();
        let command = execution(&base, "recorded", "prod");
        let recorder =
            converyn_core::model::contracts::Recorder::new(converyn_runtime::plugins::registry());
        let mut expected = base.clone();
        assert!(expected.apply_with(command.clone(), &recorder).ok);
        let sealed = Command::Prepared {
            command: Box::new(command.clone()),
            contracts: recorder.finish(),
        };
        let mut replay = base.clone();
        assert!(
            replay
                .apply(serde_json::from_slice(&serde_json::to_vec(&sealed).unwrap()).unwrap())
                .ok
        );
        assert_eq!(replay, expected);
        let mut missing = base;
        assert!(
            !missing
                .apply(Command::Prepared {
                    command: Box::new(command),
                    contracts: Default::default()
                })
                .ok
        );
        assert!(!missing.runs.contains_key("run-recorded"));
    }
    #[test]
    fn preset_binding_is_fixed_before_execution_without_approval_objects() {
        let mut state = setup();
        let c = execution(&state, "stage-first", "stage");
        apply(&mut state, c);
        finish(&mut state, "stage-first");
        let c = execution(&state, "prod-first", "prod");
        apply(&mut state, c);
        let run = &state.runs["run-prod-first"];
        let Operation::Plugin { call } = &run.plan.work[0].operation else {
            panic!()
        };
        assert_eq!(call.spec["target"], state.targets["prod"].spec);
        assert_eq!(call.spec["objects"][0]["metadata"]["namespace"], "prod");
        assert!(call.resources[0].contains("uid-prod"));
        finish(&mut state, "prod-first");
        assert_eq!(state.runs["run-prod-first"].status, Status::Succeeded);
    }
    #[test]
    fn forged_success_is_rejected_by_pure_plugin_contract() {
        let registry = converyn_runtime::plugins::registry();
        let state = setup();
        let source = &state.workflows.revisions[&state.workflows.workflows["app"].current];
        let Operation::Plugin { call } = &source.playbook.steps[0].operation else {
            panic!()
        };
        let evidence = api::Observation {
            owner: "owner".into(),
            fence: 1,
            plugin: call.plugin.clone(),
            call_sha256: api::digest(call),
            payload: json!({"ready":true}),
            verified: true,
        };
        assert!(
            !registry
                .verify_observation(call, &evidence)
                .unwrap_or(false)
        );
    }
}

#[cfg(feature = "plugin-kubernetes")]
#[test]
fn node_rollout_and_controller_verification_share_one_run_and_both_target_locks() {
    use converyn_core::{
        model::{domain::*, target::ExternalTarget},
        orchestration::{administration, machine::Command},
    };
    use converyn_plugin_api::Location;
    use serde_json::json;
    let registry = converyn_runtime::plugins::registry();
    let target = ExternalTarget {
        uid: "external".into(),
        revision: 1,
        project: "team".into(),
        plugin: registry
            .get("converyn.kubernetes")
            .unwrap()
            .descriptor()
            .identity,
        connection: "local".into(),
        spec: json!({"namespace":"default","identity":{"cluster_uid":"cluster","namespace_uid":"ns","version":"v1.37.0"}}),
    };
    let call=registry.get("converyn.kubernetes").unwrap().compile("apply",json!({"target":target.spec,"objects":[{"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"config"},"data":{"value":"one"}}],"timeout_seconds":60})).unwrap();
    let value = json!({"project":"team","owner_uid":"owner","target":target,"parameters":{},"batch_size":1,"playbook":{"api_version":"converyn/v1","name":"mixed","steps":[{"id":"host","phase":"rollout","scope":"per_node","operation":{"kind":"file","path":"/opt/app/config","sha256":"a".repeat(64),"mode":420},"hosts":"worker"},{"id":"external","phase":"verify","scope":"once_per_run","operation":{"kind":"plugin","call":call}}]},"inventory":{"nodes":[{"uid":"node1","hostname":"node1","address":"192.0.2.1"},{"uid":"node2","hostname":"node2","address":"192.0.2.2"}],"groups":{"worker":["node1","node2"]}},"hosts":{"node_group_uid":"hosts","revision":1,"node_uids":["node1","node2"]}});
    let input: PlanInput = serde_json::from_value(value).unwrap();
    let plan = input.compile_with(registry).unwrap();
    assert_eq!(plan.work.len(), 3);
    assert_eq!(plan.work[2].operation.location(), Location::Controller);
    assert!(plan.work[2].wave > plan.work[1].wave);
    assert_eq!(
        plan.lock_keys().into_iter().collect::<Vec<_>>(),
        vec!["external/external", "node/node1", "node/node2"]
    );
    let mut state = crate::support::state();
    assert!(
        state
            .apply_with(
                Command::Administer {
                    change: administration::Change::Target {
                        target: plan.input.target.clone().unwrap()
                    }
                },
                registry
            )
            .ok
    );
    assert!(
        state
            .apply_with(
                Command::CreateRun {
                    output_key: None,
                    run_id: "mixed".into(),
                    idempotency_key: "mixed".into(),
                    plan: Box::new(plan.clone())
                },
                registry
            )
            .ok
    );
    assert_eq!(state.node_groups.len(), 5);
    assert_eq!(state.execution_locks.len(), 3);
    let mut conflicting = plan.input.clone();
    conflicting.target = None;
    conflicting.playbook.steps.pop();
    let r = state.apply_with(
        Command::CreateRun {
            output_key: None,
            run_id: "conflict".into(),
            idempotency_key: "conflict".into(),
            plan: Box::new(conflicting.compile().unwrap()),
        },
        registry,
    );
    assert!(!r.ok);
    assert_eq!(r.code, "node_busy");
    assert!(
        state
            .apply_with(
                Command::Cancel {
                    run_id: "mixed".into()
                },
                registry
            )
            .ok
    );
    assert!(state.execution_locks.is_empty());
}
