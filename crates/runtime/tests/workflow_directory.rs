use converyn_core::{
    model::domain::{Plan, digest},
    orchestration::{
        inventory::Inventory,
        machine::{Command, State},
        workflow::{Change, Revision},
    },
};
use converyn_runtime::{credentials::pki, orchestration::workflows::package};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

fn inventory() -> Inventory {
    serde_json::from_value(json!({"nodes":[{"uid":"b","address":"192.0.2.2","hostname":"b"},{"uid":"a","address":"192.0.2.1","hostname":"a"},{"uid":"c","address":"192.0.2.3","hostname":"c"}],"groups":{"database":["c","b","a"],"empty":[]}})).unwrap()
}
fn source(uid: &str) -> Revision {
    serde_json::from_value(json!({"uid":uid,"name":"Generic database deployment","inventory":inventory(),"targets":{"qa":{"hosts":"all","node_group_uid":"hosts"}},"batch_size":3,"playbook":{"api_version":"converyn/v1","name":"database","steps":[{"id":"check","phase":"rollout","scope":"per_node","hosts":"database","max_parallel":1,"operation":{"kind":"http_check","url":"https://example.invalid/","expected_status":200,"timeout_seconds":5}}]}})).unwrap()
}
#[test]
fn packaging_rejects_invalid_fixed_artifacts_before_registration() {
    for kind in ["binary", "package"] {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("workflow.yaml");
        let mut revision = source("invalid-artifact");
        let mut operation = json!({"kind":kind,"artifact":{"sha256":"REPLACE_WITH_REVIEWED_SHA256","size":1,"locations":[{"source":"release","path":"/tool"}]}});
        if kind == "binary" {
            operation["path"] = json!("/opt/example/tool");
            operation["mode"] = json!(493);
        } else {
            operation["manager"] = json!("rpm");
            operation["name"] = json!("chrony");
            operation["version"] = json!("0:1-1.x86_64");
            operation["timeout_seconds"] = json!(120);
        }
        revision.playbook.steps[0].operation = serde_json::from_value(operation).unwrap();
        std::fs::write(&file, serde_yaml::to_string(&revision).unwrap()).unwrap();
        let error = format!("{:#}", package(&file).unwrap_err());
        assert!(error.contains("step check operation.artifact"), "{error}");
        assert!(error.contains("invalid artifact digest or size"), "{error}");

        let artifact = match &mut revision.playbook.steps[0].operation {
            converyn_core::model::domain::Operation::Binary { artifact, .. }
            | converyn_core::model::domain::Operation::Package { artifact, .. } => artifact,
            _ => unreachable!(),
        };
        artifact.sha256 = "a".repeat(64);
        artifact.size = 0;
        std::fs::write(&file, serde_yaml::to_string(&revision).unwrap()).unwrap();
        assert!(package(&file).is_err());
        match &mut revision.playbook.steps[0].operation {
            converyn_core::model::domain::Operation::Binary { artifact, .. }
            | converyn_core::model::domain::Operation::Package { artifact, .. } => {
                artifact.size = 42
            }
            _ => unreachable!(),
        }
        std::fs::write(&file, serde_yaml::to_string(&revision).unwrap()).unwrap();
        package(&file).unwrap();

        // Required parameters may supply an artifact field during admission.
        let mut parameterized = serde_json::to_value(&revision).unwrap();
        parameterized["playbook"]["steps"][0]["operation"]["artifact"]["sha256"] = json!("");
        parameterized["parameters"] = json!({"SHA":{"kind":"string","bindings":[{"step_id":"check","path":"/artifact/sha256"}]}});
        std::fs::write(&file, serde_yaml::to_string(&parameterized).unwrap()).unwrap();
        package(&file).unwrap();
    }
}
fn state() -> State {
    enrolled_state(&inventory())
}
fn enrolled_state(inventory: &Inventory) -> State {
    let d = tempfile::tempdir().unwrap();
    let ca = pki::create_authority(&d.path().join("ca"), "ca").unwrap();
    let mut s = State::default();
    for host in inventory.nodes.clone() {
        let (_, csr) = pki::request(&host.uid).unwrap();
        s.registry.nodes.insert(
            host.uid.clone(),
            Arc::new(converyn_core::identities::registration::Node {
                uid: host.uid.clone(),
                hardware_sha256: digest(&host.uid),
                token_sha256: digest(&host.uid),
                certificate: ca.sign_client(&host.uid, &csr, 3600).unwrap(),
                enrollment_id: host.uid,
                enrollment_digest: digest(&csr),
                pending: None,
                retired: false,
            }),
        );
    }
    s.node_groups.insert(
        "example-hosts".into(),
        converyn_core::model::domain::NodeGroup {
            uid: "example-hosts".into(),
            name: "Hosts".into(),
            revision: 1,
            node_uids: inventory.nodes.iter().map(|h| h.uid.clone()).collect(),
        },
    );
    let mut hosts = s.node_groups["example-hosts"].clone();
    hosts.uid = "hosts".into();
    s.node_groups.insert("hosts".into(), hosts);
    s
}

#[cfg(feature = "full")]
#[test]
fn learning_examples_register_and_compile_with_their_own_inventory() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example");
    let registry = converyn_runtime::plugins::registry();
    for (entry, preset, expected_nodes) in [
        ("workflow", "qa", vec!["node-a"]),
        ("workflow-directory", "default", vec!["node-b", "node-a"]),
        ("outputs", "default", vec!["node-a"]),
        ("node-demo", "default", vec!["node-a"]),
        ("reconciliation", "default", vec!["node-a"]),
        ("maintenance", "default", vec!["node-a"]),
        ("plugins", "default", vec!["node-a"]),
        ("plugins/docker.yaml", "default", vec!["node-a"]),
    ] {
        let revision = package(&root.join(entry)).unwrap();
        let uid = revision.uid.clone();
        let sha = digest(&revision);
        let mut state = enrolled_state(revision.inventory.as_ref().unwrap());
        assert!(state.node_groups.len() == 2);
        state
            .workflow_change(
                Change::Register {
                    workflow: Box::new(revision),
                    expected_revision: 0,
                },
                registry,
            )
            .unwrap();
        let (plan, _, _) = state
            .workflow_plan(&uid, &sha, preset, &BTreeMap::new(), registry)
            .unwrap_or_else(|error| panic!("{entry}: {error}"));
        plan.verify_with(registry).unwrap();
        assert!(!plan.work.is_empty(), "{entry}");
        let mut seen = Vec::new();
        for work in &plan.work {
            if !seen.contains(&work.node_uid.as_str()) {
                seen.push(work.node_uid.as_str());
            }
        }
        assert_eq!(seen, expected_nodes, "{entry}");
        assert!(state.node_groups.len() == 2);
    }
}

#[cfg(feature = "full")]
#[test]
fn external_learning_examples_compile_with_fixed_target_and_bounded_replicas() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/external-kubernetes");
    let registry = converyn_runtime::plugins::registry();
    for entry in ["workflow.yaml", "deployment.yaml"] {
        let revision = package(&root.join(entry)).unwrap();
        let operation = serde_json::to_value(&revision.playbook.steps[0].operation).unwrap();
        let mut state = State::default();
        let target = serde_json::from_value(json!({
            "uid":"k8s-demo", "revision":1, "project":"default", "connection":"k8s-qa",
            "plugin":operation["call"]["plugin"], "spec":operation["call"]["spec"]["target"]
        }))
        .unwrap();
        assert!(
            state
                .apply_with(
                    Command::Administer {
                        change: converyn_core::orchestration::administration::Change::Target {
                            target
                        }
                    },
                    registry
                )
                .ok
        );
        let uid = revision.uid.clone();
        let sha = digest(&revision);
        state
            .workflow_change(
                Change::Register {
                    workflow: Box::new(revision),
                    expected_revision: 0,
                },
                registry,
            )
            .unwrap();
        let mut values = BTreeMap::new();
        if entry == "deployment.yaml" {
            values.insert("replicas".into(), json!(2));
        }
        let (plan, _, _) = state
            .workflow_plan(&uid, &sha, "qa", &values, registry)
            .unwrap();
        plan.verify_with(registry).unwrap();
        assert_eq!(plan.work.len(), 1);
        assert_eq!(
            plan.work[0].operation.location(),
            converyn_plugin_api::Location::Controller
        );
        if entry == "deployment.yaml" {
            let operation = serde_json::to_value(&plan.work[0].operation).unwrap();
            assert_eq!(
                operation["call"]["spec"]["objects"][0]["spec"]["replicas"],
                2
            );
            values.insert("replicas".into(), json!(6));
            assert!(
                state
                    .workflow_plan(&uid, &sha, "qa", &values, registry)
                    .is_err()
            );
        }
        assert!(state.node_groups.is_empty());
        assert!(state.registry.nodes.is_empty());
    }
}

#[test]
fn five_host_directory_preserves_secret_output_bindings_and_protocol_limits() {
    let d = tempfile::tempdir().unwrap();
    let nodes: Vec<_> = (1..=5).map(|n| json!({"uid":format!("host-{n}"),"hostname":format!("host-{n}"),"address":format!("192.0.2.{n}")})).collect();
    let manifest = json!({
        "uid":"community-five-hosts", "name":"Five-host output distribution",
        "inventory":{"nodes":nodes,"groups":{"control":["host-1","host-2","host-3"],"worker":["host-4","host-5"]}},
        "targets":{"default":{"node_group_uid":"hosts","hosts":"all"}},
        "playbook":{"api_version":"converyn/v1","name":"community","steps":[
            {"id":"produce","phase":"prepare","scope":"per_node","hosts":"control[0]","outputs":{"credential":{"type":"string","secret":true,"max_bytes":1024}},"operation":{"kind":"exec","script":"produce.sh","timeout_seconds":30}},
            {"id":"consume","phase":"rollout","scope":"per_node","hosts":"control","inputs":{"/environment/CREDENTIAL":{"step":"produce","name":"credential","node":"control[0]"}},"operation":{"kind":"exec","script":"consume.sh","timeout_seconds":30}}
        ]}
    });
    std::fs::write(
        d.path().join("workflow.yaml"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(d.path().join("produce.sh"), "printf ready\n").unwrap();
    std::fs::write(d.path().join("consume.sh"), "test -n \"$CREDENTIAL\"\n").unwrap();
    let revision = package(d.path()).unwrap();
    let inventory = revision.inventory.clone().unwrap();
    let uid = revision.uid.clone();
    let sha = digest(&revision);
    let mut state = enrolled_state(&inventory);
    let registry = converyn_runtime::plugins::registry();
    state
        .workflow_change(
            Change::Register {
                workflow: Box::new(revision),
                expected_revision: 0,
            },
            registry,
        )
        .unwrap();
    let (plan, _, _) = state
        .workflow_plan(&uid, &sha, "default", &BTreeMap::new(), registry)
        .unwrap();
    plan.verify_with(registry).unwrap();
    assert!(
        serde_json::to_vec(&plan).unwrap().len() <= converyn_core::model::domain::MAX_PLAN_BYTES
    );
    let consumers: Vec<_> = plan
        .work
        .iter()
        .filter(|w| w.step_id == "consume")
        .collect();
    assert_eq!(consumers.len(), 3);
    assert!(
        consumers
            .iter()
            .all(|w| w.inputs["/environment/CREDENTIAL"].node.as_deref() == Some("host-1"))
    );
}

fn register(s: &mut State, source: Revision) -> Plan {
    let uid = source.uid.clone();
    let sha = digest(&source);
    let reply = s.apply(Command::Workflow {
        change: Change::Register {
            workflow: Box::new(source),
            expected_revision: 0,
        },
    });
    assert!(reply.ok, "{}", reply.code);
    s.workflow_plan(
        &uid,
        &sha,
        "qa",
        &BTreeMap::new(),
        &converyn_plugin_api::NoPlugins,
    )
    .unwrap()
    .0
}
#[test]
fn ordered_groups_and_invalid_selections() {
    let i = inventory();
    i.validate().unwrap();
    assert_eq!(i.select("database").unwrap(), ["c", "b", "a"]);
    assert_eq!(i.select("database[0]").unwrap(), ["c"]);
    assert_eq!(i.select("database[-1]").unwrap(), ["a"]);
    assert_eq!(i.select("database[0-1]").unwrap(), ["c", "b"]);
    assert_eq!(i.select("all").unwrap(), ["b", "a", "c"]);
    for x in [
        "missing",
        "database[3]",
        "database[2-1]",
        "database[-2]",
        "empty[0]",
        "database[0]suffix",
        "database[+1]",
        "database[0-+1]",
    ] {
        assert!(i.select(x).is_err(), "{x}");
    }
    let mut bad = i.clone();
    bad.groups
        .insert("duplicate".into(), vec!["a".into(), "a".into()]);
    assert!(bad.validate().is_err());
}

#[test]
fn node_defaults_and_run_overrides_do_not_replace_inventory_identity() {
    use sha2::{Digest, Sha256};
    let mut s = state();
    let mut source = source("values");
    let script = "echo $VERSION\n";
    let sha = hex::encode(Sha256::digest(script));
    source.definitions.insert(sha.clone(), script.into());
    source.playbook.steps[0].operation =
        serde_json::from_value(json!({"kind":"exec","script_sha256":sha,"timeout_seconds":5}))
            .unwrap();
    source.values.insert("VERSION".into(), json!("global"));
    source
        .inventory
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|n| n.uid == "c")
        .unwrap()
        .values
        .insert("VERSION".into(), json!("inventory"));
    source.node_values.insert(
        "c".into(),
        serde_json::from_value(json!({"VERSION":"explicit"})).unwrap(),
    );
    let plan = register(&mut s, source);
    let value = |plan: &Plan| match &plan.work[0].operation {
        converyn_core::model::domain::Operation::Exec { environment, .. } => {
            serde_json::to_value(&environment["VERSION"]).unwrap()
        }
        _ => panic!(),
    };
    assert_eq!(value(&plan), json!("explicit"));
    let (run, _, _) = s
        .workflow_plan(
            "values",
            &s.workflows.workflows["values"].current,
            "qa",
            &BTreeMap::from([("VERSION".into(), json!("run"))]),
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    assert_eq!(value(&run), json!("run"));
    assert_eq!(run.input.inventory, plan.input.inventory);
}

#[test]
fn fixed_step_options_cannot_be_overridden_by_values_or_output_bindings() {
    use sha2::{Digest, Sha256};
    let mut s = state();
    let mut revision = source("fixed-options");
    let script = "echo $ENTRY:$VERSION\n";
    let sha = hex::encode(Sha256::digest(script));
    revision.definitions.insert(sha.clone(), script.into());
    revision.playbook.steps[0].operation=serde_json::from_value(json!({"kind":"exec","script_sha256":sha,"timeout_seconds":5,"fixed_environment":{"ENTRY":"check"}})).unwrap();
    revision.values.insert("VERSION".into(), json!("one"));
    let mut invalid = revision.clone();
    invalid.playbook.steps[0].inputs.insert(
        "/environment/ENTRY".into(),
        serde_json::from_value(json!({"step":"produce","name":"entry"})).unwrap(),
    );
    assert!(invalid.validate().is_err());
    let plan = register(&mut s, revision);
    let get = |plan: &Plan| match &plan.work[0].operation {
        converyn_core::model::domain::Operation::Exec {
            environment,
            fixed_environment,
            ..
        } => {
            assert!(fixed_environment.is_empty());
            serde_json::to_value(environment).unwrap()
        }
        _ => panic!(),
    };
    assert_eq!(get(&plan)["ENTRY"], json!("check"));
    let sha = s.workflows.workflows["fixed-options"].current.clone();
    assert!(
        s.workflow_plan(
            "fixed-options",
            &sha,
            "qa",
            &BTreeMap::from([("ENTRY".into(), json!("other"))]),
            &converyn_plugin_api::NoPlugins
        )
        .is_err()
    );
    let (changed, _, _) = s
        .workflow_plan(
            "fixed-options",
            &sha,
            "qa",
            &BTreeMap::from([("VERSION".into(), json!("two"))]),
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    assert_eq!(get(&changed)["ENTRY"], json!("check"));
    assert_eq!(get(&changed)["VERSION"], json!("two"));
}
#[test]
fn workflow_labels_do_not_write_global_membership_and_serial_claims_preserve_order() {
    let mut s = state();
    assert!(s.apply(Command::Administer{change:serde_json::from_value(json!({"action":"create_node_group","expected_revision":0,"group":{"uid":"fleet","name":"fleet","revision":1,"node_uids":["c"]}})).unwrap()}).ok);
    let plan = register(&mut s, source("database"));
    assert_eq!(
        plan.work
            .iter()
            .map(|w| w.node_uid.as_str())
            .collect::<Vec<_>>(),
        ["c", "b", "a"]
    );
    let original = s.node_groups.clone();
    assert!(
        s.apply(Command::CreateRun {
            output_key: None,
            run_id: "run".into(),
            idempotency_key: "run".into(),
            plan: Box::new(plan.clone())
        })
        .ok
    );
    assert_eq!(s.node_groups, original);
    assert!(s.node_groups["fleet"].node_uids.contains("c"));
    let claim = |s: &mut State, index: usize| {
        s.apply(Command::Claim {
            input_sha256: None,
            proof: None,
            authorized_ms: Some(1),
            run_id: "run".into(),
            work_id: plan.work[index].id.clone(),
            node_uid: plan.work[index].node_uid.clone(),
            grant: format!("grant-{index}"),
            boot_id: "boot".into(),
            session_id: "session".into(),
        })
    };
    assert_eq!(claim(&mut s, 1).code, "step_order_not_ready");
    assert!(claim(&mut s, 0).ok);
    let other = register(&mut s, source("monitoring"));
    let reply = s.apply(Command::CreateRun {
        output_key: None,
        run_id: "other".into(),
        idempotency_key: "other".into(),
        plan: Box::new(other),
    });
    assert_eq!(reply.code, "node_busy");
    let restored: State = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    assert_eq!(restored.runs["run"].plan, s.runs["run"].plan);
}

#[test]
fn parallel_groups_authorize_in_order_and_disjoint_targets_do_not_block_each_other() {
    let mut s = state();
    let mut revision = source("parallel");
    revision.playbook.steps[0].max_parallel = Some(2);
    let plan = register(&mut s, revision);
    assert!(
        s.apply(Command::CreateRun {
            output_key: None,
            run_id: "parallel".into(),
            idempotency_key: "parallel".into(),
            plan: Box::new(plan.clone())
        })
        .ok
    );
    let claim = |s: &mut State, index: usize| {
        s.apply(Command::Claim {
            input_sha256: None,
            proof: None,
            authorized_ms: Some(1),
            run_id: "parallel".into(),
            work_id: plan.work[index].id.clone(),
            node_uid: plan.work[index].node_uid.clone(),
            grant: format!("grant-{index}"),
            boot_id: "boot".into(),
            session_id: "session".into(),
        })
    };
    assert_eq!(claim(&mut s, 1).code, "step_order_not_ready");
    assert!(claim(&mut s, 0).ok);
    assert_eq!(claim(&mut s, 2).code, "step_order_not_ready");
    assert!(claim(&mut s, 1).ok);
    assert_eq!(claim(&mut s, 2).code, "step_parallel_limit");
    let mut s = state();
    for (name, selector) in [("first", "database[0]"), ("second", "database[1]")] {
        let mut source = source(name);
        source.targets.get_mut("qa").unwrap().hosts = Some(selector.into());
        let plan = register(&mut s, source);
        assert_eq!(plan.work.len(), 1);
        let reply = s.apply(Command::CreateRun {
            output_key: None,
            run_id: name.into(),
            idempotency_key: name.into(),
            plan: Box::new(plan),
        });
        assert!(reply.ok, "{}", reply.code);
    }
    assert_eq!(s.runs.len(), 2);
}

#[test]
fn ordered_inventory_preserves_batch_hooks_on_a_different_node() {
    let mut s = state();
    let mut source = source("batches");
    source.batch_size = 1;
    source
        .inventory
        .as_mut()
        .unwrap()
        .groups
        .insert("workers".into(), vec!["b".into(), "c".into()]);
    source
        .inventory
        .as_mut()
        .unwrap()
        .groups
        .insert("hook".into(), vec!["a".into()]);
    source.playbook.steps[0].hosts = Some("workers".into());
    let mut hook = source.playbook.steps[0].clone();
    hook.id = "hook".into();
    hook.hosts = Some("hook".into());
    hook.scope = converyn_core::model::domain::Scope::OncePerBatch;
    source.playbook.steps.insert(0, hook);
    let plan = register(&mut s, source);
    assert_eq!(
        plan.work
            .iter()
            .map(|w| (w.step_id.as_str(), w.node_uid.as_str(), w.batch))
            .collect::<Vec<_>>(),
        [
            ("hook", "a", Some(0)),
            ("check", "b", Some(0)),
            ("hook", "a", Some(1)),
            ("check", "c", Some(1))
        ]
    );
}

#[test]
fn workflow_artifacts_are_admitted_without_creating_global_membership() {
    use converyn_core::orchestration::preparation::Change as Preparation;
    let mut s = state();
    let mut source = source("artifacts");
    source.playbook.steps[0].hosts = Some("database[0]".into());
    source.playbook.steps[0].operation=serde_json::from_value(json!({"kind":"binary","path":"/opt/example/tool","mode":493,"artifact":{"sha256":"a".repeat(64),"size":10,"locations":[{"source":"release","path":"/tool"}]}})).unwrap();
    let plan = register(&mut s, source);
    let work = plan.work[0].clone();
    assert!(
        s.apply(Command::CreateRun {
            output_key: None,
            run_id: "artifacts".into(),
            idempotency_key: "artifacts".into(),
            plan: Box::new(plan)
        })
        .ok
    );
    s.prepare(Preparation::Prepared {
        run_id: "artifacts".into(),
        work_id: work.id,
        node_uid: work.node_uid,
        sha256: "a".repeat(64),
        size: 10,
        proof: None,
    })
    .unwrap();
    s.prepare(Preparation::Admit {
        run_id: "artifacts".into(),
        expected_revision: s.runs["artifacts"].revision,
    })
    .unwrap();
    assert!(!s.runs["artifacts"].preparing);
    assert!(s.node_groups.len() == 2);

    assert!(s.execution_locks.contains_key("node/c"));
}
#[test]
fn empty_group_must_be_explicit_and_unknown_nodes_are_rejected() {
    let mut s = state();
    let mut w = source("empty");
    w.playbook.steps[0].hosts = Some("empty".into());
    assert!(w.validate().is_err());
    w.playbook.steps[0].allow_empty = true;
    w.validate().unwrap();
    w.inventory.as_mut().unwrap().nodes[0].uid = "unknown".into();
    assert!(w.validate().is_err());
    let mut w = source("retired");
    s.registry.nodes.remove("a");
    let r = s.apply(Command::Workflow {
        change: Change::Register {
            workflow: Box::new(w.clone()),
            expected_revision: 0,
        },
    });
    assert!(!r.ok);
    w.playbook.steps[0].hosts = Some("database[0]".into());
}
#[test]
fn directory_packages_relative_dependencies_without_external_generators() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path();
    std::fs::create_dir(p.join("lib")).unwrap();
    std::fs::write(p.join("lib/helper.sh"), "printf helper\\n").unwrap();
    std::fs::write(
        p.join("check.sh"),
        "source \"$CONVERYN_PACKAGE_ROOT/lib/helper.sh\"\n",
    )
    .unwrap();
    std::fs::write(
        p.join("inventory.yaml"),
        serde_yaml::to_string(&inventory()).unwrap(),
    )
    .unwrap();
    std::fs::write(p.join("values.yaml"), "MESSAGE: ready\n").unwrap();
    std::fs::write(p.join("workflow.yaml"),"uid: directory\nname: Directory workflow\ninventory: inventory.yaml\nvalues: values.yaml\npackage:\n  include: [lib]\ntargets:\n  qa: {hosts: all, node_group_uid: hosts}\nscripts:\n  check: check.sh\nplaybook:\n  api_version: converyn/v1\n  name: directory\n  steps:\n  - id: check\n    phase: rollout\n    scope: per_node\n    hosts: database[0]\n    operation: {kind: exec, timeout_seconds: 10}\n").unwrap();
    let revision = package(p).unwrap();
    assert_eq!(revision.bundle.len(), 1);
    assert_eq!(revision.values["MESSAGE"], json!("ready"));
    assert_eq!(revision.batch_size, 3);
    let before = digest(&revision);
    let mut s = state();
    let plan = register(&mut s, revision);
    assert_eq!(plan.work[0].node_uid, "c");
    std::fs::remove_dir_all(p.join("lib")).unwrap();
    assert!(!s.workflows.revisions[&before].definitions.is_empty());
}

#[test]
fn declarative_references_and_expansions_are_generic_and_deterministic() {
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join("workflow.yaml");
    let manifest = json!({"uid":"checks","name":"Endpoint checks","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"artifacts":{"checks":[{"url":"https://first.invalid/","status":200},{"url":"https://second.invalid/","status":204}]},"playbook":{"api_version":"converyn/v1","name":"checks","steps":[{"id":"probe","for_each":"/artifacts/checks","phase":"verify","scope":"per_node","operation":{"kind":"http_check","url":{"$item":"/url"},"expected_status":{"$item":"/status"},"timeout_seconds":5}}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}});
    std::fs::write(&file, manifest.to_string()).unwrap();
    let revision = package(&file).unwrap();
    assert_eq!(
        revision
            .playbook
            .steps
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["probe-0", "probe-1"]
    );
    assert_eq!(digest(&revision), digest(&package(&file).unwrap()));
    let mut bad = manifest.clone();
    bad["artifacts"]["checks"] = json!([]);
    std::fs::write(&file, bad.to_string()).unwrap();
    assert!(package(&file).is_err());
    let mut bad = manifest;
    bad["playbook"]["steps"][0]["operation"]["url"] = json!({"$ref":"/missing"});
    std::fs::write(&file, bad.to_string()).unwrap();
    assert!(package(&file).is_err());
}

#[test]
fn schema_payload_references_are_data_and_literal_values_are_not_interpreted() {
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join("workflow.yaml");
    let mut manifest = json!({"uid":"schema-data","name":"Schema data","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"artifacts":{"document":{"$ref":"#/definitions/message"}},"files":{"write":{"json":"/artifacts/document"}},"playbook":{"api_version":"converyn/v1","name":"schema-data","steps":[{"id":"write","phase":"rollout","scope":"per_node","operation":{"kind":"file","path":"/opt/example/schema.json","mode":420}}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}});
    for document in [
        json!({"$ref":"#/definitions/message"}),
        json!({"$literal":{"$ref":"/runtime/schema"}}),
    ] {
        manifest["artifacts"]["document"] = document.clone();
        std::fs::write(&file, manifest.to_string()).unwrap();
        let revision = package(&file).unwrap();
        let hash = revision.playbook.steps[0]
            .operation
            .definition_sha()
            .unwrap();
        let actual: serde_json::Value = serde_json::from_str(&revision.definitions[hash]).unwrap();
        assert_eq!(
            actual,
            document.get("$literal").unwrap_or(&document).clone()
        );
    }
}

#[test]
fn directory_rejects_source_escape_and_package_symlinks() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("workflow");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(d.path().join("outside.sh"), "echo outside").unwrap();
    let mut manifest = json!({"uid":"confined","name":"Confined","targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"playbook":{"api_version":"converyn/v1","name":"confined","steps":[{"id":"run","phase":"rollout","scope":"per_node","operation":{"kind":"exec","script":"../outside.sh","timeout_seconds":5}}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}});
    let file = root.join("workflow.yaml");
    std::fs::write(&file, manifest.to_string()).unwrap();
    assert!(package(&root).is_err());
    std::os::unix::fs::symlink(d.path().join("outside.sh"), root.join("linked.sh")).unwrap();
    manifest["playbook"]["steps"][0]["operation"]["script"] = json!("linked.sh");
    std::fs::write(&file, manifest.to_string()).unwrap();
    assert!(package(&root).is_err());
    manifest["playbook"]["steps"][0]["operation"]
        .as_object_mut()
        .unwrap()
        .remove("script");
    manifest["playbook"]["steps"][0]["operation"]["content"] = json!("echo safe");
    manifest["package"] = json!({"include":["linked.sh"]});
    std::fs::write(&file, manifest.to_string()).unwrap();
    assert!(package(&root).is_err());
}
