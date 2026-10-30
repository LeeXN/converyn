use converyn_plugin_api::Plugin;
use converyn_plugin_linux_host::LinuxHost;
use serde_json::json;
#[test]
fn sysctl_declares_runtime_and_persistent_resources() {
    let c = LinuxHost
        .compile(
            "sysctl",
            json!({"key":"vm.swappiness","value":"10","persist":true}),
        )
        .unwrap();
    assert_eq!(
        c.resources,
        vec![
            "file/etc/sysctl.d/90-converyn-vm.swappiness.conf",
            "sysctl/vm.swappiness"
        ]
    );
    assert!(c.reconcilable);
}
#[test]
fn invalid_or_action_sysctls_are_not_state_operations() {
    for key in ["vm/../secret", "kernel.sysrq", "vm.drop_caches"] {
        assert!(
            LinuxHost
                .compile("sysctl", json!({"key":key,"value":"1","persist":true}))
                .is_err()
        );
    }
    assert!(
        LinuxHost
            .compile(
                "sysctl",
                json!({"key":"vm.swappiness","value":"10\nother=1","persist":true})
            )
            .is_err()
    );
}
#[test]
fn kernel_selection_is_explicit_and_not_automatically_reconciled() {
    let c = LinuxHost
        .compile("select_kernel", json!({"release":"6.8.0-142-generic"}))
        .unwrap();
    assert!(!c.reconcilable);
    assert!(
        !LinuxHost
            .verify(&c, &json!({"after":"/boot/vmlinuz-wrong"}))
            .unwrap()
    );
}

#[test]
fn bridge_sysctls_and_bounded_deployment_actions_are_explicit() {
    assert!(
        LinuxHost
            .compile(
                "sysctl",
                json!({"key":"net.bridge.bridge-nf-call-iptables","value":"1","persist":true})
            )
            .is_ok()
    );
    for (action, spec) in [
        (
            "service",
            json!({"name":"containerd","action":"restart","enabled":true,"optional":false,"requested":true}),
        ),
        (
            "service",
            json!({"name":"systemd-journald","action":"restart","enabled":null,"optional":false,"requested":true}),
        ),
        (
            "directories",
            json!({"paths":["/srv/example/lib","/etc/example/service"]}),
        ),
        ("swapoff", json!({})),
        ("modules", json!({"names":["overlay","br_netfilter"]})),
        ("initramfs", json!({"requested":false})),
        (
            "kernel_arguments",
            json!({"method":"grubby","arguments":["intel_iommu=on","iommu=pt"]}),
        ),
    ] {
        let call = LinuxHost.compile(action, spec).unwrap();
        assert!(!call.reconcilable);
        assert!(LinuxHost.authorize(&call, &json!({}), "default").is_err());
    }
    for path in [
        "/",
        "/srv/example/../other",
        "/srv//example",
        "/srv/example/",
    ] {
        assert!(
            LinuxHost
                .compile("directories", json!({"paths":[path]}))
                .is_err()
        );
    }
    assert!(
        LinuxHost
            .compile(
                "service",
                json!({"name":"app;reboot","action":"restart","enabled":true,"optional":false})
            )
            .is_err()
    );
}
#[test]
fn service_activation_preserves_static_units_and_verifies_requested_state() {
    let call=LinuxHost.compile("service",json!({"name":"systemd-journald","action":"restart","enabled":null,"optional":false,"requested":true})).unwrap();
    assert!(LinuxHost.verify(&call,&json!({"name":"systemd-journald","action":"restart","active":"active","enabled":"static"})).unwrap());
    assert!(!LinuxHost.verify(&call,&json!({"name":"systemd-journald","action":"restart","active":"failed","enabled":"static"})).unwrap());
    let skip=LinuxHost.compile("service",json!({"name":"keepalived","action":"restart","enabled":true,"optional":true,"requested":false})).unwrap();
    assert!(LinuxHost.verify(&skip, &json!({"skipped":true})).unwrap());
}

#[test]
fn directory_actions_use_generic_paths_and_exact_local_grants() {
    let call = LinuxHost
        .compile("directories", json!({"paths":["/srv/example/data"]}))
        .unwrap();
    assert!(LinuxHost.authorize(&call, &json!({}), "default").is_err());
    assert!(
        LinuxHost
            .authorize(&call, &json!({"directories":["/srv"]}), "default")
            .is_err()
    );
    let policy = json!({"directories":["/srv/example/data"]});
    assert!(LinuxHost.authorize(&call, &policy, "default").is_ok());
    let paths = LinuxHost.policy_paths(&policy).unwrap();
    assert_eq!(
        paths.writable,
        vec![std::path::PathBuf::from("/srv/example/data")]
    );
}
