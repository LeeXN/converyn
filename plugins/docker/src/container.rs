//! Docker reconciliation verifies the resolved image/platform, actual container
//! configuration and Engine health. It never deletes named or bind-mounted data.
use anyhow::{Context, Result, ensure};
use converyn_plugin_api::{self as api, digest, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub host: u16,
    pub container: u16,
    #[serde(default = "loopback")]
    pub address: String,
}
fn loopback() -> String {
    "127.0.0.1".into()
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Mount {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub name: String,
    pub image: String,
    pub platform: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub environment: api::variables::Variables,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub mounts: Vec<Mount>,
    pub healthcheck: String,
    pub readiness_timeout_seconds: u32,
    #[serde(default = "user")]
    pub user_id: u32,
    #[serde(default = "memory")]
    pub memory_bytes: u64,
    #[serde(default = "cpu")]
    pub cpu_millis: u32,
    #[serde(default)]
    pub credential_ref: Option<api::variables::Reference>,
}
fn user() -> u32 {
    65534
}
fn memory() -> u64 {
    256 * 1024 * 1024
}
fn cpu() -> u32 {
    1000
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub registry: String,
    pub directory: PathBuf,
    #[serde(default = "converyn_plugin_api::default_projects")]
    pub projects: BTreeSet<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub names: BTreeSet<String>,
    pub registries: BTreeSet<String>,
    #[serde(default)]
    pub volume_roots: Vec<PathBuf>,
    #[serde(default)]
    pub credentials: BTreeMap<String, Credential>,
    #[serde(default)]
    pub allow_root_user: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Observation {
    pub container_id: String,
    pub image_id: String,
    pub image_digest: String,
    pub platform: String,
    pub owner: String,
    pub configuration_sha256: String,
    pub healthy: bool,
}
pub(crate) fn normalized_image(image: &str) -> Result<(String, String, String)> {
    let (name, sha) = image
        .split_once("@sha256:")
        .context("container image must be pinned by digest")?;
    ensure!(
        api::valid_sha(sha)
            && !name.is_empty()
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"/._-:".contains(&b)),
        "invalid image reference"
    );
    let first = name.split('/').next().unwrap();
    let explicit = first.contains('.') || first.contains(':') || first == "localhost";
    let registry = if explicit { first } else { "docker.io" };
    let qualified = if explicit {
        name.to_string()
    } else if name.contains('/') {
        format!("docker.io/{name}")
    } else {
        format!("docker.io/library/{name}")
    };
    Ok((
        format!("{qualified}@sha256:{sha}"),
        registry.into(),
        sha.into(),
    ))
}
fn path_ok(path: &str) -> bool {
    path.starts_with('/')
        && path != "/"
        && path.len() < 4096
        && !path.contains(['\0', ',', '\n'])
        && path
            .split('/')
            .skip(1)
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
impl Spec {
    pub fn configuration_sha256(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.image = normalized_image(&self.image)?.0;
        copy.credential_ref = None;
        copy.readiness_timeout_seconds = 1;
        for port in &mut copy.ports {
            port.address = port.address.parse::<std::net::IpAddr>()?.to_string();
        }
        copy.ports.sort_by(|a, b| {
            (&a.address, a.host, a.container).cmp(&(&b.address, b.host, b.container))
        });
        copy.mounts.sort_by(|a, b| a.target.cmp(&b.target));
        Ok(digest(&copy))
    }
    pub fn validate(&self) -> Result<()> {
        normalized_image(&self.image)?;
        ensure!(
            valid_id(&self.name)
                && matches!(self.platform.as_str(), "linux/amd64" | "linux/arm64")
                && !self.command.is_empty()
                && self.command.len() <= 32
                && self
                    .command
                    .iter()
                    .all(|a| a.len() <= 4096 && !a.contains('\0')),
            "invalid container identity, platform or command"
        );
        api::variables::validate(&self.environment)?;
        ensure!(
            !self.healthcheck.is_empty()
                && self.healthcheck.len() <= 4096
                && !self.healthcheck.contains('\0')
                && (1..=600).contains(&self.readiness_timeout_seconds)
                && (32 * 1024 * 1024..=64 * 1024 * 1024 * 1024).contains(&self.memory_bytes)
                && (100..=64000).contains(&self.cpu_millis),
            "invalid container health or limits"
        );
        ensure!(
            self.ports.len() <= 16
                && self.ports.iter().all(|p| p.host >= 1024
                    && p.container > 0
                    && p.address.parse::<std::net::IpAddr>().is_ok())
                && self
                    .ports
                    .iter()
                    .map(|p| (&p.address, p.host))
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.ports.len(),
            "invalid or duplicate port binding"
        );
        ensure!(
            self.mounts.len() <= 16
                && self.mounts.iter().all(|m| path_ok(&m.source)
                    && path_ok(&m.target)
                    && !["/proc", "/sys", "/dev"]
                        .iter()
                        .any(|root| Path::new(&m.target).starts_with(root))),
            "invalid container mount"
        );
        ensure!(
            self.credential_ref
                .as_ref()
                .is_none_or(|s| valid_id(&s.secret) && valid_id(&s.version))
                && serde_json::to_vec(self)?.len() <= 16384,
            "container configuration exceeds limit"
        );
        Ok(())
    }
}
fn protected_volume(path: &Path) -> Result<()> {
    ensure!(
        std::fs::canonicalize(path)? == path && std::fs::symlink_metadata(path)?.is_dir(),
        "volume source must be a real canonical directory"
    );
    let mut child = path;
    while let Some(parent) = child.parent() {
        let m = std::fs::symlink_metadata(parent)?;
        let c = std::fs::symlink_metadata(child)?;
        ensure!(
            m.is_dir()
                && m.uid() == 0
                && (m.mode() & 0o022 == 0 || (m.mode() & 0o1000 != 0 && c.uid() == 0)),
            "volume source has a replaceable ancestor"
        );
        child = parent;
    }
    Ok(())
}
fn call(
    config: &Path,
    args: &[String],
    timeout: u64,
    cancel: &AtomicBool,
) -> Result<(bool, String)> {
    ensure!(!cancel.load(Ordering::SeqCst), "container action canceled");
    let mut child = Command::new("/usr/bin/docker")
        .args(["--host=unix:///var/run/docker.sock", "--config"])
        .arg(config)
        .args(args)
        .env_remove("DOCKER_HOST")
        .env_remove("DOCKER_CONTEXT")
        .env_remove("DOCKER_TLS")
        .env_remove("DOCKER_TLS_VERIFY")
        .env_remove("DOCKER_CERT_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut data = Vec::new();
        let mut buffer = [0; 16384];
        loop {
            let n = stdout.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            if data.len() + n <= 1024 * 1024 {
                data.extend_from_slice(&buffer[..n]);
            }
        }
        Ok::<_, std::io::Error>(data)
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(timeout) || cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            anyhow::bail!(
                "container Engine acknowledgment lost or canceled; observe the named resource before retry"
            );
        }
        thread::sleep(Duration::from_millis(100));
    };
    let output = reader
        .join()
        .map_err(|_| anyhow::anyhow!("container output reader failed"))??;
    Ok((status.success(), String::from_utf8(output)?))
}
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
fn inspect(config: &Path, kind: &str, name: &str, cancel: &AtomicBool) -> Result<Option<Value>> {
    let (ok, text) = call(config, &args(&[kind, "inspect", name]), 30, cancel)?;
    if !ok {
        return Ok(None);
    }
    let values: Vec<Value> = serde_json::from_str(&text)?;
    Ok(values.into_iter().next())
}
fn matches(
    actual: &Value,
    spec: &Spec,
    image: &Value,
    owner: &str,
    configuration: &str,
    environment: &BTreeMap<String, String>,
) -> bool {
    let cfg = &actual["Config"];
    let host = &actual["HostConfig"];
    if actual["Image"] != image["Id"]
        || cfg["User"] != format!("{}:{}", spec.user_id, spec.user_id)
        || cfg["Cmd"] != json!(spec.command)
        || cfg["Entrypoint"] != image["Config"]["Entrypoint"]
        || cfg["Labels"]["converyn.owner"] != owner
        || cfg["Labels"]["converyn.configuration"] != configuration
        || host["ReadonlyRootfs"] != true
        || host["Privileged"] != false
        || host["Init"] != true
        || host["Memory"] != spec.memory_bytes
        || host["NanoCpus"] != u64::from(spec.cpu_millis) * 1000000
        || host["RestartPolicy"]["Name"] != "unless-stopped"
        || cfg["Healthcheck"]["Test"] != json!(["CMD-SHELL", spec.healthcheck])
    {
        return false;
    }
    if !host["CapDrop"].as_array().is_some_and(|a| {
        a.iter()
            .any(|v| v.as_str().is_some_and(|v| v.eq_ignore_ascii_case("all")))
    }) || !host["SecurityOpt"].as_array().is_some_and(|a| {
        a.iter().any(|v| {
            v.as_str()
                .is_some_and(|v| v.starts_with("no-new-privileges"))
        })
    }) {
        return false;
    }
    if host["CapAdd"].as_array().is_some_and(|v| !v.is_empty())
        || host["NetworkMode"] != "bridge"
        || host["PidMode"].as_str().is_some_and(|s| !s.is_empty())
        || host["IpcMode"] != "private"
        || cfg["WorkingDir"] != image["Config"]["WorkingDir"]
    {
        return false;
    }
    let mut env = BTreeMap::new();
    for entry in image["Config"]["Env"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if let Some((k, v)) = entry.split_once('=') {
            env.insert(k.to_string(), v.to_string());
        }
    }
    env.extend(environment.clone());
    let observed = cfg["Env"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|e| e.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<BTreeMap<_, _>>();
    if env != observed {
        return false;
    }
    let mut bindings: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for p in &spec.ports {
        bindings.entry(format!("{}/tcp",p.container)).or_default().push(json!({"HostIp":p.address.parse::<std::net::IpAddr>().unwrap().to_string(),"HostPort":p.host.to_string()}));
    }
    for values in bindings.values_mut() {
        values.sort_by_key(|v| v.to_string());
    }
    let actual_ports = host["PortBindings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if json!(actual_ports) != json!(bindings) {
        return false;
    }
    let mounts = actual["Mounts"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if mounts.len() != spec.mounts.len() {
        return false;
    }
    spec.mounts.iter().all(|wanted| {
        mounts.iter().any(|m| {
            m["Type"] == "bind"
                && m["Source"] == wanted.source
                && m["Destination"] == wanted.target
                && m["RW"] == !wanted.read_only
        })
    })
}
pub fn reconcile(context: &api::Context<'_>, spec: &Spec) -> Result<api::Applied> {
    let cancel = context.cancel;
    spec.validate()?;
    ensure!(
        // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
        unsafe { libc::geteuid() } == 0,
        "container management requires root supervisor"
    );
    let rules: Rules = serde_json::from_value(context.policy.clone())?;
    ensure!(
        rules.names.contains(&spec.name) && (spec.user_id > 0 || rules.allow_root_user),
        "container identity/user is not allowed"
    );
    let environment = context.host.resolve(&spec.environment)?;
    ensure!(
        environment
            .values
            .values()
            .all(|v| !v.contains(['\n', '\r'])),
        "container environment must be single-line; use mounted application secrets for multiline data"
    );
    let owner = context.owner;
    let (image_name, registry, image_digest) = normalized_image(&spec.image)?;
    ensure!(
        rules.registries.contains(&registry),
        "container registry is not allowed"
    );
    let platform = match std::env::consts::ARCH {
        "x86_64" => "linux/amd64",
        "aarch64" => "linux/arm64",
        _ => "unsupported",
    };
    ensure!(
        spec.platform == platform,
        "container platform differs from the Node platform"
    );
    for mount in &spec.mounts {
        let path = Path::new(&mount.source);
        ensure!(
            rules.volume_roots.iter().any(|root| path == root),
            "volume source is not an explicitly allowed root"
        );
        protected_volume(path)?;
    }
    struct PrivateConfig(Option<PathBuf>);
    impl Drop for PrivateConfig {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = std::fs::remove_file(path.join("config.json"));
                let _ = std::fs::remove_dir(path);
            }
        }
    }
    let mut private = PrivateConfig(None);
    let config = if let Some(reference) = &spec.credential_ref {
        let variables = [(
            "CONVERYN_DOCKER_AUTH".into(),
            api::variables::Value::Secret(reference.clone()),
        )]
        .into();
        let resolved = context.host.resolve(&variables)?;
        let value = &resolved.values["CONVERYN_DOCKER_AUTH"];
        let document: Value = serde_json::from_str(value)
            .context("Docker registry Secret must contain config.json")?;
        ensure!(
            document
                .get("auths")
                .and_then(Value::as_object)
                .is_some_and(|a| a.contains_key(&registry)),
            "Docker registry Secret must contain the requested registry"
        );
        ensure!(
            document
                .as_object()
                .is_some_and(|m| m.keys().all(|k| k == "auths")),
            "Docker registry Secret cannot configure external credential helpers"
        );
        let path =
            context
                .state_dir
                .join(format!("docker-auth-{}-{}", context.fence, digest(spec)));
        std::fs::create_dir_all(&path)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        context.host.private_directory(&path)?;
        private.0 = Some(path.clone());
        context
            .host
            .write_private(&path.join("config.json"), value.as_bytes())?;
        path
    } else {
        let path = context.state_dir.join("docker-public");
        std::fs::create_dir_all(&path)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        path
    };
    // Inspect failures are distinguished from a healthy Engine before interpreting absence.
    ensure!(
        call(
            &config,
            &args(&["info", "--format", "{{.ServerVersion}}"]),
            30,
            cancel
        )?
        .0,
        "Docker Engine is unavailable"
    );
    let image = if let Some(image) = inspect(&config, "image", &image_name, cancel)? {
        image
    } else {
        ensure!(
            call(
                &config,
                &args(&["pull", "--platform", &spec.platform, &image_name]),
                1800,
                cancel
            )?
            .0,
            "immutable image pull failed"
        );
        inspect(&config, "image", &image_name, cancel)?.context("pulled image not inspectable")?
    };
    ensure!(
        format!(
            "{}/{}",
            image["Os"].as_str().unwrap_or(""),
            image["Architecture"].as_str().unwrap_or("")
        ) == spec.platform
            && image["RepoDigests"]
                .as_array()
                .is_some_and(|v| v.iter().any(|d| d
                    .as_str()
                    .is_some_and(|d| d.ends_with(&format!("@sha256:{image_digest}"))))),
        "resolved image digest/platform mismatch"
    );
    ensure!(
        image["Config"]["Volumes"]
            .as_object()
            .is_none_or(|volumes| volumes
                .keys()
                .all(|target| spec.mounts.iter().any(|mount| &mount.target == target))),
        "image-declared data volumes require explicit bind mappings"
    );
    let configuration = spec.configuration_sha256()?;
    let existing = inspect(&config, "container", &spec.name, cancel)?;
    let mut changed = false;
    if let Some(container) = &existing {
        ensure!(
            container["Config"]["Labels"]["converyn.owner"] == owner,
            "refusing to replace an unmanaged or differently owned container"
        );
        if !matches(
            container,
            spec,
            &image,
            owner,
            &configuration,
            &environment.values,
        ) {
            ensure!(
                call(
                    &config,
                    &args(&["stop", "--time", "30", &spec.name]),
                    45,
                    cancel
                )?
                .0,
                "container stop was not confirmed"
            );
            ensure!(
                call(&config, &args(&["rm", &spec.name]), 30, cancel)?.0,
                "container removal was not confirmed"
            );
            changed = true;
        }
    }
    if existing.is_none() || changed {
        let mut command = args(&[
            "run",
            "--detach",
            "--name",
            &spec.name,
            "--platform",
            &spec.platform,
            "--read-only",
            "--init",
            "--network=bridge",
            "--ipc=private",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--restart=unless-stopped",
            "--tmpfs",
            "/tmp:rw,noexec,nosuid,size=64m",
            "--health-cmd",
            &spec.healthcheck,
            "--health-interval=1s",
            "--health-timeout=2s",
            "--health-retries=3",
            "--user",
            &format!("{}:{}", spec.user_id, spec.user_id),
            "--memory",
            &spec.memory_bytes.to_string(),
            "--cpus",
            &format!("{:.3}", f64::from(spec.cpu_millis) / 1000.0),
            "--label",
            &format!("converyn.owner={owner}"),
            "--label",
            &format!("converyn.configuration={configuration}"),
        ]);
        let env_path = context
            .state_dir
            .join(format!("docker-env-{}", &configuration[..24]));
        let env = environment
            .values
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect::<String>();
        context.host.write_private(&env_path, env.as_bytes())?;
        command.extend(["--env-file".into(), env_path.to_string_lossy().into_owned()]);
        for port in &spec.ports {
            command.extend([
                "--publish".into(),
                format!(
                    "{}:{}:{}",
                    if port.address.contains(':') {
                        format!("[{}]", port.address)
                    } else {
                        port.address.clone()
                    },
                    port.host,
                    port.container
                ),
            ]);
        }
        for mount in &spec.mounts {
            command.extend([
                "--mount".into(),
                format!(
                    "type=bind,src={},dst={}{}",
                    mount.source,
                    mount.target,
                    if mount.read_only { ",readonly" } else { "" }
                ),
            ]);
        }
        command.push(image_name);
        command.extend(spec.command.clone());
        let created = call(&config, &command, 60, cancel);
        std::fs::remove_file(&env_path)?;
        std::fs::File::open(env_path.parent().unwrap())?.sync_all()?;
        ensure!(created?.0, "container creation/start was not confirmed");
        changed = true;
    } else if existing
        .as_ref()
        .is_some_and(|c| c["State"]["Running"] != true)
    {
        ensure!(
            call(&config, &args(&["start", &spec.name]), 60, cancel)?.0,
            "container start was not confirmed"
        );
        changed = true;
    }
    let deadline = Instant::now() + Duration::from_secs(u64::from(spec.readiness_timeout_seconds));
    loop {
        let current = inspect(&config, "container", &spec.name, cancel)?
            .context("managed container disappeared")?;
        ensure!(
            matches(
                &current,
                spec,
                &image,
                owner,
                &configuration,
                &environment.values
            ),
            "container configuration changed during verification (network={}, ipc={}, mount_types={})",
            current["HostConfig"]["NetworkMode"],
            current["HostConfig"]["IpcMode"],
            current["Mounts"]
                .as_array()
                .map(|v| v
                    .iter()
                    .map(|m| m["Type"].as_str().unwrap_or("unknown"))
                    .collect::<Vec<_>>())
                .map(|v| v.join(","))
                .unwrap_or_default()
        );
        if current["State"]["Running"] == true
            && current["State"]["Paused"] != true
            && current["State"]["Health"]["Status"] == "healthy"
        {
            let observed = Observation {
                container_id: current["Id"]
                    .as_str()
                    .context("container id missing")?
                    .into(),
                image_id: image["Id"].as_str().context("image id missing")?.into(),
                image_digest,
                platform: spec.platform.clone(),
                owner: owner.into(),
                configuration_sha256: configuration,
                healthy: true,
            };
            return Ok(api::Applied {
                changed,
                payload: serde_json::to_value(observed)?,
            });
        }
        ensure!(
            Instant::now() < deadline,
            "container did not become healthy before readiness deadline (state={}, health={})",
            current["State"]["Status"],
            current["State"]["Health"]["Status"]
        );
        thread::sleep(Duration::from_millis(500));
    }
}
