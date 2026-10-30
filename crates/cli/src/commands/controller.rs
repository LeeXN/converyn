//! controller 命令实现。
use super::*;
pub(super) async fn run(command: Commands) -> Result<()> {
    match command {
        Commands::OutputKey { file, rotate } => {
            let public = converyn_runtime::credentials::outputs::Keyring::generate(&file, rotate)?;
            output::print(&public)?;
            Ok(())
        }
        Commands::ApiSchema => {
            println!("{}", converyn_runtime::interface::contract::OPENAPI);
            Ok(())
        }
        Commands::Audit {
            config,
            after,
            limit,
        } => {
            ensure!((1..=100).contains(&limit), "audit limit must be 1..100");
            let c = config.load()?;
            let path = format!("/api/v1/audit?after={after}&limit={limit}");
            let result = converyn_runtime::interface::client::request(
                &c.client()?,
                reqwest::Method::GET,
                &path,
                None,
            )
            .await?;
            output::print(&result)?;
            Ok(())
        }
        Commands::Members { config, file } => {
            let c = config.load()?;
            let api = c.client()?;
            let intent: converyn_core::orchestration::controlplane::Replace =
                serde_yaml::from_slice(&bounded_input(&file)?)?;
            let value = converyn_runtime::interface::client::request(
                &api,
                reqwest::Method::POST,
                "/api/v1/controlplane/members",
                Some(&serde_json::to_value(intent)?),
            )
            .await?;
            output::print(&value)?;
            Ok(())
        }
        Commands::DeleteNodeGroup {
            config,
            id,
            expected_revision,
            reason,
        } => {
            ensure!(!reason.trim().is_empty(), "reason must not be empty");
            let api = config.load()?.client()?;
            let reply = converyn_runtime::interface::client::request(&api, reqwest::Method::POST,
                "/api/v1/node-groups/commands", Some(&serde_json::json!({"action":"delete_node_group","uid":id,"expected_revision":expected_revision,"reason":reason}))).await?;
            output::print(&reply)?;
            Ok(())
        }
        Commands::NodeGroups { config, file } => {
            management(config, file, "/api/v1/node-groups/commands").await
        }
        Commands::Backup { config, output } => {
            let c = config.load()?;
            let (sha256, cluster) =
                converyn_runtime::interface::client::backup(&c.client()?, &output).await?;
            output::print(&serde_json::json!({"sha256":sha256,"cluster":cluster}))?;
            Ok(())
        }
        Commands::Restore {
            backup,
            data_dir,
            node_id,
            old_control_plane_fenced,
            keyring,
        } => {
            converyn_runtime::storage::backup::restore_with_keyring(
                &backup,
                &data_dir,
                node_id,
                old_control_plane_fenced,
                keyring.as_deref(),
            )
            .await?;
            output::print(
                &"Restored; initialize the replacement quorum, review paused Runs and remove Node maintenance only after reconciliation.",
            )?;
            Ok(())
        }
        Commands::Server {
            config,
            id,
            listen,
            data_dir,
            cluster,
            token,
            auth_file,
            issuer_cert,
            issuer_key,
            leaf_lifetime_seconds,
            tls_cert,
            tls_key,
            tls_ca,
        } => {
            if let Some(file) = config {
                let profile: converyn_runtime::controller::initialization::ServerProfile =
                    serde_json::from_slice(&bounded_input(&file)?)?;
                let base = std::fs::canonicalize(file.parent().context("profile parent missing")?)?;
                return converyn_runtime::controller::server::serve_settings(
                    profile.settings(&base)?,
                )
                .await;
            }
            let id = id.context("controller id required")?;
            let listen = listen.context("listen address required")?;
            let data_dir = data_dir.context("data directory required")?;
            let auth = auth_file
                .map(|p| -> Result<_> {
                    Ok(serde_json::from_str(
                        &converyn_runtime::credentials::security::secret(&p)?,
                    )?)
                })
                .transpose()?;
            let tls = tls_cert.map(|cert| converyn_runtime::credentials::security::Tls {
                cert,
                key: tls_key.unwrap(),
                ca: tls_ca.unwrap(),
            });
            let issuer = issuer_cert.map(|cert| converyn_runtime::credentials::pki::Authority {
                cert,
                key: issuer_key.unwrap(),
                leaf_lifetime_seconds,
            });
            converyn_runtime::controller::server::serve_settings(
                converyn_runtime::controller::server::Settings {
                    log_store: None,
                    output_keyring: None,
                    plugin_connections: Default::default(),
                    id,
                    listen,
                    cluster,
                    token,
                    data_dir,
                    auth,
                    tls,
                    issuer,
                },
            )
            .await
        }
        _ => unreachable!("command dispatched to wrong family"),
    }
}
