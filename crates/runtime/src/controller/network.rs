use std::time::Duration;

use openraft::{
    BasicNode,
    error::{InstallSnapshotError, NetworkError, RPCError, RaftError, RemoteError, Unreachable},
    network::{RPCOption, RaftNetwork, RaftNetworkFactory},
    raft::{
        AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest,
        InstallSnapshotResponse, VoteRequest, VoteResponse,
    },
};
use serde::{Serialize, de::DeserializeOwned};

use crate::TypeConfig;
pub const PROTOCOL_VERSION: u64 = 27;

#[derive(Clone)]
pub struct Network {
    pub https: bool,
    pub token: String,
    pub cluster: String,
    pub client: reqwest::Client,
    pub store: Option<crate::storage::store::Store>,
    pub peer_fingerprints: std::collections::BTreeSet<String>,
}
pub struct Peer {
    network: Network,
    target: u64,
    node: BasicNode,
}

impl RaftNetworkFactory<TypeConfig> for Network {
    type Network = Peer;
    async fn new_client(&mut self, target: u64, node: &BasicNode) -> Peer {
        Peer {
            network: self.clone(),
            target,
            node: node.clone(),
        }
    }
}

impl Network {
    pub async fn verify_target(
        &self,
        target: u64,
        node: &BasicNode,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let base = format!(
            "{}://{}",
            if self.https { "https" } else { "http" },
            node.addr
        );
        let response = self
            .client
            .get(format!("{base}/health/live"))
            .timeout(timeout)
            .send()
            .await?
            .error_for_status()?;
        if self.https {
            use sha2::{Digest, Sha256};
            let allowed = if let Some(store) = &self.store {
                store
                    .machine()
                    .await?
                    .state
                    .identities
                    .bindings
                    .get("peer")
                    .map(|binding| binding.fingerprints.clone())
                    .unwrap_or_else(|| self.peer_fingerprints.clone())
            } else {
                self.peer_fingerprints.clone()
            };
            let certificate = response
                .extensions()
                .get::<reqwest::tls::TlsInfo>()
                .and_then(|tls| tls.peer_certificate());
            anyhow::ensure!(
                certificate.is_some_and(|der| allowed.contains(&hex::encode(Sha256::digest(der)))
                    && crate::credentials::pki::info_der(der)
                        .is_ok_and(|info| info.expires_ms > crate::executors::host::now_ms())),
                "peer TLS leaf is expired or not in the committed peer trust set"
            );
        }
        let health: serde_json::Value = response.json().await?;
        anyhow::ensure!(
            health["protocol"] == PROTOCOL_VERSION
                && health["node_id"] == target
                && health["plugin_manifest"] == crate::plugins::registry().fingerprint(),
            "peer protocol/identity mismatch; no payload sent"
        );
        Ok(())
    }
}

impl Peer {
    async fn send<Q: Serialize, A: DeserializeOwned, E: std::error::Error + DeserializeOwned>(
        &self,
        route: &str,
        body: Q,
        timeout: Duration,
    ) -> Result<A, RPCError<u64, BasicNode, E>> {
        let net = |e: reqwest::Error| {
            let e = e.without_url();
            if e.is_connect() || e.is_timeout() {
                RPCError::Unreachable(Unreachable::new(&e))
            } else {
                RPCError::Network(NetworkError::new(&e))
            }
        };
        // Check before any vote/log/snapshot mutation. Otherwise an older peer
        // can accept a JSON snapshot while silently dropping newly added fields.
        let began = std::time::Instant::now();
        self.network
            .verify_target(self.target, &self.node, timeout)
            .await
            .map_err(|e| {
                RPCError::Unreachable(Unreachable::new(&std::io::Error::other(e.to_string())))
            })?;
        let response = self
            .network
            .client
            .post(format!(
                "{}://{}/internal/raft/{route}",
                if self.network.https { "https" } else { "http" },
                self.node.addr
            ))
            .bearer_auth(&self.network.token)
            .header("x-converyn-cluster", &self.network.cluster)
            .header("x-converyn-protocol", PROTOCOL_VERSION.to_string())
            .header(
                "x-converyn-plugin-manifest",
                crate::plugins::registry().fingerprint(),
            )
            .timeout(timeout.saturating_sub(began.elapsed()))
            .json(&body)
            .send()
            .await
            .map_err(net)?
            .error_for_status()
            .map_err(net)?;
        let identity = self.target.to_string();
        if response
            .headers()
            .get("x-converyn-node")
            .and_then(|v| v.to_str().ok())
            != Some(&identity)
        {
            return Err(RPCError::Network(NetworkError::new(
                &std::io::Error::other("peer identity mismatch"),
            )));
        }
        let result: Result<A, E> = response.json().await.map_err(net)?;
        result.map_err(|e| {
            RPCError::RemoteError(RemoteError::new_with_node(
                self.target,
                self.node.clone(),
                e,
            ))
        })
    }
}

impl RaftNetwork<TypeConfig> for Peer {
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<TypeConfig>,
        option: RPCOption,
    ) -> Result<AppendEntriesResponse<u64>, RPCError<u64, BasicNode, RaftError<u64>>> {
        self.send("append", rpc, option.hard_ttl()).await
    }
    async fn install_snapshot(
        &mut self,
        rpc: InstallSnapshotRequest<TypeConfig>,
        option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<u64>,
        RPCError<u64, BasicNode, RaftError<u64, InstallSnapshotError>>,
    > {
        self.send("snapshot", rpc, option.hard_ttl()).await
    }
    async fn vote(
        &mut self,
        rpc: VoteRequest<u64>,
        option: RPCOption,
    ) -> Result<VoteResponse<u64>, RPCError<u64, BasicNode, RaftError<u64>>> {
        self.send("vote", rpc, option.hard_ttl()).await
    }
}
