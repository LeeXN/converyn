//! Converyn 单二进制入口，具体命令按职责组织。
use anyhow::Result;
use clap::Parser;
mod commands;
mod output;
mod progress;
#[derive(Parser)]
#[command(
    version,
    help_template = include_str!("help.txt"),
    about = "Converyn — durable host delivery controller and autonomous agent",
    after_help = "Start here:\n  converyn status\n  converyn get workflows\n  converyn get runs\n  converyn get nodes\n  converyn run workflows WORKFLOW --wait\n  converyn logs runs RUN_ID\n\nUse -o json for indented JSON. Maintenance commands are under admin."
)]
struct Cli {
    /// Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile.
    #[arg(long, global = true)]
    config: Option<std::path::PathBuf>,
    /// Select a context for this invocation without changing the saved selection.
    #[arg(long, global = true)]
    context: Option<String>,
    /// Output format: human-readable tables/details, or indented JSON for scripts.
    #[arg(short = 'o', long = "output-format", global = true, value_enum, default_value_t = output::Format::Table)]
    output_format: output::Format,
    #[command(subcommand)]
    command: commands::CliCommands,
}

#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    output::set_format(cli.output_format);
    commands::run(
        cli.command,
        commands::ClientSource {
            file: cli.config,
            context: cli.context,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::collections::BTreeSet;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("converyn").chain(args.iter().copied()))
    }

    #[test]
    fn verb_resource_contract_and_help_are_complete() {
        let command = Cli::command();
        command.clone().debug_assert();
        let expected: BTreeSet<_> = [
            "delete", "get", "status", "validate", "register", "create", "update", "enable",
            "disable", "rotate", "revoke", "export", "replace", "retire", "run", "watch", "logs",
            "cancel", "pause", "resume", "resolve", "compile", "prepare", "expand", "config",
            "auth", "admin", "task", "server", "agent",
        ]
        .into_iter()
        .collect();
        let actual: BTreeSet<_> = command
            .get_subcommands()
            .filter(|c| !c.is_hide_set())
            .map(|c| c.get_name())
            .collect();
        assert_eq!(actual, expected);
        let listed: BTreeSet<_> = include_str!("help.txt")
            .lines()
            .filter_map(|line| {
                line.strip_prefix("  ")
                    .and_then(|line| line.split_whitespace().next())
            })
            .filter(|name| *name != "help")
            .collect();
        assert_eq!(listed, expected);
        // Every command, including commands without summaries, has help without loading config.
        fn visit(command: &clap::Command, path: Vec<String>) {
            let mut args = path.clone();
            args.push("--help".into());
            let error = Cli::try_parse_from(&args).err().unwrap();
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp,
                "{args:?}"
            );
            for child in command
                .get_subcommands()
                .filter(|c| !c.is_hide_set() && c.get_name() != "help")
            {
                let mut next = path.clone();
                next.push(child.get_name().into());
                visit(child, next);
            }
        }
        visit(&command, vec!["converyn".into()]);
        for args in [
            vec!["get", "runs", "r", "-o", "json"],
            vec!["get", "plugins", "converyn.kubernetes", "--local"],
            vec!["get", "history"],
            vec!["get", "audit", "--after", "1", "--limit", "2"],
            vec!["validate", "workflows", "-f", "workflow.yaml"],
            vec!["register", "workflows", "-f", "workflow.yaml"],
            vec![
                "create",
                "identities",
                "--name",
                "alice",
                "--authority",
                "ca",
                "--output",
                "out",
            ],
            vec![
                "create",
                "credentials",
                "--identity",
                "alice",
                "--authority",
                "ca",
                "--output",
                "out",
            ],
            vec!["update", "identities", "alice", "--name", "Alice"],
            vec!["update", "node-groups", "-f", "change.yaml"],
            vec!["update", "targets", "-f", "change.yaml"],
            vec!["update", "history", "-f", "change.yaml"],
            vec!["replace", "members", "-f", "change.yaml"],
            vec![
                "retire",
                "nodes",
                "n",
                "--expected-fingerprint",
                "f",
                "--evidence",
                "checked",
            ],
            vec!["enable", "workflows", "w"],
            vec!["disable", "identities", "u"],
            vec![
                "rotate",
                "credentials",
                "--identity",
                "alice",
                "--authority",
                "ca",
                "--output",
                "out",
            ],
            vec!["revoke", "credentials", "c"],
            vec!["resume", "credentials", "--output", "out"],
            vec![
                "export",
                "credentials",
                "--current",
                "--output",
                "browser.p12",
            ],
            vec![
                "export",
                "credentials",
                "--id",
                "c",
                "--output",
                "public.crt",
            ],
            vec![
                "export",
                "credentials",
                "--directory",
                "bundle",
                "--output",
                "browser.p12",
            ],
            vec!["auth", "gui-login", "--output-dir", "guide"],
            vec!["run", "workflows", "w"],
            vec!["run", "workflows", "w", "--dry-run"],
            vec!["run", "workflows", "w", "--wait", "--timeout-seconds", "1"],
            vec!["watch", "runs", "r"],
            vec!["logs", "runs", "r", "--follow"],
            vec!["pause", "runs", "r"],
            vec!["resume", "runs", "r"],
            vec!["cancel", "runs", "r"],
            vec![
                "resolve",
                "runs",
                "r",
                "--work-id",
                "w",
                "--status",
                "failed",
                "--evidence",
                "checked",
            ],
            vec!["compile", "plans", "-f", "plan.yaml"],
            vec![
                "compile",
                "plugins",
                "p",
                "--action",
                "a",
                "--spec",
                "spec.yaml",
            ],
            vec![
                "prepare", "plugins", "p", "--action", "a", "--input", "in.yaml", "--output", "out",
            ],
            vec![
                "expand", "plugins", "p", "--action", "a", "--input", "in.yaml", "--output", "out",
            ],
            vec!["admin", "backup", "--output", "backup"],
            vec!["admin", "renew", "--config", "node.json"],
        ] {
            assert!(parse(&args).is_ok(), "{args:?}: {:?}", parse(&args).err());
        }
    }

    #[test]
    fn duplicate_entrypoints_and_invalid_combinations_are_rejected() {
        for args in [
            vec!["identity", "list"],
            vec!["identity", "create"],
            vec!["certificates", "list"],
            vec!["certificates", "export"],
            vec!["workflow", "register"],
            vec!["workflow", "validate"],
            vec!["workflow", "run", "w"],
            vec!["plugins", "list"],
            vec!["plugins", "prepare"],
            vec!["plan", "-f", "p"],
            vec!["runs", "watch", "r"],
            vec!["runs", "logs", "r"],
            vec!["admin", "clusters"],
            vec!["admin", "targets"],
            vec!["admin", "members"],
            vec!["admin", "history"],
            vec!["admin", "audit"],
            vec!["admin", "retire-node"],
            vec!["run", "w"],
            vec!["run", "workflows", "w", "--detach"],
            vec!["run", "workflows", "w", "--dry-run", "--wait"],
            vec!["run", "workflows", "w", "--timeout-seconds", "1"],
            vec!["run", "workflows", "w", "--param", "x=y"],
            vec!["run", "workflows", "w", "--params-file", "v"],
            vec!["create", "runs"],
            vec!["get", "workflow"],
            vec!["get", "node"],
            vec!["get", "certificates"],
            vec!["update", "clusters"],
            vec!["replace", "members"],
            vec!["update", "targets"],
            vec!["update", "history"],
            vec!["get", "runs", "r", "--status", "running"],
            vec!["get", "nodes", "n", "--limit", "1"],
            vec!["get", "runs", "--limit", "0"],
            vec!["get", "runs", "--cursor", "x"],
            vec!["get", "audit", "--limit", "0"],
            vec!["get", "plugins", "--limit", "1"],
            vec!["get", "credentials", "c", "--identity", "alice"],
            vec!["logs", "runs", "r", "--full", "--tail", "1"],
            vec!["logs", "runs", "r", "--output-dir", "out"],
            vec!["logs", "runs", "r", "--step", "s", "--work-id", "w"],
            vec!["export", "credentials", "--output", "out"],
            vec![
                "export",
                "credentials",
                "--directory",
                "bundle",
                "--current",
                "--output",
                "out",
            ],
            vec![
                "export",
                "credentials",
                "--directory",
                "bundle",
                "--config",
                "c",
                "--output",
                "out",
            ],
            vec![
                "export",
                "credentials",
                "--ca",
                "--current",
                "--output",
                "out",
            ],
            vec![
                "export",
                "credentials",
                "--id",
                "c",
                "--password-file",
                "pw",
                "--output",
                "out",
            ],
            vec![
                "auth",
                "gui-login",
                "--output-dir",
                "out",
                "--password-file",
                "pw",
            ],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
