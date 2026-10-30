mod support;
use converyn_core::{
    model::{
        domain::{Operation, Plan, PlanInput},
        outputs::StoredValue,
        receipt::{Outcome, Receipt},
    },
    orchestration::machine::{Command, State, Status},
};
use converyn_runtime::{plugins, storage::store::Store};
use serde_json::json;
use std::sync::Arc;
fn fixture() -> (State, Plan) {
    let call = plugins::registry()
        .get("converyn.linux-host")
        .unwrap()
        .compile("initramfs", json!({"requested":false}))
        .unwrap();
    let input:PlanInput=serde_json::from_value(json!({"project":"default","owner_uid":"owner","inventory":{"nodes":[{"uid":"master","hostname":"master","address":"192.0.2.1"},{"uid":"worker","hostname":"worker","address":"192.0.2.2"}],"groups":{"master":["master"],"worker":["worker"]}},"hosts":{"node_group_uid":"hosts","revision":1,"node_uids":["master","worker"]},"batch_size":2,"playbook":{"api_version":"converyn/v1","name":"plugin-input","steps":[{"id":"produce","phase":"prepare","scope":"per_node","hosts":"master","outputs":{"requested":{"type":"boolean"}},"operation":{"kind":"exec","script_sha256":"a".repeat(64),"timeout_seconds":60}},{"id":"consume","phase":"rollout","scope":"per_node","hosts":"worker","inputs":{"/call/spec/requested":{"step":"produce","name":"requested","node":"master"}},"operation":{"kind":"plugin","call":call}}]}})).unwrap();
    let p = input.compile_with(plugins::registry()).unwrap();
    let mut s = support::state();
    support::scope(&mut s, &p);
    let identity = match &p.work[1].operation {
        Operation::Plugin { call } => call.plugin.clone(),
        _ => unreachable!(),
    };
    s.observations.insert("worker".into(),serde_json::from_value(json!({"boot_id":"boot","session_id":"session","observed_ms":100,"capabilities":[identity]})).unwrap());
    assert!(
        s.apply_with(
            Command::CreateRun {
                output_key: None,
                run_id: "run".into(),
                idempotency_key: "run".into(),
                plan: Box::new(p.clone())
            },
            plugins::registry()
        )
        .ok
    );
    let producer = &p.work[0];
    let run = Arc::make_mut(s.runs.get_mut("run").unwrap());
    let attempt = Arc::make_mut(run.attempts.get_mut(&producer.id).unwrap());
    attempt.status = Status::Succeeded;
    attempt.grant = Some("producer-grant".into());
    attempt.completed_ms = Some(100);
    attempt.receipt = Some(Receipt {
        outputs: [(
            "requested".into(),
            StoredValue::Public {
                value: json!(false),
            },
        )]
        .into(),
        diagnostic: None,
        execution: None,
        logs: None,
        native: None,
        operation_id: format!("run/{}", producer.id),
        script_sha256: "a".repeat(64),
        timeout_seconds: 60,
        outcome: Outcome::Succeeded,
        exit_code: Some(0),
        boot_id: "boot".into(),
    });
    let reply = s.apply_with(
        Command::Materialize {
            run_id: "run".into(),
            work_id: p.work[1].id.clone(),
            expected_revision: s.runs["run"].revision,
            now_ms: 200,
        },
        plugins::registry(),
    );
    assert!(reply.ok);
    (s, p)
}
fn claim(s: &State, p: &Plan) -> Command {
    let w = s.runs["run"].effective_work(&p.work[1].id).unwrap();
    Command::Claim {
        input_sha256: w.input_sha256.clone(),
        proof: None,
        authorized_ms: Some(300),
        run_id: "run".into(),
        work_id: w.id.clone(),
        node_uid: w.node_uid.clone(),
        grant: "consumer-grant".into(),
        boot_id: "boot".into(),
        session_id: "session".into(),
    }
}
fn failed_fixture() -> (State, Plan) {
    let (mut s, p) = fixture();
    s.apply(claim(&s, &p));
    assert_eq!(s.runs["run"].status, Status::Failed);
    assert!(s.runs["run"].can_retry_inputs());
    (s, p)
}
#[tokio::test]
async fn plugin_input_claim_records_contracts_for_replica_replay() {
    let (s, p) = fixture();
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(root.path(), 1).unwrap();
    store
        .restore_business_state(s.clone(), &"a".repeat(64))
        .await
        .unwrap();
    let command = store.prepare_command(claim(&s, &p)).await.unwrap();
    assert!(matches!(&command, Command::Prepared { .. }));
    let mut replica = s;
    let reply = replica.apply(command);
    assert!(
        reply.ok,
        "reply={} reason={:?}",
        reply.code, replica.runs["run"].attempts[&p.work[1].id].reason
    );
    assert_eq!(
        replica.runs["run"].attempts[&p.work[1].id].status,
        Status::Authorized
    );
}
#[test]
fn retry_inputs_preserves_success_and_requires_fresh_admission() {
    let (mut s, p) = failed_fixture();
    let producer = s.runs["run"].attempts[&p.work[0].id].clone();
    let revision = s.runs["run"].revision;
    assert!(
        s.apply(Command::RetryInputs {
            run_id: "run".into(),
            expected_revision: revision
        })
        .ok
    );
    let r = &s.runs["run"];
    assert!(r.preparing);
    assert_eq!(r.status, Status::Preparing);
    assert_eq!(r.attempts[&p.work[0].id], producer);
    assert_eq!(r.attempts[&p.work[1].id].status, Status::Pending);
    assert!(r.attempts[&p.work[1].id].materialized.is_none());
}
#[test]
fn retry_inputs_rejects_granted_failures_and_changed_scope() {
    let (mut s, p) = failed_fixture();
    let revision = s.runs["run"].revision;
    Arc::make_mut(
        Arc::make_mut(s.runs.get_mut("run").unwrap())
            .attempts
            .get_mut(&p.work[1].id)
            .unwrap(),
    )
    .grant = Some("possible-side-effect".into());
    assert!(
        !s.apply(Command::RetryInputs {
            run_id: "run".into(),
            expected_revision: revision
        })
        .ok
    );
    let (mut s, _) = failed_fixture();
    s.node_groups
        .get_mut("hosts")
        .unwrap()
        .node_uids
        .remove("worker");
    let revision = s.runs["run"].revision;
    assert!(
        !s.apply(Command::RetryInputs {
            run_id: "run".into(),
            expected_revision: revision
        })
        .ok
    );
    assert_eq!(s.runs["run"].status, Status::Failed);
}

#[test]
fn retry_inputs_rejects_a_newer_execution_of_the_same_owner() {
    let (mut state, _) = failed_fixture();
    let mut newer = (*state.runs["run"]).clone();
    newer.id = "newer".into();
    newer.sequence += 1;
    state.runs.insert("newer".into(), Arc::new(newer));
    let revision = state.runs["run"].revision;
    let reply = state.apply(Command::RetryInputs {
        run_id: "run".into(),
        expected_revision: revision,
    });
    assert!(!reply.ok);
    assert_eq!(reply.code, "newer_run_executed");
    assert_eq!(state.runs["run"].status, Status::Failed);
}
