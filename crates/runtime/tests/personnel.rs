use converyn_core::{
    identities::personnel::{self, Change, Credential, Identity, Registry},
    identities::registration::Authorization,
    model::domain::digest,
    orchestration::machine::{Command, State},
};
use converyn_runtime::credentials::pki;

fn credential(
    root: &std::path::Path,
    authority: &pki::Authority,
    id: &str,
    identity: &str,
) -> Credential {
    let signed = authority.issue_personal(&root.join(id), identity).unwrap();
    let info = pki::info(&signed.certificate_pem).unwrap();
    Credential {
        uid: id.into(),
        identity_uid: identity.into(),
        certificate_pem: signed.certificate_pem,
        fingerprint: info.fingerprint,
        token_sha256: digest(&id),
        not_before_ms: info.not_before_ms,
        expires_ms: info.expires_ms,
        revoked_ms: None,
        overlap_until_ms: None,
    }
}
fn identity(id: &str) -> Identity {
    Identity {
        uid: id.into(),
        name: id.into(),

        disabled: false,
        revision: 1,
    }
}
#[test]
fn personal_lifetimes_and_default_immediate_rotation() {
    let root = tempfile::tempdir().unwrap();
    let mut ca = pki::create_authority(&root.path().join("ca"), "test").unwrap();
    ca.leaf_lifetime_seconds = personnel::MAX_LIFETIME;
    let old = credential(root.path(), &ca, "old", "user-alice");
    let now = converyn_runtime::executors::host::now_ms();
    let mut bad = old.clone();
    bad.certificate_pem
        .push_str(&std::fs::read_to_string(root.path().join("old/tls.key")).unwrap());
    assert!(bad.validate(now).is_err());
    let mut registry = Registry::default();
    registry
        .change(Change::Create {
            super_admin: false,
            identity: identity("user-alice"),
            credential: old.clone(),
            request_id: "create".into(),
            now_ms: now,
        })
        .unwrap();
    let new = credential(root.path(), &ca, "new", "user-alice");
    registry
        .change(Change::Issue {
            credential: new.clone(),
            expected_revision: 1,
            rotate: true,
            overlap_seconds: 0,
            request_id: "rotate".into(),
            now_ms: now,
        })
        .unwrap();
    assert!(
        registry
            .authenticate(&old.fingerprint, &old.token_sha256, now)
            .is_none()
    );
    assert!(
        registry
            .authenticate(&new.fingerprint, &new.token_sha256, now)
            .is_some()
    );
    assert!(
        registry
            .authenticate(&old.fingerprint, &new.token_sha256, now)
            .is_none()
    );
    assert!(
        registry
            .authenticate(&new.fingerprint, &old.token_sha256, now)
            .is_none()
    );
    assert!(
        registry
            .authenticate(&new.fingerprint, &new.token_sha256, new.expires_ms)
            .is_none()
    );
    ca.leaf_lifetime_seconds = personnel::MAX_LIFETIME + 1;
    assert!(
        ca.issue_personal(&root.path().join("too-long"), "user-bob")
            .is_err()
    );
}
#[test]
fn overlap_expires_without_cleanup_and_never_extends_old_windows() {
    let root = tempfile::tempdir().unwrap();
    let mut ca = pki::create_authority(&root.path().join("ca"), "test").unwrap();
    ca.leaf_lifetime_seconds = personnel::DEFAULT_LIFETIME;
    let old = credential(root.path(), &ca, "first", "user-alice");
    let mut registry = Registry::default();
    let now = converyn_runtime::executors::host::now_ms();
    registry
        .change(Change::Create {
            super_admin: false,
            identity: identity("user-alice"),
            credential: old.clone(),
            request_id: "create".into(),
            now_ms: now,
        })
        .unwrap();
    let new = credential(root.path(), &ca, "second", "user-alice");
    let change = Change::Issue {
        credential: new.clone(),
        expected_revision: 1,
        rotate: true,
        overlap_seconds: 60,
        request_id: "rotate".into(),
        now_ms: now,
    };
    registry.change(change.clone()).unwrap();
    let snapshot = registry.clone();
    registry.change(change).unwrap();
    assert_eq!(registry, snapshot);
    assert!(
        registry
            .authenticate(&old.fingerprint, &old.token_sha256, now + 59999)
            .is_some()
    );
    assert!(
        registry
            .authenticate(&old.fingerprint, &old.token_sha256, now + 60000)
            .is_none()
    );
    let third = credential(root.path(), &ca, "third", "user-alice");
    registry
        .change(Change::Issue {
            credential: third,
            expected_revision: 2,
            rotate: true,
            overlap_seconds: personnel::MAX_OVERLAP,
            request_id: "again".into(),
            now_ms: now + 1000,
        })
        .unwrap();
    assert_eq!(
        registry.credentials["first"].overlap_until_ms,
        Some(now + 60000)
    );
    registry
        .change(Change::Update {
            uid: "user-alice".into(),
            name: "Alice renamed".into(),
            disabled: true,
            expected_revision: 3,
        })
        .unwrap();
    assert!(
        registry
            .authenticate(&new.fingerprint, &new.token_sha256, now + 2000)
            .is_none()
    );
}
#[test]
fn executor_cannot_inject_plans_or_control_others_and_disabled_identity_fails_commit() {
    let root = tempfile::tempdir().unwrap();
    let ca = pki::create_authority(&root.path().join("ca"), "test").unwrap();
    let now = converyn_runtime::executors::host::now_ms();
    let c = credential(root.path(), &ca, "alice", "user-alice");
    let mut state = State::default();
    state
        .personnel
        .change(Change::Create {
            super_admin: false,
            identity: identity("user-alice"),
            credential: c.clone(),
            request_id: "create".into(),
            now_ms: now,
        })
        .unwrap();
    let auth = Authorization {
        request_id: None,
        observed_ms: Some(now),
        token_sha256: Some(c.token_sha256),
        principal: "user-alice".into(),
        fingerprint: c.fingerprint,
    };
    assert!(!state.access_allows(
        &auth,
        &Command::Cancel {
            run_id: "someone-else".into()
        }
    ));
    assert!(!state.access_allows(
        &auth,
        &Command::PutScript {
            sha256: "a".repeat(64),
            content: "echo bad".into()
        }
    ));
    state
        .personnel
        .identities
        .get_mut("user-alice")
        .unwrap()
        .disabled = true;
    let r = state.apply(Command::Authorized {
        authorization: auth,
        command: Box::new(Command::Cancel {
            run_id: "someone-else".into(),
        }),
    });
    assert!(!r.ok);
}

#[test]
fn transport_identities_cannot_submit_human_workflows_or_manage_users() {
    let state = State::default();
    for principal in ["node:node-a", "unknown", "user-unregistered"] {
        let auth = Authorization {
            request_id: None,
            observed_ms: Some(1),
            token_sha256: Some("a".repeat(64)),
            principal: principal.into(),
            fingerprint: "b".repeat(64),
        };
        assert!(!state.access_allows(
            &auth,
            &Command::PutScript {
                sha256: "c".repeat(64),
                content: "echo wrong role".into()
            }
        ));
        assert!(!state.access_allows(
            &auth,
            &Command::Cancel {
                run_id: "another-task".into()
            }
        ));
    }
}
