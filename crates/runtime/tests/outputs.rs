mod support;
use converyn_core::{
    model::{
        domain::{Operation, Plan, PlanInput},
        outputs::{self, Specification, StoredValue, ValueType},
        receipt::{Outcome, Receipt},
    },
    orchestration::machine::{Command, State, Status},
};
use converyn_runtime::{
    credentials::outputs::{Keyring, binding, seal},
    executors::{
        outputs::Context as OutputContext,
        runner::{Options, run_script_options},
    },
};
use serde_json::json;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

fn plan(secret: bool) -> Plan {
    serde_json::from_value::<PlanInput>(json!({"project":"default","owner_uid":"owner","parameters":{},"batch_size":2,"playbook":{"api_version":"converyn/v1","name":"outputs","steps":[{"id":"produce","phase":"prepare","scope":"once_per_run","outputs":{"message":{"type":"string","secret":secret,"ttl_seconds":60}},"operation":{"kind":"exec","script_sha256":"a".repeat(64),"timeout_seconds":3600},"hosts":"master"},{"id":"consume","phase":"rollout","scope":"per_node","inputs":{"/environment/MESSAGE":{"step":"produce","name":"message"}},"operation":{"kind":"exec","script_sha256":"b".repeat(64),"timeout_seconds":10},"hosts":"worker"}]},"inventory":{"nodes":[{"uid":"master","hostname":"master","address":"192.0.2.1"},{"uid":"worker","hostname":"worker","address":"192.0.2.2"}],"groups":{"master":["master"],"worker":["worker"]}},"hosts":{"node_group_uid":"hosts","revision":1,"node_uids":["master","worker"]}})).unwrap().compile().unwrap()
}
fn auth(command: Command, at: u64) -> Command {
    Command::Authorized {
        authorization: converyn_core::identities::registration::Authorization {
            principal: "peer".into(),
            fingerprint: "f".repeat(64),
            token_sha256: None,
            observed_ms: Some(at),
            request_id: None,
        },
        command: Box::new(command),
    }
}
fn create(state: &mut State, p: &Plan, key: Option<outputs::PublicKey>, id: &str) {
    let reply = state.apply(Command::CreateRun {
        output_key: key,
        run_id: id.into(),
        idempotency_key: id.into(),
        plan: Box::new(p.clone()),
    });
    assert!(reply.ok, "{}", reply.code);
}
fn claim(
    state: &mut State,
    p: &Plan,
    index: usize,
    at: u64,
) -> converyn_core::orchestration::machine::Reply {
    let w = &p.work[index];
    let sha = state.runs["run"]
        .effective_work(&w.id)
        .and_then(|w| w.input_sha256.clone());
    state.apply(Command::Claim {
        input_sha256: sha,
        proof: None,
        authorized_ms: Some(at),
        run_id: "run".into(),
        work_id: w.id.clone(),
        node_uid: w.node_uid.clone(),
        grant: format!("grant-{index}"),
        boot_id: "boot".into(),
        session_id: "session".into(),
    })
}
fn finish(state: &mut State, p: &Plan, values: outputs::Values, at: u64) {
    assert!(claim(state, p, 0, 100).ok);
    let work = &p.work[0];
    let running = Command::Report {
        proof: None,
        run_id: "run".into(),
        work_id: work.id.clone(),
        node_uid: work.node_uid.clone(),
        grant: "grant-0".into(),
        sequence: 1,
        status: Status::Running,
        receipt: None,
    };
    assert!(state.apply(auth(running, at - 1)).ok);
    let receipt = Receipt {
        outputs: values,
        diagnostic: None,
        execution: None,
        logs: None,
        native: None,
        operation_id: format!("run/{}", work.id),
        script_sha256: "a".repeat(64),
        timeout_seconds: 3600,
        outcome: Outcome::Succeeded,
        exit_code: Some(0),
        boot_id: "boot".into(),
    };
    let reply = state.apply(auth(
        Command::Report {
            proof: None,
            run_id: "run".into(),
            work_id: work.id.clone(),
            node_uid: work.node_uid.clone(),
            grant: "grant-0".into(),
            sequence: 2,
            status: Status::Succeeded,
            receipt: Some(receipt),
        },
        at,
    ));
    assert!(reply.ok, "{}", reply.code);
}
fn materialize(state: &mut State, p: &Plan, at: u64) {
    let reply = state.apply(auth(
        Command::Materialize {
            run_id: "run".into(),
            work_id: p.work[1].id.clone(),
            expected_revision: state.runs["run"].revision,
            now_ms: at,
        },
        at,
    ));
    assert!(reply.ok, "{}", reply.code);
}
#[test]
fn outputs_are_committed_with_receipts_and_inputs_are_frozen_without_changing_plan() {
    let p = plan(false);
    let mut state = crate::support::state();
    create(&mut state, &p, None, "run");
    assert!(state.runs["run"].effective_work(&p.work[1].id).is_none());
    finish(
        &mut state,
        &p,
        BTreeMap::from([(
            "message".into(),
            StoredValue::Public {
                value: json!("complete"),
            },
        )]),
        10000,
    );
    materialize(&mut state, &p, 10001);
    let run = &state.runs["run"];
    assert_eq!(*run.plan, p);
    let resolved = run.effective_work(&p.work[1].id).unwrap();
    let Operation::Exec { environment, .. } = &resolved.operation else {
        panic!()
    };
    assert_eq!(environment["MESSAGE"], "complete".to_string().into());
    assert!(resolved.input_sha256.is_some());
    assert!(claim(&mut state, &p, 1, 10002).ok);
    let restored: State = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(
        restored.runs["run"].effective_work(&p.work[1].id),
        state.runs["run"].effective_work(&p.work[1].id)
    );
}
#[test]
fn missing_output_receipt_and_future_references_are_rejected() {
    let p = plan(false);
    let mut state = crate::support::state();
    create(&mut state, &p, None, "run");
    assert!(claim(&mut state, &p, 0, 1).ok);
    let w = &p.work[0];
    let r = state.apply(Command::Report {
        proof: None,
        run_id: "run".into(),
        work_id: w.id.clone(),
        node_uid: w.node_uid.clone(),
        grant: "grant-0".into(),
        sequence: 1,
        status: Status::Succeeded,
        receipt: None,
    });
    assert!(!r.ok);
    let mut input = p.input.clone();
    input.playbook.steps[0].inputs.insert(
        "/environment/FUTURE".into(),
        outputs::Reference {
            step: "consume".into(),
            name: "message".into(),
            node: None,
            batch: None,
            pointer: None,
            collect: false,
        },
    );
    assert!(input.compile().is_err());
}
#[test]
fn secret_results_are_sealed_bound_to_scope_and_rotation_keeps_old_results_readable() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("keys.json");
    let public = Keyring::generate(&file, false).unwrap();
    let spec = Specification {
        value_type: ValueType::String,
        secret: true,
        max_bytes: 4096,
        ttl_seconds: 60,
        source: None,
    };
    let value = json!("never-in-consensus");
    let aad = binding("run", "work", "token");
    let encrypted = seal(&public, &value, &aad).unwrap();
    let encoded = serde_json::to_string(&encrypted).unwrap();
    assert!(!encoded.contains("never-in-consensus"));
    assert!(!encoded.contains("plaintext_sha256"));
    assert_eq!(
        Keyring::load(&file)
            .unwrap()
            .open(&encrypted, &aad, &spec)
            .unwrap(),
        value
    );
    assert!(
        Keyring::load(&file)
            .unwrap()
            .open(&encrypted, &binding("other", "work", "token"), &spec)
            .is_err()
    );
    let next = Keyring::generate(&file, true).unwrap();
    assert_ne!(next.id, public.id);
    assert_eq!(
        Keyring::load(&file)
            .unwrap()
            .open(&encrypted, &aad, &spec)
            .unwrap(),
        value
    );
}
#[test]
fn expiry_starts_at_output_commit_and_cannot_revoke_an_existing_grant_retry() {
    let dir = tempfile::tempdir().unwrap();
    let public = Keyring::generate(&dir.path().join("keys"), false).unwrap();
    let p = plan(true);
    let mut state = crate::support::state();
    create(&mut state, &p, Some(public.clone()), "run");
    let value = seal(
        &public,
        &json!("token"),
        &binding("run", &p.work[0].id, "message"),
    )
    .unwrap();
    finish(
        &mut state,
        &p,
        BTreeMap::from([("message".into(), StoredValue::Secret { sealed: value })]),
        100000,
    );
    assert_eq!(
        state.runs["run"].attempts[&p.work[0].id].completed_ms,
        Some(100000)
    );
    materialize(&mut state, &p, 110000);
    assert!(claim(&mut state, &p, 1, 110001).ok);
    assert!(claim(&mut state, &p, 1, 999999).ok);
    assert_eq!(
        state.runs["run"].attempts[&p.work[1].id].status,
        Status::Authorized
    );
    let serialized = serde_json::to_string(&state).unwrap();
    assert!(!serialized.contains("\"token\""));
}
#[test]
fn expired_materialization_terminates_and_releases_original_locks() {
    let dir = tempfile::tempdir().unwrap();
    let public = Keyring::generate(&dir.path().join("keys"), false).unwrap();
    let p = plan(true);
    let mut state = crate::support::state();
    create(&mut state, &p, Some(public.clone()), "run");
    let value = seal(
        &public,
        &json!("short"),
        &binding("run", &p.work[0].id, "message"),
    )
    .unwrap();
    finish(
        &mut state,
        &p,
        BTreeMap::from([("message".into(), StoredValue::Secret { sealed: value })]),
        1000,
    );
    materialize(&mut state, &p, 70000);
    assert_eq!(state.runs["run"].status, Status::Failed);
    assert!(state.execution_locks.is_empty());
    assert!(state.runs["run"].finished_ms.is_some());
    create(&mut state, &p, Some(public), "new-run");
}
#[test]
fn real_script_result_capture_is_idempotent_and_rejects_unreported_fields() {
    let d = tempfile::tempdir().unwrap();
    let script = d.path().join("script.sh");
    let code =
        "printf '{\"message\":\"ok\"}' > \"$CONVERYN_OUTPUT_FILE\"\nprintf 'ordinary log\\n'\n";
    std::fs::write(&script, code).unwrap();
    let context = OutputContext {
        run_id: "run".into(),
        work_id: "work".into(),
        specifications: plan(false).work[0].outputs.clone(),
        key: None,
    };
    let invoke = || {
        run_script_options(
            &d.path().join("state"),
            "run/work",
            &script,
            &hex::encode(sha2::Sha256::digest(code)),
            10,
            &AtomicBool::new(false),
            Options {
                policy: None,
                environment: None,
                outputs: Some(&context),
            },
        )
        .unwrap()
    };
    let first = invoke();
    assert_eq!(first.outcome, Outcome::Succeeded);
    assert!(matches!(&first.outputs["message"],StoredValue::Public{value} if value=="ok"));
    assert_eq!(first, invoke());
}
use sha2::Digest;

#[test]
fn secret_producer_logs_are_suppressed_and_completion_survives_journal_recovery() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } != 0 {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o711)).unwrap();
    let key = Keyring::generate(&d.path().join("keyring"), false).unwrap();
    let code = "printf '{\"message\":\"runtime-generated-secret\"}' > \"$CONVERYN_OUTPUT_FILE\"\nprintf 'runtime-generated-secret\\n'\nprintf 'runtime-generated-secret\\n' >&2\n";
    let script = d.path().join("script");
    std::fs::write(&script, code).unwrap();
    let policy = converyn_runtime::executors::execution::Policy {
        privileged: false,
        uid: 65534,
        gid: 65534,
        mode: converyn_runtime::executors::execution::Mode::Process,
        work_root: d.path().join("tasks"),
        home: None,
        writable_paths: vec![],
        memory_bytes: 128 * 1024 * 1024,
        tasks_max: 64,
    };
    let context = OutputContext {
        run_id: "secret-run".into(),
        work_id: "producer".into(),
        specifications: plan(true).work[0].outputs.clone(),
        key: Some(key),
    };
    let state = d.path().join("state");
    let sha = hex::encode(sha2::Sha256::digest(code));
    let invoke = || {
        run_script_options(
            &state,
            "secret-run/producer",
            &script,
            &sha,
            10,
            &AtomicBool::new(false),
            Options {
                policy: Some(&policy),
                environment: None,
                outputs: Some(&context),
            },
        )
        .unwrap()
    };
    let first = invoke();
    assert_eq!(first.outcome, Outcome::Succeeded);
    assert!(first.logs.is_none());
    let encoded = serde_json::to_string(&first).unwrap();
    assert!(!encoded.contains("runtime-generated-secret"));
    let StoredValue::Secret { sealed } = &first.outputs["message"] else {
        panic!()
    };
    assert_eq!(
        Keyring::load(&d.path().join("keyring"))
            .unwrap()
            .open(
                sealed,
                &binding("secret-run", "producer", "message"),
                &context.specifications["message"]
            )
            .unwrap(),
        json!("runtime-generated-secret")
    );
    let journal = state.join(hex::encode(sha2::Sha256::digest("secret-run/producer")));
    for name in ["stdout.log", "stderr.log"] {
        assert_eq!(std::fs::read(journal.join(name)).unwrap(), b"");
    }
    assert!(
        !std::fs::read_to_string(journal.join("completion.json"))
            .unwrap()
            .contains("runtime-generated-secret")
    );
    // Reproduce a crash after completion commit but before the supervisor receipt commit.
    let mut interrupted = first.clone();
    interrupted.outcome = Outcome::Running;
    interrupted.outputs.clear();
    std::fs::write(
        journal.join("receipt.json"),
        serde_json::to_vec(&interrupted).unwrap(),
    )
    .unwrap();
    std::fs::remove_file(&script).unwrap();
    assert_eq!(invoke(), first);
}

#[test]
fn unsafe_output_permissions_and_swallowed_writer_errors_fail_the_task() {
    let d = tempfile::tempdir().unwrap();
    for (i,code) in [
        "printf '{\"message\":\"bad\"}' > \"$CONVERYN_OUTPUT_FILE\"\nchmod 644 \"$CONVERYN_OUTPUT_FILE\"\n",
        "printf '{\"message\":\"bad\"}' > \"$CONVERYN_OUTPUT_FILE\"\ntouch \"$(dirname \"$CONVERYN_OUTPUT_FILE\")/.converyn-output-error\"\n",
    ].iter().enumerate() {
        let script=d.path().join(format!("script-{i}"));std::fs::write(&script,code).unwrap();
        let context=OutputContext{run_id:"run".into(),work_id:format!("work-{i}"),specifications:plan(false).work[0].outputs.clone(),key:None};
        let receipt=run_script_options(&d.path().join("state"),&format!("run/work-{i}"),&script,&hex::encode(sha2::Sha256::digest(code)),10,&AtomicBool::new(false),Options{policy:None,environment:None,outputs:Some(&context)}).unwrap();
        assert_eq!(receipt.outcome,Outcome::Failed);assert!(receipt.outputs.is_empty());
    }
}

#[test]
fn malformed_envelope_lengths_are_rejected_without_integer_overflow() {
    let d = tempfile::tempdir().unwrap();
    let key = Keyring::generate(&d.path().join("keys"), false).unwrap();
    let mut sealed = seal(&key, &json!("value"), &binding("run", "work", "message")).unwrap();
    sealed.plaintext_bytes = usize::MAX;
    let values = BTreeMap::from([("message".into(), StoredValue::Secret { sealed })]);
    assert!(outputs::validate_stored(&plan(true).work[0], &values, Some(&key), true).is_err());
}

#[test]
fn global_role_writeback_is_rejected() {
    assert!(
        serde_json::from_value::<converyn_core::model::domain::Operation>(
            json!({"kind":"node_roles","managed_roles":["worker"],"changes":{"node":["worker"]}})
        )
        .is_err()
    );
}

#[test]
fn missing_native_output_selector_is_a_cached_contract_failure() {
    let d = tempfile::tempdir().unwrap();
    let context = OutputContext {
        run_id: "run".into(),
        work_id: "native".into(),
        key: None,
        specifications: BTreeMap::from([(
            "state".into(),
            Specification {
                value_type: ValueType::String,
                secret: false,
                max_bytes: 1024,
                ttl_seconds: 3600,
                source: Some("/missing".into()),
            },
        )]),
    };
    let receipt = Receipt {
        outputs: Default::default(),
        diagnostic: None,
        execution: None,
        logs: None,
        native: Some(Box::new(converyn_core::model::evidence::Evidence::empty(
            &Operation::Service {
                name: "app".into(),
                running: true,
                enabled: true,
                executable_sha256: None,
            },
        ))),
        operation_id: "run/native".into(),
        script_sha256: String::new(),
        timeout_seconds: 0,
        outcome: Outcome::Succeeded,
        exit_code: None,
        boot_id: "controller".into(),
    };
    let finish = || {
        converyn_runtime::executors::outputs::native_receipt(d.path(), &context, receipt.clone())
            .unwrap()
    };
    let failed = finish();
    assert_eq!(failed.outcome, Outcome::Failed);
    assert!(failed.outputs.is_empty());
    assert_eq!(failed, finish());
}
