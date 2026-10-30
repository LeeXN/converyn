use converyn_plugin_api::Plugin;
use converyn_plugin_kubeadm::Kubeadm;
use serde_json::json;
fn input() -> serde_json::Value {
    json!({"version":"v1.36.4","execution":"host","timeout_seconds":600,"environment":{"CA_FILE":"/etc/converyn/kubeadm/ca.crt","KUBECONFIG":"/etc/kubernetes/admin.conf","NODE_NAME":"node1","CA_SHA256":"a".repeat(64),"BINARY_SHA256":"b".repeat(64),"CONFIG_PATH":"/etc/converyn/kubeadm/init.yaml","CONFIG_SHA256":"c".repeat(64),"REFERENCE_KUBECONFIG":"/etc/converyn/kubeadm/reference.conf"}})
}
#[test]
fn all_actions_expand_to_fixed_scripts_and_never_native_lifecycle_calls() {
    for action in Kubeadm.descriptor().actions {
        let mut input = input();
        if action.starts_with("upgrade") {
            input["version"] = "v1.37.0".into();
        }
        let expanded = Kubeadm.expand(&action, input).unwrap();
        assert_eq!(expanded.steps.len(), 1);
        let op = &expanded.steps[0]["operation"];
        assert_eq!(op["kind"], "exec");
        assert_eq!(op["resources"], json!(["kubernetes-host"]));
        assert_eq!(op["origin"]["plugin"]["sha256"], expanded.plugin.sha256);
        let sha = op["script_sha256"].as_str().unwrap();
        assert!(Kubeadm.contains_script(&action, sha));
        assert!(expanded.definitions.contains_key(sha));
        assert_eq!(
            op["environment"]["VERSION"],
            if action.starts_with("upgrade") {
                "v1.37.0"
            } else {
                "v1.36.4"
            }
        );
    }
}
#[test]
fn unverified_versions_and_missing_transaction_inputs_are_rejected() {
    let mut i = input();
    i["version"] = "v1.30.0".into();
    assert!(Kubeadm.expand("init", i).is_err());
    let mut i = input();
    i["environment"]
        .as_object_mut()
        .unwrap()
        .remove("CONFIG_SHA256");
    assert!(Kubeadm.expand("init", i).is_err());
    let mut i = input();
    i["version"] = "v1.37.0".into();
    assert!(Kubeadm.expand("init", i).is_err());
    assert!(Kubeadm.compile("init", input()).is_err());
}
#[test]
fn scripts_preserve_unknown_transaction_and_eviction_boundaries() {
    let d = Kubeadm.expand("drain", input()).unwrap();
    let script = d.definitions.values().next().unwrap();
    assert!(script.contains("resourceVersion"));
    assert!(script.contains("api drain"));
    assert!(!script.contains("--disable-eviction"));
    assert!(!script.contains("--delete-emptydir-data"));
    let init = Kubeadm.expand("init", input()).unwrap();
    assert!(
        init.definitions
            .values()
            .next()
            .unwrap()
            .contains("test ! -e /var/lib/etcd/member")
    );
}

#[test]
fn readiness_waits_are_bounded_and_only_repeat_observation() {
    let e = Kubeadm.expand("check", input()).unwrap();
    let script = e.definitions.values().next().unwrap();
    assert!(script.contains("until ("));
    assert!(script.contains("deadline="));
    assert!(!script.contains("upgrade apply"));
    assert_eq!(
        e.steps[0]["operation"]["environment"]["CHECK_TIMEOUT_SECONDS"],
        "300"
    );
}

#[test]
fn ready_api_does_not_hide_an_unready_node() {
    let script = format!(
        "VERSION=v1.37.0\nCHECK_TIMEOUT_SECONDS=0\nverify_node() {{ return 0; }}\nverify_ready() {{ return 1; }}\napi() {{ printf '%s' '{{\"gitVersion\":\"v1.37.0\"}}'; }}\n{}",
        include_str!("../resources/check.sh")
    );
    let r = std::process::Command::new("/bin/bash")
        .arg("-c")
        .arg(script)
        .output()
        .unwrap();
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("bounded deadline"));
}

#[test]
fn node_registration_retries_observation_and_preserves_validation_failure() {
    // Exercise the production function without requiring a Kubernetes API or
    // host DMI access. Only the external observation/identity validator are faked.
    let common = include_str!("../resources/common.sh");
    let registration = common
        .split_once("verify_node() {")
        .unwrap()
        .1
        .split_once("verify_ready() {")
        .unwrap()
        .0;
    let marker =
        std::env::temp_dir().join(format!("converyn-node-registration-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let script = format!(
        r#"set -eu
NODE_NAME=node1
node_json() {{
  if ! test -e "$MARKER"; then
    : > "$MARKER"
    printf 'NotFound\n' >&2
    return 1
  fi
  printf '%s' '{{"metadata":{{"uid":"observed-node"}}}}'
}}
sleep() {{ :; }}
python3() {{ cat; return "$VALIDATION_STATUS"; }}
verify_node() {{{registration}
verify_node
"#
    );
    for (validation_status, success) in [(0, true), (7, false)] {
        let _ = std::fs::remove_file(&marker);
        let result = std::process::Command::new("/bin/bash")
            .args(["-c", &script])
            .env("MARKER", &marker)
            .env("VALIDATION_STATUS", validation_status.to_string())
            .output()
            .unwrap();
        assert_eq!(result.status.success(), success);
        assert!(String::from_utf8_lossy(&result.stdout).contains("observed-node"));
        assert!(marker.exists());
    }
    std::fs::remove_file(marker).unwrap();
}
