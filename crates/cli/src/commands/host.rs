//! host 命令实现。
use super::*;
pub(super) async fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Emit { context, key } => {
            let result = (|| -> Result<()> {
                let mut bytes = Vec::new();
                std::io::stdin()
                    .lock()
                    .take((converyn_core::model::outputs::MAX_DOCUMENT_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)?;
                if let Some(key) = key {
                    ensure!(
                        converyn_core::model::variables::name_valid(&key),
                        "invalid output key"
                    );
                    bytes =
                        serde_json::to_vec(&serde_json::json!({key:String::from_utf8(bytes)?}))?;
                }
                if context {
                    converyn_runtime::executors::outputs::emit_context(bytes.as_slice())
                } else {
                    converyn_runtime::executors::outputs::emit(bytes.as_slice())
                }
            })();
            if result.is_err() {
                converyn_runtime::executors::outputs::mark_invalid();
            }
            result?;
            Ok(())
        }
        Commands::Invoke {
            environments,
            fixed,
            script,
            publish_context,
        } => {
            let code = converyn_runtime::executors::outputs::invoke(
                &environments,
                &fixed,
                &script,
                publish_context,
            )?;
            if code != 0 {
                std::process::exit(code)
            }
            Ok(())
        }
        Commands::TaskContext { get, raw } => {
            let value = converyn_runtime::executors::outputs::context_value(&get)?;
            if let Some(value) = value {
                if raw {
                    println!(
                        "{}",
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string())
                    );
                } else {
                    output::print(&value)?;
                }
            }
            Ok(())
        }
        Commands::UninstallLocal { node } => {
            converyn_runtime::nodes::installation::uninstall_local(&node)?;
            output::print(
                &"Agent service removed. Execution state, credentials, applications and binary retained in place.",
            )?;
            Ok(())
        }
        Commands::InstallLocal {
            hardware_id_file,
            bootstrap,
            bootstrap_stdin,
        } => {
            let bytes = if let Some(path) = bootstrap {
                converyn_runtime::credentials::security::secret(&path)?.into_bytes()
            } else {
                ensure!(bootstrap_stdin, "provide a private bootstrap file or stdin");
                let mut bytes = Vec::new();
                std::io::stdin()
                    .take(1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                bytes
            };
            converyn_runtime::nodes::installation::install_local(
                &bytes,
                None,
                hardware_id_file.as_deref(),
            )
            .await?;
            output::print(&"Agent installed; authenticated heartbeat confirmed")?;
            Ok(())
        }
        Commands::InstallSsh {
            hardware_id_file,
            host,
            user,
            port,
            identity_file,
            known_hosts,
            binary,
            bootstrap,
        } => {
            converyn_runtime::nodes::installation::install_ssh(
                converyn_runtime::nodes::installation::SshInstall {
                    hardware_id_file,
                    host,
                    user,
                    port,
                    identity: identity_file,
                    known_hosts,
                    binary: binary.unwrap_or(std::env::current_exe()?),
                    bootstrap,
                },
            )?;
            output::print(&"Remote Agent installation confirmed")?;
            Ok(())
        }
        Commands::AgentInfo { state_dir } => {
            let value: serde_json::Value =
                serde_json::from_slice(&bounded_input(&state_dir.join("status.json"))?)?;
            output::print(&value)?;
            Ok(())
        }
        Commands::TaskEnter {
            privileged,
            uid,
            gid,
            script,
            journal,
        } => converyn_runtime::executors::execution::enter(uid, gid, &script, &journal, privileged),
        Commands::TaskComplete { journal, unit } => {
            converyn_runtime::executors::runner::complete_task(&journal, &unit)
        }
        Commands::Agent { config } => {
            converyn_runtime::nodes::agent::serve(
                serde_json::from_str(&std::fs::read_to_string(&config)?)?,
                &config,
            )
            .await
        }
        Commands::RunScript {
            state_dir,
            operation_id,
            script,
            sha256,
            timeout,
        } => {
            let receipt = converyn_runtime::executors::runner::run_script(
                &state_dir,
                &operation_id,
                &script,
                &sha256,
                timeout,
            )?;
            output::print(&receipt)?;
            if receipt.outcome != converyn_runtime::executors::runner::Outcome::Succeeded {
                std::process::exit(2);
            }
            Ok(())
        }
        Commands::Fetch {
            url,
            sha256,
            size,
            cache_dir,
            attempts,
            bytes_per_second,
        } => {
            let result = fetch(
                &url,
                Artifact { sha256, size },
                &cache_dir,
                attempts,
                bytes_per_second,
            )
            .await?;
            output::print(&result)?;
            Ok(())
        }
        _ => unreachable!("command dispatched to wrong family"),
    }
}
