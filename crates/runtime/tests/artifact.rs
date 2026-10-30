use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};
use converyn_core::model::domain::MAX_ARTIFACT_SIZE;
use converyn_runtime::artifacts::artifact::{Artifact, fetch};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct Source {
    data: Arc<Vec<u8>>,
    ranges: Arc<Mutex<Vec<String>>>,
}
async fn handler(
    State(s): State<Source>,
    Path(mode): Path<String>,
    headers: HeaderMap,
) -> Response {
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    s.ranges.lock().unwrap().push(range.clone());
    let offset = range
        .strip_prefix("bytes=")
        .and_then(|s| s.strip_suffix('-'))
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let (status, start) = if offset > 0 && mode != "ignore" {
        (StatusCode::PARTIAL_CONTENT, offset)
    } else {
        (StatusCode::OK, 0)
    };
    let mut response = Response::builder()
        .status(status)
        .header("etag", "\"v1\"")
        .header("content-length", (s.data.len() - start).to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        response = response.header(
            "content-range",
            format!(
                "bytes {}-{}/{}",
                if mode == "bad-range" {
                    start + 1
                } else {
                    start
                },
                s.data.len() - 1,
                s.data.len()
            ),
        );
    }
    response.body(Body::from(s.data[start..].to_vec())).unwrap()
}
async fn source() -> (String, Source, tokio::task::JoinHandle<()>) {
    let s = Source {
        data: Arc::new((0..262144).map(|n| (n % 251) as u8).collect()),
        ranges: Default::default(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/{mode}", get(handler))
        .with_state(s.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), s, task)
}
fn spec(s: &Source) -> Artifact {
    Artifact {
        sha256: hex::encode(Sha256::digest(s.data.as_slice())),
        size: s.data.len() as u64,
    }
}
fn partial(dir: &std::path::Path, s: &Artifact) -> PathBuf {
    dir.join(format!("{}.part", s.sha256))
}

#[tokio::test]
async fn resumes_partial_file_and_reuses_verified_cache() {
    let (url, s, task) = source().await;
    let dir = tempfile::tempdir().unwrap();
    let a = spec(&s);
    std::fs::write(partial(dir.path(), &a), &s.data[..65536]).unwrap();
    let r = fetch(&format!("{url}/range"), a.clone(), dir.path(), 1, None)
        .await
        .unwrap();
    assert_eq!(r.resumed_from, 65536);
    assert_eq!(std::fs::read(&r.path).unwrap(), *s.data);
    assert_eq!(*s.ranges.lock().unwrap(), vec!["bytes=65536-"]);
    assert!(
        fetch(&format!("{url}/range"), a, dir.path(), 1, None)
            .await
            .unwrap()
            .cache_hit
    );
    assert_eq!(s.ranges.lock().unwrap().len(), 1);
    task.abort();
}

#[tokio::test]
async fn full_response_to_range_replaces_instead_of_appending() {
    let (url, s, task) = source().await;
    let dir = tempfile::tempdir().unwrap();
    let a = spec(&s);
    std::fs::write(partial(dir.path(), &a), b"incorrect partial prefix").unwrap();
    let r = fetch(&format!("{url}/ignore"), a, dir.path(), 1, None)
        .await
        .unwrap();
    assert_eq!(std::fs::read(r.path).unwrap(), *s.data);
    task.abort();
}

#[tokio::test]
async fn bad_range_never_becomes_a_verified_artifact() {
    let (url, s, task) = source().await;
    let dir = tempfile::tempdir().unwrap();
    let a = spec(&s);
    std::fs::write(partial(dir.path(), &a), &s.data[..4096]).unwrap();
    assert!(
        fetch(&format!("{url}/bad-range"), a.clone(), dir.path(), 1, None)
            .await
            .is_err()
    );
    assert!(!dir.path().join(a.sha256).exists());
    task.abort();
}

#[tokio::test]
async fn checksum_mismatch_discards_partial_and_does_not_commit() {
    let (url, s, task) = source().await;
    let dir = tempfile::tempdir().unwrap();
    let mut a = spec(&s);
    a.sha256 = "0".repeat(64);
    assert!(
        fetch(&format!("{url}/range"), a.clone(), dir.path(), 1, None)
            .await
            .is_err()
    );
    assert!(!partial(dir.path(), &a).exists());
    assert!(!dir.path().join(a.sha256).exists());
    task.abort();
}

#[tokio::test]
async fn corrupted_cache_is_not_reported_as_a_hit() {
    let (url, s, task) = source().await;
    let dir = tempfile::tempdir().unwrap();
    let a = spec(&s);
    std::fs::write(dir.path().join(&a.sha256), b"damaged").unwrap();
    assert!(
        fetch(&format!("{url}/range"), a, dir.path(), 1, None)
            .await
            .is_err()
    );
    assert!(s.ranges.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn rejects_oversized_or_unsafe_descriptors_before_network() {
    let dir = tempfile::tempdir().unwrap();
    for a in [
        Artifact {
            sha256: "a".repeat(64),
            size: MAX_ARTIFACT_SIZE + 1,
        },
        Artifact {
            sha256: "../../escape".into(),
            size: 1,
        },
    ] {
        assert!(
            fetch("http://127.0.0.1:1/a", a, dir.path(), 1, None)
                .await
                .is_err()
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn partial_symlink_cannot_overwrite_another_file() {
    let (url, s, task) = source().await;
    let dir = tempfile::tempdir().unwrap();
    let a = spec(&s);
    let other = dir.path().join("precious");
    std::fs::write(&other, b"keep").unwrap();
    std::os::unix::fs::symlink(&other, partial(dir.path(), &a)).unwrap();
    assert!(
        fetch(&format!("{url}/range"), a, dir.path(), 1, None)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(other).unwrap(), b"keep");
    task.abort();
}
