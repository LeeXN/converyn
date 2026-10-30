//! identity 命令实现。
use super::*;
pub(super) async fn run(command: Commands) -> Result<()> {
    match command {
        Commands::RetireNode {
            config,
            node,
            expected_fingerprint,
            evidence,
        } => {
            ensure!(
                converyn_core::model::domain::valid_id(&node)
                    && converyn_core::identities::registration::valid_sha(&expected_fingerprint),
                "invalid Node or certificate fingerprint"
            );
            let c = config.load()?;
            let result=converyn_runtime::interface::client::request(&c.client()?,reqwest::Method::POST,"/api/v1/identity/retire",Some(&serde_json::json!({"node_uid":node,"expected_fingerprint":expected_fingerprint,"evidence":evidence}))).await?;
            output::print(&result)?;
            Ok(())
        }
        Commands::Init {
            directory,
            cluster,
            members,
            project,
            business_cluster,
            targets,
            leaf_lifetime_seconds,
        } => {
            converyn_runtime::controller::initialization::create_with_lifetime(
                &directory,
                &cluster,
                &members,
                &project,
                &business_cluster,
                &targets,
                leaf_lifetime_seconds,
            )?;
            output::print(&format!(
                "Private cluster bundle written to {}",
                directory.display()
            ))?;
            Ok(())
        }
        Commands::Initialize { config, members } => {
            let c = config.load()?;
            let value: serde_json::Value = serde_json::from_slice(&bounded_input(&members)?)?;
            output::print(
                &converyn_runtime::interface::client::request(
                    &c.client()?,
                    reqwest::Method::POST,
                    "/api/v1/controlplane/init",
                    Some(&value),
                )
                .await?,
            )?;
            Ok(())
        }
        Commands::Bootstrap {
            config,
            node,
            output,
            ttl_seconds,
            recover,
        } => {
            converyn_runtime::nodes::enrollment::bootstrap_with_config(
                config.load()?,
                &node,
                &output,
                ttl_seconds,
                recover,
            )
            .await?;
            output::print(&format!(
                "Bootstrap credentials written to {}",
                output.display()
            ))?;
            Ok(())
        }
        Commands::Enroll {
            bootstrap,
            state_dir,
            output,
            template,
            hardware_id_file,
        } => {
            converyn_runtime::nodes::enrollment::enroll(
                &bootstrap,
                &state_dir,
                &output,
                template.as_deref(),
                hardware_id_file.as_deref(),
            )
            .await?;
            output::print(&format!(
                "Enrolled Agent configuration written to {}",
                output.display()
            ))?;
            Ok(())
        }
        Commands::Renew { config } => {
            let cfg: converyn_runtime::nodes::agent::Config =
                serde_json::from_slice(&std::fs::read(config)?)?;
            let credential = converyn_runtime::nodes::enrollment::active(&cfg)?
                .context("Agent is not enrolled")?;
            let api = converyn_runtime::nodes::agent::Client {
                preferred: Default::default(),
                http: std::sync::Arc::new(std::sync::RwLock::new(credential.tls.client()?)),
                fingerprint: std::sync::Arc::new(std::sync::RwLock::new(Some(
                    credential.fingerprint,
                ))),
                servers: cfg.servers.clone(),
                cluster: cfg.cluster.clone(),
                token: converyn_runtime::credentials::security::secret(&credential.token_file)?,
                hardware: converyn_runtime::executors::host::hardware(
                    cfg.hardware_id_file.as_deref(),
                )?,
                session: None,
            };
            let renewed = converyn_runtime::nodes::enrollment::renew(&cfg, &api, true).await?;
            output::print(&serde_json::json!({"renewed":renewed,"node_uid":cfg.node_uid}))?;
            Ok(())
        }
        Commands::PkiInit { directory, name } => {
            converyn_runtime::credentials::pki::create_authority(&directory, &name)?;
            output::print(&format!("Authority written to {}", directory.display()))?;
            Ok(())
        }
        Commands::PkiIssue {
            authority,
            directory,
            name,
            ip,
            leaf_lifetime_seconds,
        } => {
            let issuer = converyn_runtime::credentials::pki::Authority {
                cert: authority.join("ca.crt"),
                key: authority.join("ca.key"),
                leaf_lifetime_seconds,
            };
            let cert = issuer.issue(&directory, &name, &ip)?;
            output::print(
                &serde_json::json!({"directory":directory,"fingerprint":cert.fingerprint,"expires_ms":cert.expires_ms}),
            )?;
            Ok(())
        }
        _ => unreachable!("command dispatched to wrong family"),
    }
}
