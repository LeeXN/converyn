//! Authorization boundaries, including revocation between HTTP and consensus.
use converyn_core::{
    identities::{
        access::{Action, Binding, Change, Group, Role, Subject},
        personnel::{Identity, Registry},
    },
    orchestration::machine::{Command, State},
};
fn person(r: &mut Registry, id: &str) {
    r.identities.insert(
        id.into(),
        Identity {
            uid: id.into(),
            name: id.into(),
            disabled: false,
            revision: 1,
        },
    );
}
fn bind(
    r: &mut Registry,
    id: &str,
    subject: Subject,
    role: Role,
    workflow: Option<&str>,
    presets: &[&str],
) {
    r.access_change(Change::PutBinding {
        expected_revision: r.access.revision,
        binding: Binding {
            uid: id.into(),
            subject,
            role,
            workflow: workflow.map(Into::into),
            presets: presets.iter().map(|p| p.to_string()).collect(),
        },
    })
    .unwrap();
}
#[test]
fn deny_by_default_and_additive_group_preset_grants() {
    let mut r = Registry::default();
    person(&mut r, "user-a");
    assert!(!r.allows("user-a", Action::View, Some("deploy"), Some("prod")));
    r.access_change(Change::PutGroup {
        expected_revision: 0,
        group: Group {
            uid: "team".into(),
            name: "Team".into(),
            members: ["user-a".into()].into(),
        },
    })
    .unwrap();
    bind(
        &mut r,
        "test",
        Subject::Group("team".into()),
        Role::Executor,
        Some("deploy"),
        &["test"],
    );
    assert!(r.allows("user-a", Action::Execute, Some("deploy"), Some("test")));
    for (w, p) in [("deploy", "prod"), ("other", "test")] {
        assert!(!r.allows("user-a", Action::View, Some(w), Some(p)));
    }
    assert!(!r.allows("user-a", Action::Maintain, Some("deploy"), None));
    bind(
        &mut r,
        "prod-view",
        Subject::Person("user-a".into()),
        Role::Viewer,
        Some("deploy"),
        &["prod"],
    );
    assert!(r.allows("user-a", Action::View, Some("deploy"), Some("prod")));
    assert!(!r.allows("user-a", Action::Execute, Some("deploy"), Some("prod")));
    r.access_change(Change::PutGroup {
        expected_revision: r.access.revision,
        group: Group {
            uid: "team".into(),
            name: "Team".into(),
            members: Default::default(),
        },
    })
    .unwrap();
    assert!(!r.allows("user-a", Action::Execute, Some("deploy"), Some("test")));
    assert!(r.allows("user-a", Action::View, Some("deploy"), Some("prod")));
    r.identities.get_mut("user-a").unwrap().disabled = true;
    assert!(!r.allows("user-a", Action::View, Some("deploy"), Some("prod")));
}
#[test]
fn platform_roles_do_not_imply_business_or_access_admin_permissions() {
    let mut r = Registry::default();
    person(&mut r, "user-op");
    bind(
        &mut r,
        "platform",
        Subject::Person("user-op".into()),
        Role::PlatformOperator,
        None,
        &[],
    );
    assert!(r.allows("user-op", Action::Platform, None, None));
    assert!(!r.allows("user-op", Action::Access, None, None));
    assert!(!r.allows("user-op", Action::Execute, Some("deploy"), Some("prod")));
}
#[test]
fn changes_are_atomic_and_cas_guarded_and_preserve_last_super_admin() {
    let mut r = Registry::default();
    person(&mut r, "user-op");
    r.grant_initial("user-op", Role::SuperAdmin);
    let original = r.clone();
    assert!(
        r.access_change(Change::DeleteBinding {
            uid: "initial-user-op".into(),
            expected_revision: 0
        })
        .is_err()
    );
    assert!(
        r.access_change(Change::DeleteBinding {
            uid: "initial-user-op".into(),
            expected_revision: r.access.revision
        })
        .is_err()
    );
    assert_eq!(original, r);
    assert!(
        r.access_change(Change::PutBinding {
            expected_revision: r.access.revision,
            binding: Binding {
                uid: "bad".into(),
                subject: Subject::Person("user-op".into()),
                role: Role::Maintainer,
                workflow: Some("deploy".into()),
                presets: ["prod".into()].into()
            }
        })
        .is_err()
    );
    assert_eq!(original, r);
}
#[test]
fn authorization_is_rechecked_at_commit_and_cannot_escalate() {
    use converyn_core::identities::{
        personnel::{Change as PersonChange, Credential},
        registration::Authorization,
    };
    use converyn_runtime::credentials::pki;
    let tmp = tempfile::tempdir().unwrap();
    let ca = pki::create_authority(&tmp.path().join("ca"), "access").unwrap();
    let now = converyn_runtime::executors::host::now_ms();
    let signed = ca.issue_personal(&tmp.path().join("a"), "user-a").unwrap();
    let info = pki::info(&signed.certificate_pem).unwrap();
    let mut s = State::default();
    s.personnel
        .change(PersonChange::Create {
            super_admin: false,
            identity: Identity {
                uid: "user-a".into(),
                name: "A".into(),
                disabled: false,
                revision: 1,
            },
            credential: Credential {
                uid: "cred-a".into(),
                identity_uid: "user-a".into(),
                certificate_pem: signed.certificate_pem,
                fingerprint: info.fingerprint.clone(),
                token_sha256: converyn_core::model::domain::digest(&"token"),
                not_before_ms: info.not_before_ms,
                expires_ms: info.expires_ms,
                revoked_ms: None,
                overlap_until_ms: None,
            },
            request_id: "create".into(),
            now_ms: now,
        })
        .unwrap();
    let auth = Authorization {
        principal: "user-a".into(),
        fingerprint: info.fingerprint,
        token_sha256: Some(converyn_core::model::domain::digest(&"token")),
        observed_ms: Some(now),
        request_id: None,
    };
    let command = Command::Workflow {
        change: converyn_core::orchestration::workflow::Change::Enable {
            uid: "deploy".into(),
            enabled: false,
            expected_revision: 1,
        },
    };
    bind(
        &mut s.personnel,
        "maintain",
        Subject::Person("user-a".into()),
        Role::Maintainer,
        Some("deploy"),
        &[],
    );
    assert!(s.access_allows(&auth, &command));
    s.personnel
        .access_change(Change::DeleteBinding {
            uid: "maintain".into(),
            expected_revision: s.personnel.access.revision,
        })
        .unwrap();
    assert_eq!(
        s.apply(Command::Authorized {
            authorization: auth.clone(),
            command: Box::new(command)
        })
        .code,
        "access_denied_before_commit"
    );
    bind(
        &mut s.personnel,
        "access",
        Subject::Person("user-a".into()),
        Role::AccessAdmin,
        None,
        &[],
    );
    let escalation = Command::Access {
        change: Change::PutBinding {
            expected_revision: s.personnel.access.revision,
            binding: Binding {
                uid: "self-admin".into(),
                subject: Subject::Person("user-a".into()),
                role: Role::SuperAdmin,
                workflow: None,
                presets: Default::default(),
            },
        },
    };
    assert!(!s.access_allows(&auth, &escalation));
    person(&mut s.personnel, "user-disabled-admin");
    s.personnel
        .grant_initial("user-disabled-admin", Role::SuperAdmin);
    s.personnel
        .identities
        .get_mut("user-disabled-admin")
        .unwrap()
        .disabled = true;
    let enable_admin = Command::Personnel {
        change: PersonChange::Update {
            uid: "user-disabled-admin".into(),
            name: "Disabled admin".into(),
            disabled: false,
            expected_revision: 1,
        },
    };
    assert!(!s.access_allows(&auth, &enable_admin));

    s.personnel.grant_initial("user-a", Role::SuperAdmin);
    // An API caller cannot forge time to bypass the last active credential guard.
    let forged = Command::Personnel {
        change: PersonChange::Revoke {
            uid: "cred-a".into(),
            expected_revision: 1,
            now_ms: 0,
        },
    };
    assert!(!s.access_allows(&auth, &forged));
}
#[test]
fn old_stores_are_rejected_without_rewriting_credentials_or_schema() {
    use rusqlite::Connection;
    let tmp = tempfile::tempdir().unwrap();
    let db = Connection::open(tmp.path().join("state.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value BLOB);")
        .unwrap();
    let bytes = serde_json::to_vec(&22u32).unwrap();
    db.execute("INSERT INTO metadata VALUES('schema_version',?1)", [&bytes])
        .unwrap();
    assert!(converyn_runtime::storage::access_migration::migrate(tmp.path()).is_err());
    let after: Vec<u8> = db
        .query_row(
            "SELECT value FROM metadata WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, bytes);
}
