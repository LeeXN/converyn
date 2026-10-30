//! Run execution, evidence queries and local plan compilation.
use super::*;
pub(super) async fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Wait {
            config,
            run_id,
            timeout_seconds,
        } => {
            let c = config.load()?;
            let uid = run_id;
            let (code, value) = watch(&c.client()?, &uid, timeout_seconds).await?;
            output::print(&value)?;
            if code != 0 {
                use std::io::Write;
                std::io::stdout().flush()?;
                std::process::exit(i32::from(code));
            }
            Ok(())
        }
        Commands::History { config, file } => {
            let c = config.load()?;
            let change: converyn_core::orchestration::history::Change =
                serde_yaml::from_slice(&bounded_input(&file)?)?;
            let value = converyn_runtime::interface::client::request(
                &c.client()?,
                reqwest::Method::POST,
                "/api/v1/history",
                Some(&serde_json::to_value(change)?),
            )
            .await?;
            output::print(&value)?;
            Ok(())
        }
        Commands::Pause {
            config,
            run_id,
            expected_revision,
            resume,
        } => {
            ensure!(
                converyn_core::model::domain::valid_id(&run_id),
                "invalid Run id"
            );
            let c = config.load()?;
            let api = c.client()?;
            let expected_revision = match expected_revision {
                Some(r) => r,
                None => converyn_runtime::interface::client::request(
                    &api,
                    reqwest::Method::GET,
                    &format!("/api/v1/runs/{run_id}"),
                    None,
                )
                .await?["revision"]
                    .as_u64()
                    .context("missing run revision")?,
            };
            output::print(&converyn_runtime::interface::client::request(
                    &api,
                    reqwest::Method::POST,
                    &format!("/api/v1/runs/{run_id}/pause"),
                    Some(
                        &serde_json::json!({"paused":!resume,"expected_revision":expected_revision})
                    )
                )
                .await?)?;
            Ok(())
        }
        Commands::Logs {
            config,
            run_id,
            options,
        } => super::task_logs::run(config, run_id, options).await,
        Commands::Status { config } => super::query::overview(config).await,
        Commands::Cancel { config, run_id } => {
            anyhow::ensure!(
                converyn_core::model::domain::valid_id(&run_id),
                "invalid run id"
            );
            let c = config.load()?;
            output::print(
                &converyn_runtime::interface::client::request(
                    &c.client()?,
                    reqwest::Method::POST,
                    &format!("/api/v1/runs/{run_id}/cancel"),
                    Some(&serde_json::json!({})),
                )
                .await?,
            )?;
            Ok(())
        }
        Commands::Plan { file } => {
            let mut bytes = Vec::new();
            std::fs::File::open(file)
                .context("read plan input")?
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 2 * 1024 * 1024, "input exceeds 2 MiB limit");
            let input: PlanInput = serde_yaml::from_slice(&bytes)?;
            let plan = input.compile()?;
            eprintln!("Offline compilation: platform node-group membership has not been verified.");
            output::print(&plan)?;
            Ok(())
        }
        _ => unreachable!("command dispatched to wrong family"),
    }
}

/// Human progress goes to stderr; stdout remains one final result in JSON mode.
pub(super) async fn watch(
    api: &converyn_runtime::nodes::agent::Client,
    uid: &str,
    timeout: u64,
) -> Result<(u8, serde_json::Value)> {
    let waiting = converyn_runtime::interface::client::wait_with(api, uid, timeout, |value| {
        if !output::is_json() {
            let completed = value["attempts"]
                .as_object()
                .map(|a| {
                    a.values()
                        .filter(|v| {
                            matches!(
                                v["status"].as_str(),
                                Some("succeeded" | "failed" | "canceled")
                            )
                        })
                        .count()
                })
                .unwrap_or(0);
            let total = value["attempts"].as_object().map_or(0, |a| a.len());
            eprintln!(
                "{}  {completed}/{total} steps  {uid}",
                value["status"].as_str().unwrap_or("unknown")
            );
            if let Some(work) = value["plan"]["work"].as_array() {
                for w in work {
                    let a = &value["attempts"][w["id"].as_str().unwrap_or("")];
                    if matches!(a["status"].as_str(), Some("failed" | "needs_attention")) {
                        eprintln!("  {} / {}: {}", w["step_id"], w["node_uid"], a["reason"]);
                    }
                }
            }
        }
    });
    tokio::select! { result = waiting => result, _ = tokio::signal::ctrl_c() => {
        if !output::is_json() { eprintln!("Stopped following. The task continues. Resume: converyn watch runs {uid}"); }
        Ok((130, serde_json::json!({"run_id":uid,"following_interrupted":true})))
    }}
}
