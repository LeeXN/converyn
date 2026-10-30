use converyn_runtime::artifacts::source::{ArtifactRef, Location, Source};
#[test]
fn source_credentials_cannot_be_redirected_by_plan_paths() {
    let source = Source {
        origin: "https://example.com:8443".into(),
        ca_file: None,
        bearer_token_file: None,
    };
    for path in [
        "//evil.example/object",
        "https://evil.example/a",
        "/../secret",
        "/%2fsecret",
        "/a?token=x",
        "/a#b",
        "/a\\b",
        "/a\nb",
    ] {
        assert!(source.client_and_url(path).is_err(), "{path}");
    }
    assert_eq!(
        source.client_and_url("/objects/app-v1").unwrap().1,
        "https://example.com:8443/objects/app-v1"
    );
    let bad = Source {
        origin: "https://user:secret@example.com".into(),
        ca_file: None,
        bearer_token_file: None,
    };
    assert!(bad.client_and_url("/a").is_err());
}
#[test]
fn immutable_artifact_limits_and_source_count_are_enforced() {
    let mut artifact = ArtifactRef {
        sha256: "a".repeat(64),
        size: 10 * 1024 * 1024 * 1024,
        locations: vec![Location {
            source: "private".into(),
            path: "/artifact".into(),
        }],
    };
    assert!(artifact.validate().is_ok());
    artifact.size += 1;
    assert!(artifact.validate().is_err());
    artifact.size = 1;
    artifact.locations.clear();
    assert!(artifact.validate().is_ok());
}
#[test]
fn cache_eviction_keeps_locked_payload_and_admits_only_with_capacity() {
    use fs2::FileExt;
    let dir = tempfile::tempdir().unwrap();
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let wanted = "c".repeat(64);
    std::fs::write(dir.path().join(&a), [1u8; 100]).unwrap();
    std::fs::write(dir.path().join(&b), [2u8; 100]).unwrap();
    let lock = std::fs::File::create(dir.path().join(format!("{a}.lock"))).unwrap();
    lock.lock_exclusive().unwrap();
    converyn_runtime::artifacts::cache::reserve(dir.path(), &wanted, 100, 200).unwrap();
    assert!(dir.path().join(&a).exists());
    assert!(!dir.path().join(&b).exists());
    assert!(converyn_runtime::artifacts::cache::reserve(dir.path(), &wanted, 150, 200).is_err());
    assert!(dir.path().join(&a).exists());
}
