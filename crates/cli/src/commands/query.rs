//! Resource discovery and cluster overview, using authenticated API reads.
use super::*;
use serde_json::{Value, json};

#[derive(Clone, Copy, clap::ValueEnum, Debug)]
pub(crate) enum Resource {
    Workflows,
    Runs,
    Nodes,
    NodeGroups,
    Identities,
    Credentials,
    Members,
    Targets,
    Plugins,
}
impl Resource {
    fn path(self) -> &'static str {
        match self {
            Self::Workflows => "/api/v1/workflows",
            Self::Runs => "/api/v1/runs",
            Self::Nodes => "/api/v1/nodes",
            Self::NodeGroups => "/api/v1/node-groups",
            Self::Identities => "/api/v1/identities",
            Self::Credentials => "/api/v1/credentials",
            Self::Members => "/api/v1/controlplane/members",
            Self::Targets => "/api/v1/targets",
            Self::Plugins => "/api/v1/plugins",
        }
    }
    fn paginated(self) -> bool {
        matches!(
            self,
            Self::Workflows | Self::Runs | Self::Nodes | Self::NodeGroups | Self::Targets
        )
    }
}
#[derive(clap::Args)]
pub(crate) struct GetArgs {
    #[command(subcommand)]
    command: GetResource,
}
#[derive(clap::Subcommand)]
enum GetResource {
    /// List Secret names and versions without exposing values.
    Secrets { name: Option<String> },
    /// List immutable artifacts uploaded to Converyn.
    Artifacts,
    /// List workflow names, targets and revisions; accepts an ID for details.
    Workflows(WorkflowArgs),
    /// List Runs, newest submission first (unknown submission times last); accepts an ID for details.
    #[command(
        after_help = "Run detail columns: STEP / NODE / PHASE / OPERATION / STATUS / EXIT / REASON. Use -o wide to include full Work IDs.
Valid states: pending, preparing, paused, authorized, running, cancel_requested, needs_attention, succeeded, failed, canceled.
Logs: converyn logs runs RUN --step verify --follow
Pagination: default reads all pages; --limit returns one page; reuse --cursor with the same --status."
    )]
    Runs(RunArgs),
    /// List node heartbeat, IP, CPU, memory, GPU and OS; accepts an ID for grouped asset details.
    #[command(after_help = "Examples:
  converyn get nodes
  converyn get nodes NODE
  converyn get nodes -o wide
  converyn get nodes -o json
Heartbeat: Online <60s, Offline >=60s, Unknown without an observation.
Inventory is checked every 5 minutes (stale after 10 minutes); resources are sampled every 20 seconds (stale after 60 seconds). Missing fields and collection errors are reported separately.
Pagination: default reads all pages; --limit returns one page; reuse --cursor with the same --status.")]
    Nodes(NodeArgs),
    /// List node groups and revisions; accepts an ID for details.
    NodeGroups(PageArgs),
    /// List people; use an identity UID or unique name for details and their certificates.
    Identities(DetailArgs),
    /// List registered certificates; use a credential UID for metadata, storage and export instructions.
    #[command(after_help = "Examples:
  converyn get credentials
  converyn get credentials --identity OP
  converyn get credentials CREDENTIAL_ID
  converyn export credentials --id CREDENTIAL_ID --output ./public.crt
  converyn export credentials --current --output ./browser.p12
The controller records public certificates and metadata, not client private keys or original client filesystem paths. Detail checks whether the selected local config holds a matching private credential.")]
    Credentials(CredentialArgs),
    /// List control-plane members and voters; accepts a member ID for details.
    Members(DetailArgs),
    /// List external targets; accepts an ID for details.
    Targets(PageArgs),
    /// List installed plugins and fixed contracts; accepts a plugin ID for full details.
    Plugins(PluginArgs),
    /// Read retained history and retention settings.
    History,
    /// Read immutable audit pages.
    Audit {
        #[arg(long, default_value = "0")]
        after: u64,
        #[arg(long, default_value = "100", value_parser = clap::value_parser!(u16).range(1..=100))]
        limit: u16,
    },
}
#[derive(clap::Args, Default)]
struct PluginArgs {
    /// Plugin ID; omit to list.
    id: Option<String>,
    /// Inspect plugins compiled into this binary without contacting a controller.
    #[arg(long)]
    local: bool,
}
#[derive(clap::Args, Default)]
struct CredentialArgs {
    /// Credential UID; omit to list.
    #[arg(conflicts_with = "identity")]
    id: Option<String>,
    /// Filter by identity UID or unique name.
    #[arg(long)]
    identity: Option<String>,
}
#[derive(clap::Args, Default)]
struct DetailArgs {
    /// Resource ID; omit to list.
    id: Option<String>,
}
#[derive(clap::Args, Default)]
struct PageArgs {
    /// Resource ID; omit to list.
    #[arg(conflicts_with_all=["limit","cursor"])]
    id: Option<String>,
    /// Return one page (1..100); default reads all pages.
    #[arg(long, value_parser=clap::value_parser!(u16).range(1..=100))]
    limit: Option<u16>,
    /// Continue one page; preserve filters.
    #[arg(long, requires = "limit")]
    cursor: Option<String>,
}
#[derive(clap::Args, Default)]
struct NodeArgs {
    #[command(flatten)]
    page: PageArgs,
    /// Filter heartbeat observation.
    #[arg(long, value_parser=["online","offline"], conflicts_with="id")]
    status: Option<String>,
}
#[derive(clap::Args, Default)]
struct RunArgs {
    #[arg(long, conflicts_with = "id")]
    workflow: Option<String>,
    #[arg(long, conflicts_with = "id")]
    preset: Option<String>,
    #[arg(long, conflicts_with = "id")]
    mine: bool,
    #[command(flatten)]
    page: PageArgs,
    /// Filter the committed Run state.
    #[arg(long, value_parser=["pending","preparing","paused","authorized","running","cancel_requested","needs_attention","succeeded","failed","canceled"], conflicts_with="id")]
    status: Option<String>,
}
#[derive(clap::Args, Default)]
struct WorkflowArgs {
    #[command(flatten)]
    page: PageArgs,
    #[arg(long, conflicts_with = "id")]
    search: Option<String>,
    #[arg(long, conflicts_with="id", value_parser=["enabled","disabled","all"])]
    status: Option<String>,
}
impl GetArgs {
    fn parts(self) -> (Resource, PageArgs, Option<String>, Option<String>) {
        let mut identity = None;
        let parts = match self.command {
            GetResource::Runs(a) => (Resource::Runs, a.page, a.status),
            GetResource::Nodes(a) => (Resource::Nodes, a.page, a.status),
            GetResource::Workflows(a) => (Resource::Workflows, a.page, a.status),
            GetResource::Identities(a) => (
                Resource::Identities,
                PageArgs {
                    id: a.id,
                    ..Default::default()
                },
                None,
            ),
            GetResource::NodeGroups(a) => (Resource::NodeGroups, a, None),
            GetResource::Targets(a) => (Resource::Targets, a, None),
            GetResource::Credentials(a) => {
                identity = a.identity;
                (
                    Resource::Credentials,
                    PageArgs {
                        id: a.id,
                        ..Default::default()
                    },
                    None,
                )
            }
            GetResource::Members(a) => (
                Resource::Members,
                PageArgs {
                    id: a.id,
                    ..Default::default()
                },
                None,
            ),
            GetResource::Secrets { .. }
            | GetResource::Artifacts
            | GetResource::History
            | GetResource::Audit { .. } => {
                unreachable!()
            }
            GetResource::Plugins(a) => (
                Resource::Plugins,
                PageArgs {
                    id: a.id,
                    ..Default::default()
                },
                None,
            ),
        };
        (parts.0, parts.1, parts.2, identity)
    }
}
async fn read(api: &converyn_runtime::nodes::agent::Client, path: &str) -> Result<Value> {
    converyn_runtime::interface::client::request(api, reqwest::Method::GET, path, None).await
}
pub(super) async fn collection(
    api: &converyn_runtime::nodes::agent::Client,
    resource: Resource,
    status: Option<&str>,
    limit: Option<u16>,
    cursor: Option<&str>,
) -> Result<Value> {
    collection_filtered(api, resource, status, limit, cursor, &[]).await
}
async fn collection_filtered(
    api: &converyn_runtime::nodes::agent::Client,
    resource: Resource,
    status: Option<&str>,
    limit: Option<u16>,
    cursor: Option<&str>,
    filters: &[(String, String)],
) -> Result<Value> {
    if !resource.paginated() {
        return read(api, resource.path()).await;
    }
    let key = resource
        .path()
        .rsplit('/')
        .next()
        .unwrap()
        .replace('-', "_");
    let mut after = cursor.map(str::to_owned);
    let mut result = json!({key.clone(): if matches!(key.as_str(),"runs"|"workflows") { json!([]) } else { json!({}) }, "next_cursor":null});
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let mut url = reqwest::Url::parse("http://localhost")?;
        url.set_path(resource.path());
        url.query_pairs_mut()
            .append_pair("limit", &limit.unwrap_or(100).to_string());
        for (k, v) in filters {
            url.query_pairs_mut().append_pair(k, v);
        }
        if let Some(status) = status {
            url.query_pairs_mut().append_pair("status", status);
        }
        if let Some(cursor) = &after {
            url.query_pairs_mut().append_pair("after", cursor);
        }
        let page = read(
            api,
            &format!("{}?{}", url.path(), url.query().unwrap_or_default()),
        )
        .await?;
        for (name, value) in page.as_object().context("invalid collection response")? {
            if name != &key && name != "next_cursor" {
                result
                    .as_object_mut()
                    .unwrap()
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
        }
        if matches!(key.as_str(), "runs" | "workflows") {
            result[&key].as_array_mut().unwrap().extend(
                page[&key]
                    .as_array()
                    .context("invalid Run list")?
                    .iter()
                    .cloned(),
            );
        } else {
            result[&key]
                .as_object_mut()
                .unwrap()
                .extend(page[&key].as_object().context("invalid Node list")?.clone());
        }
        result["next_cursor"] = page["next_cursor"].clone();
        after = page["next_cursor"].as_str().map(str::to_owned);
        if limit.is_some() || after.is_none() {
            break;
        }
        ensure!(
            seen.insert(after.clone().unwrap()),
            "server returned a repeated pagination cursor"
        );
    }
    Ok(result)
}
pub(super) async fn get(source: ClientSource, args: GetArgs) -> Result<()> {
    match &args.command {
        GetResource::Secrets { name } => {
            let api = source.load()?.client()?;
            let mut value = read(&api, "/api/v1/secrets").await?;
            if let Some(name) = name {
                value["secrets"]
                    .as_array_mut()
                    .context("invalid Secret catalog")?
                    .retain(|v| v["name"] == *name);
            }
            return output::print(&value);
        }
        GetResource::Artifacts => {
            let api = source.load()?.client()?;
            return output::print(&read(&api, "/api/v1/artifacts").await?);
        }
        GetResource::History => {
            let api = source.load()?.client()?;
            return output::print(&read(&api, "/api/v1/history").await?);
        }
        GetResource::Audit { after, limit } => {
            return super::controller::run(Commands::Audit {
                config: source,
                after: *after,
                limit: usize::from(*limit),
            })
            .await;
        }
        GetResource::Plugins(PluginArgs { id, local: true }) => {
            super::ensure_local(&source)?;
            let registry = converyn_runtime::plugins::registry();
            let value = if let Some(id) = id {
                let descriptor = registry
                    .descriptors()
                    .into_iter()
                    .find(|p| p.identity.id == *id)
                    .context("plugin not found")?;
                json!({"plugins": [descriptor], "manifest_sha256": registry.fingerprint()})
            } else {
                json!({"plugins":registry.descriptors(), "manifest_sha256":registry.fingerprint()})
            };
            return output::print(&value);
        }
        _ => {}
    }
    let mut filters = Vec::new();
    let mine = match &args.command {
        GetResource::Runs(a) => {
            if let Some(w) = &a.workflow {
                filters.push(("workflow".into(), w.clone()));
            }
            if let Some(p) = &a.preset {
                filters.push(("preset".into(), p.clone()));
            }
            a.mine
        }
        GetResource::Workflows(a) => {
            if let Some(q) = &a.search {
                filters.push(("search".into(), q.clone()));
            }
            false
        }
        _ => false,
    };
    let (resource, args, status, identity_filter) = args.parts();
    if let Some(id) = &args.id {
        ensure!(
            if matches!(resource, Resource::Identities) {
                !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
            } else {
                converyn_core::model::domain::valid_id(id)
            },
            "invalid resource id"
        );
    }
    let config = source.load()?;
    let api = config.clone().client()?;
    if mine {
        let me = read(&api, "/api/v1/identity/me").await?;
        filters.push((
            "actor".into(),
            me["identity"]["uid"]
                .as_str()
                .context("--mine requires a named identity")?
                .into(),
        ));
    }
    let detail_requested = args.id.is_some();
    let mut selected_id = args.id.clone();
    let mut value = if let Some(id) = args.id {
        match resource {
            Resource::Runs | Resource::Workflows | Resource::Nodes | Resource::Credentials => {
                read(&api, &format!("{}/{id}", resource.path())).await?
            }
            Resource::Identities => {
                let all = collection(&api, resource, None, None, None).await?;
                let (uid, item) = resolve_identity(&all, &id)?;
                selected_id = Some(uid.to_owned());
                let mut credentials = read(&api, Resource::Credentials.path()).await?;
                credentials["credentials"]
                    .as_array_mut()
                    .context("invalid credential list")?
                    .retain(|c| c["identity_uid"] == uid);
                json!({"identities":{uid:item},"credentials":credentials["credentials"]})
            }
            Resource::NodeGroups | Resource::Targets => {
                let all = collection(&api, resource, None, None, None).await?;
                let key = resource
                    .path()
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .replace('-', "_");
                let item = all[&key].get(&id).context("resource not found")?;
                json!({key:{id:item}})
            }
            Resource::Plugins => {
                let all = read(&api, resource.path()).await?;
                let plugin = all["plugins"]
                    .as_array()
                    .context("invalid plugin list")?
                    .iter()
                    .find(|p| p["identity"]["id"] == id)
                    .context("plugin not found")?;
                json!({"plugins":[plugin],"manifest_sha256":all["manifest_sha256"]})
            }
            Resource::Members => {
                let mut all = read(&api, resource.path()).await?;
                let item = all["membership"]["membership"]["nodes"]
                    .get(&id)
                    .context("member not found")?
                    .clone();
                let voter = all["voters"].as_array().is_some_and(|v| {
                    v.iter()
                        .any(|v| v.as_u64().map(|n| n.to_string()).as_deref() == Some(id.as_str()))
                });
                all["member"] = json!({"id":id,"node":item,"voter":voter});
                all
            }
        }
    } else {
        collection_filtered(
            &api,
            resource,
            status.as_deref(),
            args.limit,
            args.cursor.as_deref(),
            &filters,
        )
        .await?
    };
    if let Some(wanted) = identity_filter {
        let identities = read(&api, Resource::Identities.path()).await?;
        let (uid, _) = resolve_identity(&identities, &wanted)?;
        value["credentials"]
            .as_array_mut()
            .context("invalid credential list")?
            .retain(|c| c["identity_uid"] == uid);
    }
    if detail_requested && matches!(resource, Resource::Identities | Resource::Credentials) {
        let local = super::certificate_export::location(&source, &config)?;
        let matched = if matches!(resource, Resource::Credentials) {
            value["credential"]["fingerprint"] == local["fingerprint"]
        } else {
            value["credentials"]
                .as_array()
                .is_some_and(|v| v.iter().any(|c| c["fingerprint"] == local["fingerprint"]))
        };
        value["local_credential"] = if matched {
            let mut local = local;
            local["matches_selected_config"] = json!(true);
            local
        } else {
            json!({"matches_selected_config":false,"selected_config_file":local["config_file"],"reason":"matching private material is not in the selected context; use the matching context or original client bundle"})
        };
    }
    if detail_requested
        && !output::is_json()
        && !matches!(resource, Resource::Runs | Resource::Workflows)
    {
        value["detail_resource"] = json!({"kind":resource.path().rsplit('/').next().unwrap().replace('-', "_"),"id":selected_id});
    }
    output::print(&value)
}
pub(super) fn resolve_identity<'a>(value: &'a Value, wanted: &str) -> Result<(&'a str, &'a Value)> {
    let rows = value["identities"]
        .as_object()
        .context("invalid get identities")?;
    if let Some((id, item)) = rows.get_key_value(wanted) {
        return Ok((id, item));
    }
    let mut matches = rows.iter().filter(|(_, v)| v["name"] == wanted);
    let (id, item) = matches
        .next()
        .context("identity not found; use an identity UID from `converyn get identities`")?;
    ensure!(
        matches.next().is_none(),
        "identity name is ambiguous; use its UID"
    );
    Ok((id, item))
}

pub(super) async fn overview(source: ClientSource) -> Result<()> {
    let config = source.load()?;
    let cluster = config.cluster.clone();
    let servers = config.servers.clone();
    let api = config.client()?;
    let me = read(&api, "/api/v1/identity/me").await?;
    if me["identity"].is_object() && me["capabilities"]["platform"] != true {
        let (runs, workflows) = tokio::try_join!(
            collection(&api, Resource::Runs, None, None, None),
            collection(&api, Resource::Workflows, None, None, None)
        )?;
        return output::print(
            &json!({"cluster":cluster,"servers":servers,"identity":me["identity"],"capabilities":me["capabilities"],"visible_runs":runs["runs"].as_array().map(Vec::len),"visible_workflows":workflows["workflows"].as_array().map(Vec::len),"next":"converyn get workflows; converyn auth status"}),
        );
    }
    // Members and business reads confirm quorum; local metrics alone cannot prove readiness.
    let (members, metrics, nodes, runs, workflows) = tokio::try_join!(
        read(&api, Resource::Members.path()),
        read(&api, "/api/v1/controlplane/metrics"),
        collection(&api, Resource::Nodes, None, None, None),
        collection(&api, Resource::Runs, None, None, None),
        collection(&api, Resource::Workflows, None, None, None),
    )?;
    output::print(&json!({"overview":true,"cluster":cluster,"servers":servers,
        "business_reads":"quorum-confirmed","members":members,"controller_observation":metrics,
        "nodes":nodes["nodes"],"runs":runs["runs"],"workflows":workflows["workflows"]}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::{Path, Query},
        routing::get,
    };
    use std::{
        collections::BTreeMap,
        sync::{Arc, RwLock},
    };
    #[test]
    fn identity_names_are_resolved_uniquely() {
        let value =
            json!({"identities":{"u1":{"name":"OP"},"u2":{"name":"QA"},"u3":{"name":"QA"}}});
        assert_eq!(resolve_identity(&value, "OP").unwrap().0, "u1");
        assert_eq!(resolve_identity(&value, "u2").unwrap().0, "u2");
        assert!(
            resolve_identity(&value, "QA")
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
        assert!(resolve_identity(&value, "missing").is_err());
    }
    #[tokio::test]
    async fn collections_follow_cursors_and_preserve_explicit_pages() {
        let app=Router::new().route("/api/v1/{kind}",get(|Path(kind):Path<String>,Query(q):Query<BTreeMap<String,String>>|async move {
            assert!(q["limit"]=="100" || q["limit"]=="1");
            let second=q.get("after").is_some_and(|v|v=="cursor&+/=");
            let id=if second {"second"} else {"first"};
            let value=if kind=="runs" {json!([{ "id":id,"status":"running" }])} else {json!({id:{"uid":id}})};
            Json(json!({kind.replace('-', "_"):value,"next_cursor":if second {Value::Null} else {json!("cursor&+/=")}}))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = converyn_runtime::nodes::agent::Client {
            preferred: Default::default(),
            fingerprint: Default::default(),
            hardware: None,
            session: None,
            http: Arc::new(RwLock::new(reqwest::Client::new())),
            servers: vec![format!("http://{address}")],
            token: "test".into(),
            cluster: "test".into(),
        };
        for resource in [
            Resource::Runs,
            Resource::Nodes,
            Resource::NodeGroups,
            Resource::Targets,
        ] {
            let all = collection(&client, resource, None, None, None)
                .await
                .unwrap();
            let key = resource
                .path()
                .rsplit('/')
                .next()
                .unwrap()
                .replace('-', "_");
            assert!(all["next_cursor"].is_null());
            assert_eq!(
                if matches!(key.as_str(), "runs" | "workflows") {
                    all[key].as_array().unwrap().len()
                } else {
                    all[key].as_object().unwrap().len()
                },
                2
            );
            let page = collection(&client, resource, None, Some(1), None)
                .await
                .unwrap();
            assert_eq!(page["next_cursor"], "cursor&+/=");
            let page = collection(&client, resource, None, Some(1), Some("cursor&+/="))
                .await
                .unwrap();
            assert!(page["next_cursor"].is_null());
        }
        server.abort();
    }
}
