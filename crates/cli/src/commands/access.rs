//! Effective authorization and optimistic, audited role administration.
use super::*;
use serde_json::json;
#[derive(Subcommand)]
pub(crate) enum AuthCommands {
    /// Export GUI login instructions and optional credentials for the selected personal identity.
    #[command(after_help = "Examples:
  converyn auth gui-login --output-dir ./gui-login
  converyn --context alice auth gui-login --output-dir ./alice-gui --with-credentials
  converyn auth gui-login --output-dir ./gui-login --language en
The command confirms a named personal identity through mTLS. Default output contains README.md and ca.crt only. --with-credentials also exports the selected user's client.p12 and token.txt, and requires installed OpenSSL. Passwords and credential contents never appear in README.md or command output. Use the intended recipient's --config/--context; this command does not create or rotate identities. Existing output directories are never replaced.")]
    GuiLogin(super::gui_login::Args),
    /// Show this identity's effective bindings and their source.
    Status,
    /// Check a workflow or platform capability without performing it.
    CanI {
        #[arg(value_parser=["view","preview","execute","maintain","control","platform","access"])]
        action: String,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long)]
        preset: Option<String>,
    },
    /// List groups, role bindings and the current access revision.
    List,
    /// Apply one typed access change from YAML/JSON, including expected_revision.
    Apply {
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Assign a predefined role to a person or group.
    Grant {
        id: String,
        #[arg(long, conflicts_with = "group", required_unless_present = "group")]
        /// Person UID or unique display name.
        person: Option<String>,
        #[arg(long, conflicts_with = "person")]
        /// Group UID or unique display name.
        group: Option<String>,
        #[arg(long,value_parser=["viewer","executor","maintainer","platform_operator","access_admin","super_admin"])]
        role: String,
        #[arg(long)]
        workflow: Option<String>,
        #[arg(long = "preset")]
        presets: Vec<String>,
    },
    /// Remove a role binding.
    Revoke { id: String },
    /// Create or replace a user group and its complete membership.
    Group {
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long = "member")]
        members: Vec<String>,
    },
    /// Diagnose the selected context and report safe next commands.
    Doctor,
}
pub(super) async fn run(source: ClientSource, c: AuthCommands) -> Result<()> {
    let c = match c {
        AuthCommands::GuiLogin(mut args) => {
            args.connection.config = source;
            return super::gui_login::run(args).await;
        }
        other => other,
    };
    let config = source.load()?;
    let api = config.clone().client()?;
    let request = |method, path: String, body: Option<serde_json::Value>| {
        let api = &api;
        async move {
            converyn_runtime::interface::client::request(api, method, &path, body.as_ref()).await
        }
    };
    let get = reqwest::Method::GET;
    let post = reqwest::Method::POST;
    let value = match c {
        AuthCommands::GuiLogin(_) => unreachable!(),
        AuthCommands::Status => {
            request(get, "/api/v1/access/me".into(), None::<serde_json::Value>).await?
        }
        AuthCommands::List => request(get, "/api/v1/access".into(), None).await?,
        AuthCommands::CanI {
            action,
            workflow,
            preset,
        } => {
            let mut url = reqwest::Url::parse("https://local/api/v1/access/check")?;
            url.query_pairs_mut().append_pair("action", &action);
            if let Some(w) = workflow {
                url.query_pairs_mut().append_pair("workflow", &w);
            }
            if let Some(p) = preset {
                url.query_pairs_mut().append_pair("preset", &p);
            }
            let v = request(
                get,
                format!("{}?{}", url.path(), url.query().unwrap_or("")),
                None,
            )
            .await?;
            output::print(&v)?;
            if v["allowed"] != true {
                std::process::exit(1)
            }
            return Ok(());
        }
        AuthCommands::Doctor => {
            let me = request(get, "/api/v1/identity/me".into(), None).await?;
            json!({"connected":true,"cluster":config.cluster,"servers":config.servers,"identity":me["identity"],"capabilities":me["capabilities"],"next":"converyn get workflows; converyn auth status; converyn auth gui-login --output-dir ./gui-login"})
        }
        AuthCommands::Apply { file } => {
            let change: converyn_core::identities::access::Change =
                serde_yaml::from_slice(&super::bounded_input(&file)?)?;
            request(
                post,
                "/api/v1/access".into(),
                Some(serde_json::to_value(change)?),
            )
            .await?
        }
        c => {
            let access = request(get, "/api/v1/access".into(), None).await?;
            let revision = access["revision"].clone();
            let mut change = match c {
                AuthCommands::Grant {
                    id,
                    person,
                    group,
                    role,
                    workflow,
                    presets,
                } => {
                    let subject = if let Some(p) = person {
                        let people =
                            request(reqwest::Method::GET, "/api/v1/identities".into(), None)
                                .await?;
                        let (uid, _) = super::query::resolve_identity(&people, &p)?;
                        json!({"kind":"person","uid":uid})
                    } else {
                        let name = group.unwrap();
                        let groups = access["groups"].as_object().context("invalid group list")?;
                        let uid = if groups.contains_key(&name) {
                            name
                        } else {
                            let mut matches = groups.iter().filter(|(_, g)| g["name"] == name);
                            let (uid, _) = matches
                                .next()
                                .context("group not found; use converyn auth list")?;
                            ensure!(
                                matches.next().is_none(),
                                "group name is ambiguous; use its UID"
                            );
                            uid.clone()
                        };
                        json!({"kind":"group","uid":uid})
                    };
                    json!({"action":"put_binding","binding":{"uid":id,"subject":subject,"role":role,"workflow":workflow,"presets":presets}})
                }
                AuthCommands::Revoke { id } => json!({"action":"delete_binding","uid":id}),
                AuthCommands::Group { id, name, members } => {
                    let people =
                        request(reqwest::Method::GET, "/api/v1/identities".into(), None).await?;
                    let members = members
                        .iter()
                        .map(|p| {
                            super::query::resolve_identity(&people, p)
                                .map(|(uid, _)| uid.to_owned())
                        })
                        .collect::<Result<Vec<_>>>()?;
                    json!({"action":"put_group","group":{"uid":id,"name":name,"members":members}})
                }
                _ => unreachable!(),
            };
            change["expected_revision"] = revision;
            request(post, "/api/v1/access".into(), Some(change)).await?
        }
    };
    output::print(&value)
}
