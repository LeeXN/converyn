use converyn_plugin_api::*;
use serde_json::{Value, json};
struct Counter;
impl Plugin for Counter {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            identity: Identity {
                id: "example.counter".into(),
                version: "1.0.0".into(),
                contract: 1,
                sha256: "a".repeat(64),
            },
            location: Location::Node,
            actions: vec!["ensure".into()],
            description: "test extension outside core".into(),
        }
    }
    fn compile(&self, action: &str, spec: Value) -> anyhow::Result<Call> {
        anyhow::ensure!(
            action == "ensure" && spec["value"].is_u64(),
            "invalid counter intent"
        );
        Ok(Call {
            plugin: self.descriptor().identity,
            action: action.into(),
            location: Location::Node,
            spec,
            resources: vec!["counter/value".into()],
            reconcilable: true,
            timeout_seconds: 60,
        })
    }
    fn apply(&self, _: &Call, _: &Context<'_>) -> anyhow::Result<Applied> {
        unreachable!()
    }
    fn verify(&self, call: &Call, payload: &Value) -> anyhow::Result<bool> {
        Ok(payload == &call.spec)
    }
}
#[test]
fn registration_and_contract_fail_closed() {
    let mut r = Registry::default();
    r.register(Counter).unwrap();
    assert!(r.register(Counter).is_err());
    assert!(r.get("missing").is_err());
    let call = r
        .get("example.counter")
        .unwrap()
        .compile("ensure", json!({"value":2}))
        .unwrap();
    r.validate(&call).unwrap();
    for mutation in 0..4 {
        let mut c = call.clone();
        match mutation {
            0 => c.plugin.sha256 = "b".repeat(64),
            1 => c.resources.clear(),
            2 => c.reconcilable = false,
            _ => c.location = Location::Controller,
        };
        assert!(r.validate(&c).is_err());
    }
}
#[test]
fn observations_bind_the_exact_call_and_do_not_trust_verified_alone() {
    let mut r = Registry::default();
    r.register(Counter).unwrap();
    let call = r
        .get("example.counter")
        .unwrap()
        .compile("ensure", json!({"value":2}))
        .unwrap();
    let mut e = Observation {
        owner: "owner".into(),
        fence: 1,
        plugin: call.plugin.clone(),
        call_sha256: digest(&call),
        payload: call.spec.clone(),
        verified: true,
    };
    assert!(r.verify(&call, &e).unwrap());
    e.payload = json!({"value":3});
    assert!(!r.verify(&call, &e).unwrap());
    e.payload = call.spec.clone();
    e.call_sha256 = "0".repeat(64);
    assert!(!r.verify(&call, &e).unwrap());
}
#[test]
fn limits_and_resource_order_are_canonical() {
    let mut c = Counter.compile("ensure", json!({"value":2})).unwrap();
    c.resources = vec!["b".into(), "a".into()];
    assert!(c.validate().is_err());
    c.resources = vec!["a/../credential".into()];
    assert!(c.validate().is_err());
    c.resources.clear();
    c.timeout_seconds = 0;
    assert!(c.validate().is_err());
}
