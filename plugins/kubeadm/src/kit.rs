//! Operator-side kubeadm inputs. Private join credentials and CA keys are
//! written locally; the public manifest contains only immutable file digests.
use anyhow::{Result, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
fn token(n: usize) -> Result<String> {
    let chars = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut result = String::new();
    let mut source = File::open("/dev/urandom")?;
    while result.len() < n {
        let mut bytes = [0; 32];
        source.read_exact(&mut bytes)?;
        for b in bytes {
            if b < 252 {
                result.push(chars[usize::from(b % 36)] as char);
                if result.len() == n {
                    break;
                }
            }
        }
    }
    Ok(result)
}
pub fn create(
    host: &dyn converyn_plugin_api::PreparationHost,
    directory: &Path,
    version: &str,
    endpoint: &str,
    advertise: std::net::IpAddr,
    pod_cidr: &str,
    service_cidr: &str,
) -> Result<()> {
    ensure!(
        matches!(version, "v1.36.4" | "v1.37.0"),
        "unsupported kubeadm profile"
    );
    ensure!(
        !directory.exists(),
        "refusing to overwrite a Kubernetes provisioning kit"
    );
    let endpoint_url = url::Url::parse(&format!("https://{endpoint}"))?;
    ensure!(
        endpoint_url.host().is_some()
            && endpoint
                .rsplit_once(':')
                .is_some_and(|(_, p)| p.parse::<u16>().is_ok_and(|n| n > 0))
            && endpoint_url.username().is_empty()
            && endpoint_url.password().is_none()
            && endpoint_url.path() == "/"
            && endpoint_url.query().is_none()
            && endpoint_url.fragment().is_none(),
        "endpoint must use host:port without credentials"
    );
    let mut networks = Vec::new();
    for cidr in [pod_cidr, service_cidr] {
        let (address, prefix) = cidr
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("CIDR prefix missing"))?;
        let address: std::net::IpAddr = address.parse()?;
        let prefix: u8 = prefix.parse()?;
        ensure!(
            prefix <= if address.is_ipv4() { 32 } else { 128 },
            "invalid CIDR prefix"
        );
        let (value, bits) = match address {
            std::net::IpAddr::V4(ip) => (u128::from(u32::from(ip)), 32),
            std::net::IpAddr::V6(ip) => (u128::from(ip), 128),
        };
        let mask = if prefix == 0 {
            0
        } else {
            u128::MAX << (bits - u32::from(prefix))
        };
        ensure!(value & mask == value, "CIDR must use its network address");
        networks.push((value, mask, bits));
    }
    let (pod, service) = (networks[0], networks[1]);
    ensure!(
        pod.2 != service.2 || (pod.0 & service.1 != service.0 && service.0 & pod.1 != pod.0),
        "Pod and Service CIDRs overlap"
    );
    let (advertise_value, bits) = match advertise {
        std::net::IpAddr::V4(ip) => (u128::from(u32::from(ip)), 32),
        std::net::IpAddr::V6(ip) => (u128::from(ip), 128),
    };
    ensure!(
        networks
            .iter()
            .all(|(net, mask, family)| *family != bits || advertise_value & mask != *net),
        "advertise address overlaps Pod or Service network"
    );
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(directory)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    let authority = host.create_authority(&directory.join("control-plane-pki"), "kubernetes")?;
    let ca = std::fs::read(&authority.cert)?;
    let ca_sha = hex::encode(Sha256::digest(&ca));
    let discovery_pin = host.describe_certificate(&ca)?.discovery_pin;
    let bootstrap = format!("{}.{}", token(6)?, token(16)?);
    let certificate_key = host.random_token()?;
    let init = json!({"apiVersion":"kubeadm.k8s.io/v1beta4","kind":"InitConfiguration","bootstrapTokens":[{"token":bootstrap,"ttl":"2h0m0s"}],"certificateKey":certificate_key,"localAPIEndpoint":{"advertiseAddress":advertise.to_string(),"bindPort":6443},"nodeRegistration":{"criSocket":"unix:///run/containerd/containerd.sock"}});
    let cluster = json!({"apiVersion":"kubeadm.k8s.io/v1beta4","kind":"ClusterConfiguration","kubernetesVersion":version,"controlPlaneEndpoint":endpoint,"encryptionAlgorithm":"ECDSA-P256","networking":{"podSubnet":pod_cidr,"serviceSubnet":service_cidr},"apiServer":{"certSANs":[endpoint_url.host_str().unwrap(),advertise.to_string().as_str()]}});
    let join = json!({"apiVersion":"kubeadm.k8s.io/v1beta4","kind":"JoinConfiguration","discovery":{"bootstrapToken":{"apiServerEndpoint":endpoint,"token":bootstrap,"caCertHashes":[format!("sha256:{discovery_pin}")]}},"nodeRegistration":{"criSocket":"unix:///run/containerd/containerd.sock"}});
    let mut control = join.clone();
    control["controlPlane"] = json!({"certificateKey":certificate_key});
    let upgrade = json!({"apiVersion":"kubeadm.k8s.io/v1beta4","kind":"UpgradeConfiguration","apply":{"kubernetesVersion":version,"etcdUpgrade":true,"certificateRenewal":true}});
    let files = [
        (
            "init.yaml",
            format!(
                "{}---\n{}",
                serde_yaml::to_string(&init)?,
                serde_yaml::to_string(&cluster)?
            ),
        ),
        ("join-worker.yaml", serde_yaml::to_string(&join)?),
        ("join-control-plane.yaml", serde_yaml::to_string(&control)?),
        ("upgrade.yaml", serde_yaml::to_string(&upgrade)?),
    ];
    let mut digests = std::collections::BTreeMap::new();
    for (name, content) in files {
        host.write_private(&directory.join(name), content.as_bytes())?;
        digests.insert(name, hex::encode(Sha256::digest(content.as_bytes())));
    }
    host.write_private(&directory.join("ca.crt"), &ca)?;
    host.write_private(
        &directory.join("manifest.json"),
        &serde_json::to_vec_pretty(
            &json!({"version":version,"endpoint":endpoint,"ca_sha256":ca_sha,"discovery_ca_pin":discovery_pin,"config_sha256":digests,"bootstrap_token_ttl_seconds":7200,"control_plane_key_directory":"control-plane-pki","private_configuration_files":["init.yaml","join-worker.yaml","join-control-plane.yaml"]}),
        )?,
    )?;
    Ok(())
}

pub fn upgrade(
    host: &dyn converyn_plugin_api::PreparationHost,
    directory: &Path,
    version: &str,
    ca_file: &Path,
) -> Result<()> {
    ensure!(
        matches!(version, "v1.36.4" | "v1.37.0"),
        "unsupported kubeadm profile"
    );
    ensure!(!directory.exists(), "refusing to overwrite an upgrade kit");
    let ca = std::fs::read(ca_file)?;
    let info = host.describe_certificate(&ca)?;
    ensure!(
        info.is_ca && info.can_sign && info.expires_ms > host.now_ms(),
        "upgrade requires the existing cluster's valid CA certificate"
    );
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(directory)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    let config = serde_yaml::to_string(
        &json!({"apiVersion":"kubeadm.k8s.io/v1beta4","kind":"UpgradeConfiguration","apply":{"kubernetesVersion":version,"etcdUpgrade":true,"certificateRenewal":true}}),
    )?;
    host.write_private(&directory.join("upgrade.yaml"), config.as_bytes())?;
    host.write_private(&directory.join("ca.crt"), &ca)?;
    host.write_private(
        &directory.join("manifest.json"),
        &serde_json::to_vec_pretty(
            &json!({"version":version,"ca_sha256":hex::encode(Sha256::digest(&ca)),"config_sha256":{"upgrade.yaml":hex::encode(Sha256::digest(config.as_bytes()))},"reuses_existing_cluster_ca":true}),
        )?,
    )?;
    Ok(())
}
