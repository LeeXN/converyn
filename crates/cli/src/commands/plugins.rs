//! plugins 命令实现。
use super::*;
pub(super) async fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Plugins { command } => {
            match command {
                PluginCommands::Prepare {
                    id,
                    action,
                    input,
                    output,
                } => {
                    let input: serde_json::Value = serde_yaml::from_slice(&bounded_input(&input)?)?;
                    converyn_runtime::plugins::registry().get(&id)?.prepare(
                        &action,
                        input,
                        &output,
                        &converyn_runtime::plugins::Preparation,
                    )?;
                    output::print(&serde_json::json!({"prepared":true,"directory":output}))?;
                }
                PluginCommands::Call { id, action, spec } => {
                    let spec: serde_json::Value = serde_yaml::from_slice(&bounded_input(&spec)?)?;
                    let call = converyn_runtime::plugins::registry()
                        .get(&id)?
                        .compile(&action, spec)?;
                    call.validate()?;
                    output::print(&serde_json::json!({"kind":"plugin","call":call}))?;
                }
                PluginCommands::Expand {
                    id,
                    action,
                    input,
                    output,
                } => {
                    let input: serde_json::Value = serde_yaml::from_slice(&bounded_input(&input)?)?;
                    let expanded = converyn_runtime::plugins::registry()
                        .get(&id)?
                        .expand(&action, input)?;
                    ensure!(!output.exists(), "refusing to overwrite a plugin expansion");
                    std::fs::create_dir_all(output.join("definitions"))?;
                    for (sha, content) in &expanded.definitions {
                        use sha2::{Digest, Sha256};
                        ensure!(
                            *sha == hex::encode(Sha256::digest(content)),
                            "invalid embedded script digest"
                        );
                        converyn_runtime::credentials::pki::private(
                            &output.join("definitions").join(format!("{sha}.txt")),
                            content.as_bytes(),
                        )?;
                    }
                    let playbook = serde_json::json!({"api_version":"converyn/v1","name":format!("{}-{}",id,action),"steps":expanded.steps});
                    converyn_runtime::credentials::pki::private(
                        &output.join("playbook.json"),
                        &serde_json::to_vec_pretty(&playbook)?,
                    )?;
                    converyn_runtime::credentials::pki::private(
                        &output.join("manifest.json"),
                        &serde_json::to_vec_pretty(&expanded)?,
                    )?;
                    output::print(
                        &serde_json::json!({"expanded":true,"directory":output,"plugin":expanded.plugin}),
                    )?;
                }
            }
            Ok(())
        }
        Commands::Targets { config, file } => {
            management(config, file, "/api/v1/targets/commands").await
        }
        _ => unreachable!("command dispatched to wrong family"),
    }
}
