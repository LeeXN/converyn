use converyn_core::{
    model::telemetry::{INVENTORY_LIMIT, Snapshot},
    orchestration::machine::{Command, State},
};
use serde_json::json;
#[test]
fn legacy_replay_optional_wire_fields_merge_and_size_rejection() {
    let old = json!({"type":"observe_node","capabilities":[],"hardware_sha256":null,"node_uid":"node","boot_id":"boot","session_id":"session","observed_ms":10});
    let command: Command = serde_json::from_value(old.clone()).unwrap();
    let encoded = serde_json::to_value(command.clone()).unwrap();
    assert_eq!(encoded, old);
    let mut state = State::default();
    assert!(state.apply(command).ok);
    let saved = serde_json::to_vec(&state).unwrap();
    let mut replay: State = serde_json::from_slice(&saved).unwrap();
    assert_eq!(replay.observations["node"].inventory, None);
    let mut fresh = old.clone();
    fresh["inventory"] =
        json!({"collected_ms":20,"data":{"serial":"asset-only","hostname":"test"}});
    fresh["inventory_checked_ms"] = json!(20);
    assert!(replay.apply(serde_json::from_value(fresh).unwrap()).ok);
    assert!(
        replay
            .apply(serde_json::from_value(old.clone()).unwrap())
            .ok
    );
    assert_eq!(
        replay.observations["node"].inventory.as_ref().unwrap().data["serial"],
        "asset-only"
    );
    assert_eq!(replay.observations["node"].hardware_sha256, None);
    let mut too_large = old;
    too_large["inventory"] = json!({"collected_ms":1,"data":{"x":"x".repeat(INVENTORY_LIMIT)}});
    assert_eq!(
        replay
            .apply(serde_json::from_value(too_large).unwrap())
            .code,
        "observation_size_limit"
    );
    assert!(Snapshot::default().valid(INVENTORY_LIMIT));
}

#[test]
fn usage_changes_do_not_change_static_assets() {
    let root = tempfile::tempdir().unwrap();
    let proc = root.path().join("proc");
    std::fs::create_dir_all(&proc).unwrap();
    std::fs::write(
        proc.join("meminfo"),
        "MemTotal: 1024 kB\nMemAvailable: 512 kB\n",
    )
    .unwrap();
    let collector = converyn_runtime::nodes::inventory::Collector::new(root.path().into());
    let before = collector.inventory();
    std::fs::write(
        proc.join("meminfo"),
        "MemTotal: 1024 kB\nMemAvailable: 256 kB\n",
    )
    .unwrap();
    let after = collector.inventory();
    assert_eq!(before.data, after.data);
    assert!(!after.data.contains_key("memory_available_bytes"));
}
