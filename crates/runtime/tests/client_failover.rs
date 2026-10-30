use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::StatusCode,
    routing::get,
};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Clone)]
struct Peer {
    leader: Arc<AtomicBool>,
    ports: Arc<Mutex<BTreeSet<u16>>>,
    requests: Arc<std::sync::atomic::AtomicUsize>,
}
async fn handle(
    State(peer): State<Peer>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
) -> (StatusCode, Json<serde_json::Value>) {
    peer.ports.lock().unwrap().insert(remote.port());
    peer.requests.fetch_add(1, Ordering::SeqCst);
    (
        if peer.leader.load(Ordering::SeqCst) {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(serde_json::json!({"leader":peer.leader.load(Ordering::SeqCst)})),
    )
}
#[tokio::test]
async fn successful_leader_is_reused_and_follower_bodies_keep_connections_reusable() {
    let mut peers = Vec::new();
    let mut urls = Vec::new();
    let mut tasks = Vec::new();
    for leader in [false, true] {
        let peer = Peer {
            leader: Arc::new(AtomicBool::new(leader)),
            ports: Default::default(),
            requests: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        urls.push(format!("http://{}", listener.local_addr().unwrap()));
        let app = Router::new()
            .route("/test", get(handle))
            .with_state(peer.clone());
        peers.push(peer);
        tasks.push(tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap()
        }));
    }
    let client = converyn_runtime::nodes::agent::Client {
        preferred: Default::default(),
        fingerprint: Default::default(),
        hardware: None,
        session: None,
        http: Arc::new(std::sync::RwLock::new(
            reqwest::Client::builder().no_proxy().build().unwrap(),
        )),
        servers: urls,
        token: "fixture".into(),
        cluster: "fixture".into(),
    };
    assert_eq!(
        client
            .request(reqwest::Method::GET, "/test", None)
            .await
            .unwrap()
            .0,
        200
    );
    assert_eq!(
        client
            .request(reqwest::Method::GET, "/test", None)
            .await
            .unwrap()
            .0,
        200
    );
    assert_eq!(peers[0].requests.load(Ordering::SeqCst), 1);
    assert_eq!(peers[1].requests.load(Ordering::SeqCst), 2);
    peers[0].leader.store(true, Ordering::SeqCst);
    peers[1].leader.store(false, Ordering::SeqCst);
    assert_eq!(
        client
            .request(reqwest::Method::GET, "/test", None)
            .await
            .unwrap()
            .0,
        200
    );
    assert_eq!(peers[0].ports.lock().unwrap().len(), 1);
    assert_eq!(peers[1].ports.lock().unwrap().len(), 1);
    for task in tasks {
        task.abort();
    }
}
