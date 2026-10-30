//! Workflow and credential UX. Private output is persisted before registration;
//! retries reuse operation.json rather than minting another identity or key.
use super::*;
use converyn_core::{identities::personnel as people, orchestration::workflow};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[derive(clap::Args)]
pub(crate) struct Connection {
    #[arg(skip)]
    pub config: super::configuration::Source,
}
#[derive(clap::Args)]
pub(crate) struct Signing {
    #[arg(long, env = "CONVERYN_AUTHORITY")]
    pub authority: PathBuf,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, conflicts_with = "expires_at")]
    pub valid_for: Option<String>,
    #[arg(long)]
    pub expires_at: Option<String>,
}
pub(crate) enum WorkflowCommands {
    /// Resolve an uncertain Work after checking its original receipt and effects.
    Resolve {
        connection: Connection,
        run_id: String,
        work_id: String,
        status: String,
        evidence: String,
    },
    Validate {
        file: PathBuf,
    },
    Register {
        connection: Connection,
        file: PathBuf,
    },
    Enable {
        connection: Connection,
        id: String,
    },
    Disable {
        connection: Connection,
        id: String,
    },

    Run(RunArgs),
}
#[derive(clap::Args)]
pub(crate) struct RunArgs {
    #[command(flatten)]
    pub connection: Connection,
    /// Workflow ID.
    pub workflow: String,
    #[arg(long)]
    pub target: Option<String>,
    /// Merge YAML/JSON values files from left to right; later files win.
    #[arg(short = 'f', long = "values", value_name = "FILE")]
    pub values_files: Vec<PathBuf>,
    /// Override or add values (KEY=VALUE, nested keys use dots); overrides files.
    #[arg(long = "set")]
    pub sets: Vec<String>,
    /// Like --set, preserving string values such as numeric version identifiers.
    #[arg(long = "set-string")]
    pub set_strings: Vec<String>,
    #[arg(long)]
    pub request_id: Option<String>,
    #[arg(long, env = "CONVERYN_REQUEST_DIR")]
    pub request_dir: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, conflicts_with = "dry_run")]
    pub wait: bool,
    #[arg(long, requires = "wait")]
    pub timeout_seconds: Option<u64>,
}
pub(crate) enum IdentityCommands {
    Create {
        connection: Connection,
        signing: Signing,
        name: String,
        /// Create a super administrator; ordinary identities start without grants.
        super_admin: bool,
    },
    Update {
        connection: Connection,
        id: String,
        name: String,
    },
    Disable {
        connection: Connection,
        id: String,
    },
    Enable {
        connection: Connection,
        id: String,
    },
}
pub(crate) enum CredentialCommands {
    Create {
        connection: Connection,
        signing: Signing,
        identity: String,
    },
    Rotate {
        connection: Connection,
        signing: Signing,
        identity: String,
        overlap: String,
    },
    Delete {
        connection: Connection,
        id: String,
    },
    Resume {
        connection: Connection,
        output: PathBuf,
    },
    /// Export a public registry certificate, CA bundle, or a local PKCS#12 bundle with private key.
    Export(super::certificate_export::ExportArgs),
}

async fn client(connection: &Connection) -> Result<converyn_runtime::nodes::agent::Client> {
    let c = connection.config.load()?;
    c.client()
}
async fn call(
    api: &converyn_runtime::nodes::agent::Client,
    path: &str,
    body: Option<&Value>,
) -> Result<Value> {
    converyn_runtime::interface::client::request(
        api,
        if body.is_some() {
            reqwest::Method::POST
        } else {
            reqwest::Method::GET
        },
        path,
        body,
    )
    .await
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    output::print(value)
}
fn uid(prefix: &str) -> Result<String> {
    Ok(format!(
        "{prefix}-{}",
        &converyn_runtime::credentials::pki::random_token()?[..24]
    ))
}
fn duration(text: &str) -> Result<u64> {
    let (number, factor) = if let Some(v) = text.strip_suffix('d') {
        (v, 86400)
    } else if let Some(v) = text.strip_suffix('h') {
        (v, 3600)
    } else if let Some(v) = text.strip_suffix('m') {
        (v, 60)
    } else if let Some(v) = text.strip_suffix('s') {
        (v, 1)
    } else {
        (text, 1)
    };
    number
        .parse::<u64>()?
        .checked_mul(factor)
        .context("duration overflow")
}
fn lifetime(signing: &Signing) -> Result<u64> {
    let seconds = if let Some(expires) = &signing.expires_at {
        let at =
            time::OffsetDateTime::parse(expires, &time::format_description::well_known::Rfc3339)?;
        let delta = at.unix_timestamp() - time::OffsetDateTime::now_utc().unix_timestamp();
        u64::try_from(delta).context("expiry must be in the future")?
    } else {
        signing
            .valid_for
            .as_deref()
            .map(duration)
            .transpose()?
            .unwrap_or(people::DEFAULT_LIFETIME)
    };
    ensure!(
        (300..=people::MAX_LIFETIME).contains(&seconds),
        "personal validity must be five minutes to 1095 days"
    );
    Ok(seconds)
}
async fn identities(
    api: &converyn_runtime::nodes::agent::Client,
) -> Result<BTreeMap<String, people::Identity>> {
    Ok(serde_json::from_value(
        call(api, "/api/v1/identities", None).await?["identities"].clone(),
    )?)
}
fn public_path(path: &str, id: &str) -> Result<String> {
    ensure!(converyn_core::model::domain::valid_id(id), "invalid id");
    Ok(format!("{path}/{id}"))
}

pub(crate) async fn workflow(command: WorkflowCommands) -> Result<()> {
    let enable = matches!(&command, WorkflowCommands::Enable { .. });
    match command {
        WorkflowCommands::Resolve {
            connection,
            run_id,
            work_id,
            status,
            evidence,
        } => {
            ensure!(
                converyn_core::model::domain::valid_id(&work_id),
                "invalid Work id"
            );
            let api = client(&connection).await?;
            let path = public_path("/api/v1/runs", &run_id)?;
            let current = call(&api, &path, None).await?;
            print(&call(&api,&format!("{path}/resolve"),Some(&json!({"work_id":work_id,"expected_revision":current["revision"],"status":status,"actor":"","evidence":evidence}))).await?)
        }
        WorkflowCommands::Validate { file } => {
            let mut progress = crate::progress::Progress::new(&file);
            let revision =
                converyn_runtime::orchestration::workflows::package_artifacts_with_progress(
                    &file,
                    |path, read, total| progress.hashing(path, read, total),
                )?
                .revision;
            progress.finish("Workflow validated");
            print(
                &json!({"uid":revision.uid,"revision":converyn_core::model::domain::digest(&revision),"valid":true,"platform_scope":"not_verified","requirements":converyn_runtime::orchestration::workflows::requirements(&revision)}),
            )
        }
        WorkflowCommands::Register { connection, file } => {
            let api = client(&connection).await?;
            let mut progress = crate::progress::Progress::new(&file);
            let packaged =
                converyn_runtime::orchestration::workflows::package_artifacts_with_progress(
                    &file,
                    |path, read, total| progress.hashing(path, read, total),
                )?;
            let files = packaged
                .artifacts
                .iter()
                .map(|(sha, path)| Ok((sha, path, std::fs::metadata(path)?.len())))
                .collect::<Result<Vec<_>>>()?;
            progress.uploads(files.len(), files.iter().map(|(_, _, size)| *size).sum());
            for (sha, path, size) in files {
                progress.file(path, size);
                converyn_runtime::artifacts::transfer::upload_with_progress(
                    &api,
                    path,
                    &converyn_runtime::artifacts::artifact::Artifact {
                        sha256: sha.clone(),
                        size,
                    },
                    |event| progress.transfer(event),
                )
                .await?;
            }
            let workflow = packaged.revision;
            progress.registering();
            let all =
                super::query::collection(&api, super::query::Resource::Workflows, None, None, None)
                    .await?;
            let expected = all["workflows"]
                .as_array()
                .context("invalid workflow list")?
                .iter()
                .find(|v| v["workflow"]["uid"] == workflow.uid)
                .and_then(|v| v["workflow"]["revision"].as_u64())
                .unwrap_or(0);
            let reply = call(
                &api,
                "/api/v1/workflows/register",
                Some(&json!({"workflow":workflow,"expected_revision":expected})),
            )
            .await?;
            progress.finish("Workflow registration confirmed");
            print(&reply)
        }
        WorkflowCommands::Enable { connection, id }
        | WorkflowCommands::Disable { connection, id } => {
            let enabled = enable;
            let api = client(&connection).await?;
            let path = public_path("/api/v1/workflows", &id)?;
            let current = call(&api, &path, None).await?;
            print(&call(&api,&format!("{path}/enabled"),Some(&json!({"enabled":enabled,"expected_revision":current["workflow"]["revision"]}))).await?)
        }
        WorkflowCommands::Run(RunArgs {
            connection,
            workflow,
            target,
            values_files,
            sets,
            set_strings,
            request_id,
            request_dir,
            dry_run,
            wait,
            timeout_seconds,
        }) => {
            let api = client(&connection).await?;
            let id = workflow;
            let request_id = request_id.unwrap_or(uid("request")?);
            ensure!(
                converyn_core::model::domain::valid_id(&request_id) && request_id.len() <= 124,
                "invalid request id"
            );
            let path = public_path("/api/v1/workflows", &id)?;
            let run_id = format!("run-{request_id}");
            let mut file_values = json!({});
            for file in &values_files {
                let values: Value = serde_yaml::from_slice(&bounded_input(file)?)?;
                ensure!(values.is_object(), "values file must contain an object");
                converyn_core::orchestration::values::merge(&mut file_values, &values);
            }
            for item in &sets {
                super::values::set(&mut file_values, item, false)?;
            }
            for item in &set_strings {
                super::values::set(&mut file_values, item, true)?;
            }
            let file_values: BTreeMap<String, Value> = serde_json::from_value(file_values)?;
            let intent = converyn_core::model::domain::digest(&(&id, &target, &file_values));
            let config = connection.config.path()?.canonicalize()?;
            let mut directory = std::path::absolute(
                request_dir.unwrap_or_else(|| config.parent().unwrap().join("requests")),
            )?
            .join(&converyn_core::model::domain::digest(&(&api.cluster, &api.servers))[..16]);
            // Shared files isolate people, while credential rotation keeps their original outbox.
            let embedded = api
                .fingerprint
                .read()
                .map_err(|_| anyhow::anyhow!("client identity lock poisoned"))?
                .is_some();
            if embedded {
                let me = call(&api, "/api/v1/identity/me", None).await?;
                let identity = me["identity"]["uid"]
                    .as_str()
                    .context("workflow execution requires a named personal identity")?;
                directory = directory.join(&converyn_core::model::domain::digest(&identity)[..16]);
            }
            let saved = directory.join(format!("{request_id}.json"));
            eprintln!(
                "request_id={request_id} run_id={run_id} request_file={}",
                saved.display()
            );
            let saved_request = if !dry_run && saved.exists() {
                converyn_runtime::credentials::security::private_state_directory(&directory)?;
                let record: Value = serde_json::from_slice(&bounded_input(&saved)?)?;
                ensure!(
                    record["intent"] == intent,
                    "request id already has different CLI inputs"
                );
                Some(serde_json::from_value::<workflow::Request>(
                    record["request"].clone(),
                )?)
            } else {
                None
            };
            let request = if let Some(original) = saved_request {
                original
            } else {
                let definition = call(&api, &path, None).await?;
                let targets = definition["definition"]["targets"]
                    .as_object()
                    .context("missing target presets")?;
                let target = target
                    .or_else(|| {
                        (targets.len() == 1).then(|| targets.keys().next().unwrap().clone())
                    })
                    .with_context(|| {
                        format!(
                            "choose a preset with --target; available: {}",
                            targets.keys().cloned().collect::<Vec<_>>().join(", ")
                        )
                    })?;
                let values = file_values;
                let preview=call(&api,&format!("{path}/preview"),Some(&json!({"workflow_revision":definition["workflow"]["current"],"target":target,"parameters":values,"request_id":request_id}))).await?;
                if dry_run {
                    return print(&preview);
                }
                let original: workflow::Request =
                    serde_json::from_value(preview["request"].clone())?;
                use std::os::unix::fs::DirBuilderExt;
                if !directory.exists() {
                    std::fs::DirBuilder::new()
                        .recursive(true)
                        .mode(0o700)
                        .create(&directory)?;
                }
                converyn_runtime::credentials::security::private_state_directory(&directory)?;
                let record = json!({"intent":intent,"request":original});
                match converyn_runtime::credentials::pki::private(
                    &saved,
                    &serde_json::to_vec_pretty(&record)?,
                ) {
                    Ok(()) => {}
                    Err(error) if saved.exists() => {
                        let prior: Value = serde_json::from_slice(&bounded_input(&saved)?)?;
                        ensure!(prior == record, "request file conflict: {error}");
                    }
                    Err(error) => return Err(error),
                }
                original
            };
            if !output::is_json() {
                eprintln!(
                    "Instance: {}  Workflow: {}  Preset: {}",
                    api.cluster, id, request.target
                );
            }
            let submitted = call(
                &api,
                &format!("{path}/runs"),
                Some(&serde_json::to_value(request)?),
            )
            .await?;
            let follow = wait;
            if !follow || !output::is_json() {
                print(&submitted)?;
            }
            if !output::is_json() {
                eprintln!(
                    "Follow: converyn watch runs {run_id}\nLogs:   converyn logs runs {run_id} --follow"
                );
                eprintln!(
                    "Open:   {}/#/runs/{run_id}",
                    api.servers.first().map(String::as_str).unwrap_or("")
                );
            }
            if follow {
                let (code, value) =
                    super::execution::watch(&api, &run_id, timeout_seconds.unwrap_or(1800)).await?;
                print(&value)?;
                if code != 0 {
                    std::process::exit(i32::from(code));
                }
            }
            Ok(())
        }
    }
}

fn prepare_bundle(
    connection: &Connection,
    signing: &Signing,
    identity_uid: &str,
) -> Result<people::Credential> {
    let authority = converyn_runtime::credentials::pki::Authority {
        cert: signing.authority.join("ca.crt"),
        key: signing.authority.join("ca.key"),
        leaf_lifetime_seconds: lifetime(signing)?,
    };
    let signed = authority.issue_personal(&signing.output, identity_uid)?;
    let output = signing.output.canonicalize()?;
    let token = converyn_runtime::credentials::pki::random_token()?;
    let cert = converyn_runtime::credentials::pki::info(&signed.certificate_pem)?;
    let original = connection.config.load()?;
    // The browser needs the controller's trust root, not the client-only issuer
    // as its root CA. The leaf file already carries the personal issuer chain.
    let chain = original.ca_pem()?.into_bytes();
    ensure!(
        !String::from_utf8_lossy(&chain).contains("PRIVATE KEY"),
        "client CA file must contain only public certificates"
    );
    let private = converyn_runtime::credentials::pki::private;
    private(&output.join("ca.crt"), &chain)?;
    private(&output.join("token"), token.as_bytes())?;
    converyn_runtime::interface::profile::write(
        &output.join("config.json"),
        &original.cluster,
        converyn_runtime::interface::client::Config {
            servers: original.servers.clone(),
            cluster: original.cluster.clone(),
            token_file: output.join("token"),
            tls: converyn_runtime::credentials::security::Tls {
                cert: output.join("tls.crt"),
                key: output.join("tls.key"),
                ca: output.join("ca.crt"),
            },
            embedded: None,
        },
    )?;
    let credential = people::Credential {
        uid: uid("credential")?,
        identity_uid: identity_uid.into(),
        certificate_pem: signed.certificate_pem,
        fingerprint: cert.fingerprint,
        token_sha256: hex::encode(Sha256::digest(token.as_bytes())),
        not_before_ms: cert.not_before_ms,
        expires_ms: cert.expires_ms,
        revoked_ms: None,
        overlap_until_ms: None,
    };
    private(
        &output.join("credential.json"),
        &serde_json::to_vec_pretty(
            &json!({"identity_uid":identity_uid,"credential_uid":credential.uid,"fingerprint":credential.fingerprint,"expires_ms":credential.expires_ms}),
        )?,
    )?;
    private(&output.join("README.txt"),b"Personal credentials: keep this directory private. CLI: converyn config import config.json --name NAME. config.json is self-contained and includes your private key and token. Import a PKCS#12 export into your browser and use the matching token file. A credential rotation changes both certificate and token.\n")?;
    Ok(credential)
}
async fn register_bundle(
    connection: &Connection,
    output: &Path,
    change: &people::Change,
) -> Result<()> {
    converyn_runtime::credentials::pki::private(
        &output.join("operation.json"),
        &serde_json::to_vec_pretty(change)?,
    )?;
    resume(connection, output).await
}
async fn resume(connection: &Connection, output: &Path) -> Result<()> {
    converyn_runtime::credentials::security::private_state_directory(&output.canonicalize()?)?;
    let new_config = Connection {
        config: super::configuration::Source::explicit(output.join("config.json")),
    };
    let pending: people::Change =
        serde_json::from_slice(&bounded_input(&output.join("operation.json"))?)?;
    let credential = match &pending {
        people::Change::Create { credential, .. } | people::Change::Issue { credential, .. } => {
            credential
        }
        _ => anyhow::bail!("not a credential operation"),
    };
    if let Ok(value) = call(&client(&new_config).await?, "/api/v1/identity/me", None).await
        && value["credential"]["uid"] == credential.uid
    {
        return print(
            &json!({"identity_uid":credential.identity_uid,"credential_uid":credential.uid,"output":output,"registered":true}),
        );
    }
    let result = call(
        &client(connection).await?,
        "/api/v1/identities",
        Some(&serde_json::to_value(&pending)?),
    )
    .await?;
    print(
        &json!({"result":result,"identity_uid":credential.identity_uid,"credential_uid":credential.uid,"output":output,"registered":true}),
    )
}
pub(crate) async fn identity(command: IdentityCommands) -> Result<()> {
    match command {
        IdentityCommands::Create {
            connection,
            signing,
            name,
            super_admin,
        } => {
            let identity = people::Identity {
                uid: uid("user")?,
                name,
                disabled: false,
                revision: 1,
            };
            let credential = prepare_bundle(&connection, &signing, &identity.uid)?;
            let change = people::Change::Create {
                super_admin,
                identity,
                credential,
                request_id: uid("operation")?,
                now_ms: converyn_runtime::executors::host::now_ms(),
            };
            register_bundle(&connection, &signing.output, &change).await
        }
        IdentityCommands::Update {
            connection,
            id,
            name,
        } => update(&connection, &id, Some(name), None).await,
        IdentityCommands::Disable { connection, id } => {
            update(&connection, &id, None, Some(true)).await
        }
        IdentityCommands::Enable { connection, id } => {
            update(&connection, &id, None, Some(false)).await
        }
    }
}
async fn update(
    connection: &Connection,
    id: &str,
    name: Option<String>,
    disabled: Option<bool>,
) -> Result<()> {
    let api = client(connection).await?;
    let users = identities(&api).await?;
    let old = users.get(id).context("identity not found")?;
    let change = people::Change::Update {
        uid: id.into(),
        name: name.unwrap_or_else(|| old.name.clone()),
        disabled: disabled.unwrap_or(old.disabled),
        expected_revision: old.revision,
    };
    print(
        &call(
            &api,
            "/api/v1/identities",
            Some(&serde_json::to_value(change)?),
        )
        .await?,
    )
}
pub(crate) async fn certificates(command: CredentialCommands) -> Result<()> {
    match command {
        CredentialCommands::Create {
            connection,
            signing,
            identity,
        } => issue(&connection, &signing, &identity, false, 0).await,
        CredentialCommands::Rotate {
            connection,
            signing,
            identity,
            overlap,
        } => issue(&connection, &signing, &identity, true, duration(&overlap)?).await,
        CredentialCommands::Delete { connection, id } => {
            let api = client(&connection).await?;
            let credentials = call(&api, "/api/v1/credentials", None).await?;
            let c = credentials["credentials"]
                .as_array()
                .context("invalid credential list")?
                .iter()
                .find(|c| c["uid"] == id)
                .context("credential not found")?;
            let users = identities(&api).await?;
            let i = users
                .get(c["identity_uid"].as_str().unwrap())
                .context("identity not found")?;
            let change = people::Change::Revoke {
                uid: id,
                expected_revision: i.revision,
                now_ms: converyn_runtime::executors::host::now_ms(),
            };
            print(
                &call(
                    &api,
                    "/api/v1/credentials",
                    Some(&serde_json::to_value(change)?),
                )
                .await?,
            )
        }
        CredentialCommands::Resume { connection, output } => resume(&connection, &output).await,
        CredentialCommands::Export(args) => super::certificate_export::export(args).await,
    }
}
async fn issue(
    connection: &Connection,
    signing: &Signing,
    id: &str,
    rotate: bool,
    overlap: u64,
) -> Result<()> {
    ensure!(
        overlap <= people::MAX_OVERLAP,
        "overlap cannot exceed seven days"
    );
    let api = client(connection).await?;
    let users = identities(&api).await?;
    let identity = users.get(id).context("identity not found")?;
    ensure!(!identity.disabled, "identity is disabled");
    let credential = prepare_bundle(connection, signing, id)?;
    let change = people::Change::Issue {
        credential,
        expected_revision: identity.revision,
        rotate,
        overlap_seconds: overlap,
        request_id: uid("operation")?,
        now_ms: converyn_runtime::executors::host::now_ms(),
    };
    register_bundle(connection, &signing.output, &change).await
}
