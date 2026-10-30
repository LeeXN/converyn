use axum::{
    Json, Router,
    routing::{get, post},
};
use openraft::{
    BasicNode, Vote,
    network::{RPCOption, RaftNetwork, RaftNetworkFactory},
    raft::VoteRequest,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[tokio::test]
async fn incompatible_or_misdirected_peer_never_receives_mutating_rpc() {
    for health in [
        serde_json::json!({"live":true,"node_id":1}),
        serde_json::json!({"live":true,"node_id":2,"protocol":converyn_runtime::controller::network::PROTOCOL_VERSION}),
        serde_json::json!({"live":true,"node_id":1,"protocol":converyn_runtime::controller::network::PROTOCOL_VERSION,"plugin_manifest":"wrong-contracts"}),
    ] {
        let writes = Arc::new(AtomicUsize::new(0));
        let count = writes.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/health/live",
                get(move || {
                    let health = health.clone();
                    async move { Json(health) }
                }),
            )
            .route(
                "/internal/raft/vote",
                post(move || {
                    let count = count.clone();
                    async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        Json(serde_json::json!({}))
                    }
                }),
            );
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut network = converyn_runtime::controller::network::Network {
            store: None,
            peer_fingerprints: Default::default(),
            https: false,
            token: "a".repeat(32),
            cluster: "test".into(),
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        };
        let mut peer = network
            .new_client(
                1,
                &BasicNode {
                    addr: addr.to_string(),
                },
            )
            .await;
        assert!(
            peer.vote(
                VoteRequest::new(Vote::new(1, 1), None),
                RPCOption::new(Duration::from_secs(2))
            )
            .await
            .is_err()
        );
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        task.abort();
    }
}
