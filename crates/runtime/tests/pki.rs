use converyn_runtime::credentials::pki;

#[test]
fn configured_lifetime_covers_bundle_leaves_and_node_renewal() {
    let d = tempfile::tempdir().unwrap();
    let bundle = d.path().join("bundle");
    let lifetime = 90 * 86400;
    let before = converyn_runtime::executors::host::now_ms();
    converyn_runtime::controller::initialization::create_with_lifetime(
        &bundle,
        "lifetime-test",
        &[
            "1=127.0.0.1:8443".into(),
            "2=127.0.0.1:8444".into(),
            "3=127.0.0.1:8445".into(),
        ],
        "team",
        "hosts",
        &[],
        lifetime,
    )
    .unwrap();
    let check = |pem: &str| {
        let info = pki::info(pem).unwrap();
        assert!(info.expires_ms >= before + lifetime * 1000 - 1000);
        assert!(info.expires_ms <= converyn_runtime::executors::host::now_ms() + lifetime * 1000);
    };
    for name in ["admin", "controller-1", "controller-2", "controller-3"] {
        check(&std::fs::read_to_string(bundle.join(name).join("tls.crt")).unwrap());
    }
    for id in 1..=3 {
        let profile: converyn_runtime::controller::initialization::ServerProfile =
            serde_json::from_slice(
                &std::fs::read(bundle.join(format!("controller-{id}/server.json"))).unwrap(),
            )
            .unwrap();
        assert_eq!(profile.issuer.unwrap().leaf_lifetime_seconds, lifetime);
    }
    let issuer = pki::Authority {
        cert: bundle.join("node-authority/ca.crt"),
        key: bundle.join("node-authority/ca.key"),
        leaf_lifetime_seconds: lifetime,
    };
    let (_, csr) = pki::request("node-a").unwrap();
    check(
        &issuer
            .sign_client("node-a", &csr, lifetime)
            .unwrap()
            .certificate_pem,
    );
    check(
        &issuer
            .issue(&d.path().join("operator"), "user-operator", &[])
            .unwrap()
            .certificate_pem,
    );
}

#[test]
fn invalid_lifetimes_leave_no_partial_bundle_or_identity() {
    let d = tempfile::tempdir().unwrap();
    let mut authority = pki::create_authority(&d.path().join("authority"), "test-ca").unwrap();
    assert_eq!(authority.leaf_lifetime_seconds, 30 * 86400);
    for lifetime in [0, 299, 365 * 86400 + 1] {
        let bundle = d.path().join(format!("bundle-{lifetime}"));
        assert!(
            converyn_runtime::controller::initialization::create_with_lifetime(
                &bundle,
                "lifetime-test",
                &["1=127.0.0.1:8443".into()],
                "team",
                "hosts",
                &[],
                lifetime,
            )
            .is_err()
        );
        assert!(!bundle.exists());
        authority.leaf_lifetime_seconds = lifetime;
        let identity = d.path().join(format!("identity-{lifetime}"));
        assert!(authority.issue(&identity, "user-a", &[]).is_err());
        assert!(!identity.exists());
    }
    for lifetime in [300, 365 * 86400] {
        pki::validate_leaf_lifetime(lifetime).unwrap();
    }
}

#[test]
fn signed_node_request_cannot_escalate_requested_ca_or_server_privileges() {
    let d = tempfile::tempdir().unwrap();
    let authority = pki::create_authority(&d.path().join("authority"), "test-ca").unwrap();
    authority.validate().unwrap();
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
    let mut params = rcgen::CertificateParams::new(vec!["admin.example".into()]).unwrap();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign];
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let csr = params.serialize_request(&key).unwrap().pem().unwrap();
    let signed = authority.sign_client("node-a", &csr, 3600).unwrap();
    let info = pki::info(&signed.certificate_pem).unwrap();
    assert!(!info.is_ca && !info.can_sign && !info.server_auth && info.client_auth);
    assert_eq!(info.public_key, key.public_key_raw());
    assert_eq!(info.fingerprint, signed.fingerprint);
    assert!(signed.expires_ms <= converyn_runtime::executors::host::now_ms() + 3600 * 1000);
}
#[test]
fn issuer_private_key_mismatch_and_identity_overwrite_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    let one = pki::create_authority(&d.path().join("one"), "one").unwrap();
    let two = pki::create_authority(&d.path().join("two"), "two").unwrap();
    let bad = pki::Authority {
        cert: one.cert.clone(),
        key: two.key,
        leaf_lifetime_seconds: 3600,
    };
    assert!(bad.validate().is_err());
    assert!(pki::create_authority(&d.path().join("one"), "one").is_err());
    let signed = one
        .issue(
            &d.path().join("server"),
            "server",
            &["127.0.0.1".parse().unwrap()],
        )
        .unwrap();
    let info = pki::info(&signed.certificate_pem).unwrap();
    assert!(info.server_auth && info.client_auth && !info.is_ca);
}

#[test]
fn online_node_issuer_cannot_create_a_valid_server_identity() {
    let d = tempfile::tempdir().unwrap();
    let root = pki::create_authority(&d.path().join("root"), "transport-root").unwrap();
    let node =
        pki::create_client_authority(&root, &d.path().join("online-node-issuer"), "nodes-only")
            .unwrap();
    let info = pki::info(&std::fs::read_to_string(&node.cert).unwrap()).unwrap();
    assert!(info.is_ca && info.can_sign && info.client_auth && !info.server_auth);
    node.issue(
        &d.path().join("invalid-server"),
        "invalid-server",
        &["127.0.0.1".parse().unwrap()],
    )
    .unwrap();
    let verify = |purpose: &str, file: &std::path::Path| {
        std::process::Command::new("openssl")
            .args(["verify", "-x509_strict", "-purpose", purpose, "-CAfile"])
            .arg(&root.cert)
            .arg("-untrusted")
            .arg(&node.cert)
            .arg(file)
            .output()
            .unwrap()
    };
    assert!(
        !verify("sslserver", &d.path().join("invalid-server/tls.crt"))
            .status
            .success()
    );
    let (_, csr) = pki::request("node").unwrap();
    let signed = node.sign_client("node-a", &csr, 3600).unwrap();
    let path = d.path().join("client.crt");
    std::fs::write(&path, signed.certificate_pem).unwrap();
    assert!(verify("sslclient", &path).status.success());
}
