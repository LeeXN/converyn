use converyn_runtime::log_store::{Config, Store};
#[tokio::test]
async fn shared_directory_identity_and_immutable_chunks_survive_reopen() {
    let d = tempfile::tempdir().unwrap();
    let c = Config::Filesystem {
        directory: d.path().join("shared"),
        shared: true,
        retention_days: 30,
    };
    let a = Store::open(c.clone()).await.unwrap();
    let b = Store::open(c).await.unwrap();
    assert_eq!(a.id, b.id);
    assert!(a.shared());
    a.put("logs/one/0000000000000000", b"first").await.unwrap();
    b.put("logs/one/0000000000000000", b"first").await.unwrap();
    assert!(
        b.put("logs/one/0000000000000000", b"different")
            .await
            .is_err()
    );
    assert_eq!(
        a.get("logs/one/0000000000000000").await.unwrap().unwrap(),
        b"first"
    );
    assert!(a.put("../outside", b"bad").await.is_err());
    a.delete("logs/one/0000000000000000").await.unwrap();
    assert!(b.get("logs/one/0000000000000000").await.unwrap().is_none());
    let other = Store::open(Config::Filesystem {
        directory: d.path().join("separate"),
        shared: true,
        retention_days: 30,
    })
    .await
    .unwrap();
    assert_ne!(a.id, other.id);
}
#[tokio::test]
async fn s3_conditional_requests_and_reconnect_preserve_objects() {
    use axum::{
        Router,
        body::Bytes,
        extract::{Path, State},
        http::{HeaderMap, StatusCode},
        routing::get,
    };
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    type Objects = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;
    async fn get_object(
        State(db): State<Objects>,
        Path(key): Path<String>,
    ) -> (StatusCode, Vec<u8>) {
        match db.lock().unwrap().get(&key) {
            Some(v) => (StatusCode::OK, v.clone()),
            None => (StatusCode::NOT_FOUND, vec![]),
        }
    }
    async fn put_object(
        State(db): State<Objects>,
        Path(key): Path<String>,
        headers: HeaderMap,
        body: Bytes,
    ) -> StatusCode {
        assert!(
            headers["authorization"]
                .to_str()
                .unwrap()
                .starts_with("AWS4-HMAC-SHA256 Credential=fixture/")
        );
        assert_eq!(
            headers["x-amz-content-sha256"],
            hex::encode(Sha256::digest(&body))
        );
        assert_eq!(headers["if-none-match"], "*");
        let mut db = db.lock().unwrap();
        if let std::collections::btree_map::Entry::Vacant(entry) = db.entry(key) {
            entry.insert(body.to_vec());
            StatusCode::OK
        } else {
            StatusCode::PRECONDITION_FAILED
        }
    }
    async fn delete_object(State(db): State<Objects>, Path(key): Path<String>) -> StatusCode {
        db.lock().unwrap().remove(&key);
        StatusCode::NO_CONTENT
    }
    let db: Objects = Default::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Router::new()
        .route(
            "/{*key}",
            get(get_object).put(put_object).delete(delete_object),
        )
        .with_state(db.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let d = tempfile::tempdir().unwrap();
    converyn_runtime::credentials::pki::private(&d.path().join("access"), b"fixture").unwrap();
    converyn_runtime::credentials::pki::private(&d.path().join("secret"), b"fixture-secret")
        .unwrap();
    let c = Config::S3 {
        endpoint: format!("http://{addr}/"),
        bucket: "test-bucket".into(),
        prefix: "cluster/logs".into(),
        region: "us-east-1".into(),
        access_key_file: d.path().join("access"),
        secret_key_file: d.path().join("secret"),
        session_token_file: None,
        retention_days: 30,
    };
    let a = Store::open(c.clone()).await.unwrap();
    let b = Store::open(c).await.unwrap();
    assert_eq!(a.id, b.id);
    a.put("chunk", b"payload").await.unwrap();
    b.put("chunk", b"payload").await.unwrap();
    assert!(a.put("chunk", b"overwrite").await.is_err());
    assert_eq!(b.get("chunk").await.unwrap().unwrap(), b"payload");
    a.delete("chunk").await.unwrap();
    assert!(b.get("chunk").await.unwrap().is_none());
    task.abort();
}
