//! 单二进制 CLI 装配与分组分派。
use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use converyn_core::model::domain::PlanInput;
use converyn_runtime::artifacts::artifact::{Artifact, fetch};
use std::io::Read;

mod access;
mod certificate_export;
mod configuration;
mod definition;
pub(crate) use configuration::Source as ClientSource;
mod operator;
mod values;
mod verbs;
pub(crate) use definition::{CliCommands, Commands, PluginCommands};
mod controller;
mod execution;
mod groups;
mod gui_login;
mod host;
mod identity;
mod plugins;
mod query;
mod task_logs;
use crate::output;
pub(crate) async fn run(command: CliCommands, source: ClientSource) -> Result<()> {
    let mut command: Commands = command.into();
    bind(&mut command, &source)?;
    match command {
        Commands::RegisterSecret {
            config,
            name,
            version,
            file,
        } => {
            let value = std::fs::read_to_string(&file).context("read private Secret input file")?;
            let api = config.load()?.client()?;
            let reply = converyn_runtime::interface::client::request(
                &api,
                reqwest::Method::POST,
                "/api/v1/secrets",
                Some(&serde_json::json!({"name":name,"version":version,"value":value})),
            )
            .await?;
            output::print(&reply)
        }
        Commands::RegisterArtifact { config, file } => {
            let api = config.load()?.client()?;
            let mut progress = crate::progress::Progress::new(&file);
            let (sha256, size) = converyn_runtime::artifacts::artifact::hash_file_with_progress(
                &file,
                |read, total| progress.hashing(&file, read, total),
            )?;
            progress.uploads(1, size);
            progress.file(&file, size);
            converyn_runtime::artifacts::transfer::upload_with_progress(
                &api,
                &file,
                &Artifact {
                    sha256: sha256.clone(),
                    size,
                },
                |event| progress.transfer(event),
            )
            .await?;
            progress.finish("Artifact registered");
            output::print(&serde_json::json!({"sha256":sha256,"size":size}))
        }
        Commands::Auth { command } => access::run(source, command).await,
        Commands::Get(args) => query::get(source, args).await,
        Commands::Config { command } => configuration::run(source, command),
        Commands::Workflow { command } => operator::workflow(command).await,
        Commands::Identity { command } => operator::identity(command).await,
        Commands::Certificates { command } => operator::certificates(command).await,
        command => run_existing(command).await,
    }
}
fn bind(command: &mut Commands, source: &ClientSource) -> Result<()> {
    match command {
        Commands::RegisterSecret { config, .. }
        | Commands::Audit { config, .. }
        | Commands::RegisterArtifact { config, .. }
        | Commands::Targets { config, .. }
        | Commands::Wait { config, .. }
        | Commands::History { config, .. }
        | Commands::RetireNode { config, .. }
        | Commands::Members { config, .. }
        | Commands::Backup { config, .. }
        | Commands::Pause { config, .. }
        | Commands::Initialize { config, .. }
        | Commands::Bootstrap { config, .. }
        | Commands::Logs { config, .. }
        | Commands::NodeGroups { config, .. }
        | Commands::DeleteNodeGroup { config, .. }
        | Commands::Status { config, .. }
        | Commands::Cancel { config, .. } => {
            *config = source.clone();
        }
        Commands::Workflow { command } => match command {
            operator::WorkflowCommands::Validate { .. } => ensure!(
                source.context.is_none() && source.file.is_none(),
                "--config/--context are not accepted by local tools"
            ),
            operator::WorkflowCommands::Resolve { connection, .. }
            | operator::WorkflowCommands::Register { connection, .. }
            | operator::WorkflowCommands::Enable { connection, .. }
            | operator::WorkflowCommands::Disable { connection, .. } => {
                connection.config = source.clone()
            }
            operator::WorkflowCommands::Run(args) => args.connection.config = source.clone(),
        },
        Commands::Identity { command } => match command {
            operator::IdentityCommands::Create { connection, .. }
            | operator::IdentityCommands::Update { connection, .. }
            | operator::IdentityCommands::Disable { connection, .. }
            | operator::IdentityCommands::Enable { connection, .. } => {
                connection.config = source.clone()
            }
        },
        Commands::Certificates { command } => match command {
            operator::CredentialCommands::Export(args) => args.connection.config = source.clone(),
            operator::CredentialCommands::Create { connection, .. }
            | operator::CredentialCommands::Rotate { connection, .. }
            | operator::CredentialCommands::Delete { connection, .. }
            | operator::CredentialCommands::Resume { connection, .. } => {
                connection.config = source.clone()
            }
        },
        Commands::Plan { .. } | Commands::Plugins { .. } => ensure_local(source)?,
        Commands::Server { config, .. } => {
            ensure!(
                source.context.is_none(),
                "server uses an explicit controller profile, not a client context"
            );
            *config = source.file.clone();
        }
        Commands::Agent { config } | Commands::Renew { config } => {
            ensure!(
                source.context.is_none(),
                "agent/renew use an explicit Node config, not a client context"
            );
            *config = source
                .file
                .clone()
                .context("this host-local command requires --config PATH")?;
        }
        Commands::Auth { .. } | Commands::Config { .. } | Commands::Get(_) => {}
        _ => ensure!(
            source.context.is_none(),
            "--context is only for client commands"
        ),
    }
    Ok(())
}
fn ensure_local(source: &ClientSource) -> Result<()> {
    ensure!(
        source.file.is_none() && source.context.is_none(),
        "--config/--context are not accepted by local tools"
    );
    Ok(())
}
async fn run_existing(command: Commands) -> Result<()> {
    match &command {
        Commands::Auth { .. }
        | Commands::RegisterSecret { .. }
        | Commands::RegisterArtifact { .. }
        | Commands::Config { .. }
        | Commands::Get(_)
        | Commands::Workflow { .. }
        | Commands::Identity { .. }
        | Commands::Certificates { .. } => {
            unreachable!()
        }
        Commands::MigrateAgent { file } => converyn_runtime::nodes::agent::migrate_config(file),
        Commands::Plugins { .. } | Commands::Targets { .. } => plugins::run(command).await,
        Commands::Wait { .. }
        | Commands::History { .. }
        | Commands::Pause { .. }
        | Commands::Logs { .. }
        | Commands::Status { .. }
        | Commands::Cancel { .. }
        | Commands::Plan { .. } => execution::run(command).await,
        Commands::RetireNode { .. }
        | Commands::Init { .. }
        | Commands::Initialize { .. }
        | Commands::Bootstrap { .. }
        | Commands::Enroll { .. }
        | Commands::Renew { .. }
        | Commands::PkiInit { .. }
        | Commands::PkiIssue { .. } => identity::run(command).await,
        Commands::Emit { .. }
        | Commands::Invoke { .. }
        | Commands::TaskContext { .. }
        | Commands::UninstallLocal { .. }
        | Commands::InstallLocal { .. }
        | Commands::InstallSsh { .. }
        | Commands::AgentInfo { .. }
        | Commands::TaskEnter { .. }
        | Commands::TaskComplete { .. }
        | Commands::Agent { .. }
        | Commands::RunScript { .. }
        | Commands::Fetch { .. } => host::run(command).await,
        Commands::OutputKey { .. }
        | Commands::Audit { .. }
        | Commands::ApiSchema
        | Commands::Members { .. }
        | Commands::NodeGroups { .. }
        | Commands::DeleteNodeGroup { .. }
        | Commands::Backup { .. }
        | Commands::Restore { .. }
        | Commands::Server { .. } => controller::run(command).await,
    }
}

fn bounded_input(path: &std::path::Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 2 * 1024 * 1024, "input exceeds 2 MiB limit");
    Ok(bytes)
}
async fn management(config: configuration::Source, file: PathBuf, endpoint: &str) -> Result<()> {
    let api = config.load()?.client()?;
    let body: serde_json::Value = serde_yaml::from_slice(&bounded_input(&file)?)?;
    let value = converyn_runtime::interface::client::request(
        &api,
        reqwest::Method::POST,
        endpoint,
        Some(&body),
    )
    .await?;
    output::print(&value)
}
