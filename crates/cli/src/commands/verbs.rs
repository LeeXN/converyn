//! Public verb/resource grammar. Business actions retain their existing handlers.
use super::operator::{Connection, Signing};
use super::*;

#[derive(Subcommand)]
pub(crate) enum ValidateResources {
    /// Validate workflows.
    Workflows {
        #[arg(
            short,
            long,
            help = "Workflow source file or directory containing workflow.yaml"
        )]
        file: PathBuf,
    },
}

impl From<ValidateResources> for Commands {
    fn from(command: ValidateResources) -> Self {
        match command {
            ValidateResources::Workflows { file } => Self::Workflow {
                command: super::operator::WorkflowCommands::Validate { file },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum RegisterResources {
    /// Encrypt and register an immutable Secret version once for the cluster.
    Secrets {
        name: String,
        #[arg(long)]
        version: String,
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Upload one local artifact to Converyn; print its immutable digest and size.
    Artifacts {
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Register workflows.
    Workflows {
        #[command(flatten)]
        connection: Connection,
        #[arg(
            short,
            long,
            help = "Workflow source file or directory containing workflow.yaml"
        )]
        file: PathBuf,
    },
}

impl From<RegisterResources> for Commands {
    fn from(command: RegisterResources) -> Self {
        match command {
            RegisterResources::Secrets {
                name,
                version,
                file,
            } => Self::RegisterSecret {
                config: Default::default(),
                name,
                version,
                file,
            },
            RegisterResources::Artifacts { file } => Self::RegisterArtifact {
                config: Default::default(),
                file,
            },
            RegisterResources::Workflows { connection, file } => Self::Workflow {
                command: super::operator::WorkflowCommands::Register { connection, file },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum EnableResources {
    /// Enable workflows.
    Workflows {
        #[command(flatten)]
        connection: Connection,
        id: String,
    },
    /// Enable identities.
    Identities {
        #[command(flatten)]
        connection: Connection,
        id: String,
    },
}

impl From<EnableResources> for Commands {
    fn from(command: EnableResources) -> Self {
        match command {
            EnableResources::Workflows { connection, id } => Self::Workflow {
                command: super::operator::WorkflowCommands::Enable { connection, id },
            },
            EnableResources::Identities { connection, id } => Self::Identity {
                command: super::operator::IdentityCommands::Enable { connection, id },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum DisableResources {
    /// Disable workflows.
    Workflows {
        #[command(flatten)]
        connection: Connection,
        id: String,
    },
    /// Disable identities.
    Identities {
        #[command(flatten)]
        connection: Connection,
        id: String,
    },
}

impl From<DisableResources> for Commands {
    fn from(command: DisableResources) -> Self {
        match command {
            DisableResources::Workflows { connection, id } => Self::Workflow {
                command: super::operator::WorkflowCommands::Disable { connection, id },
            },
            DisableResources::Identities { connection, id } => Self::Identity {
                command: super::operator::IdentityCommands::Disable { connection, id },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum CreateResources {
    /// Create identities.
    Identities {
        #[command(flatten)]
        connection: Connection,
        #[command(flatten)]
        signing: Signing,
        #[arg(long)]
        name: String,
        /// Create a super administrator; ordinary identities start without grants.
        #[arg(long)]
        super_admin: bool,
    },
    /// Create credentials.
    Credentials {
        #[command(flatten)]
        connection: Connection,
        #[command(flatten)]
        signing: Signing,
        #[arg(long)]
        identity: String,
    },
}

impl From<CreateResources> for Commands {
    fn from(command: CreateResources) -> Self {
        match command {
            CreateResources::Identities {
                connection,
                signing,
                name,
                super_admin,
            } => Self::Identity {
                command: super::operator::IdentityCommands::Create {
                    connection,
                    signing,
                    name,
                    super_admin,
                },
            },
            CreateResources::Credentials {
                connection,
                signing,
                identity,
            } => Self::Certificates {
                command: super::operator::CredentialCommands::Create {
                    connection,
                    signing,
                    identity,
                },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum UpdateResources {
    /// Update identities.
    Identities {
        #[command(flatten)]
        connection: Connection,
        id: String,
        #[arg(long)]
        name: String,
    },
    /// Submit a typed node-group management change, including its action and revision.
    NodeGroups {
        #[arg(skip)]
        config: super::configuration::Source,
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Submit a typed external-target management change.
    Targets {
        #[arg(skip)]
        config: super::configuration::Source,
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Submit a typed retention/archive/cleanup request with its revision and evidence.
    History {
        #[arg(skip)]
        config: super::configuration::Source,
        #[arg(short, long)]
        file: PathBuf,
    },
}

impl From<UpdateResources> for Commands {
    fn from(command: UpdateResources) -> Self {
        match command {
            UpdateResources::Identities {
                connection,
                id,
                name,
            } => Self::Identity {
                command: super::operator::IdentityCommands::Update {
                    connection,
                    id,
                    name,
                },
            },
            UpdateResources::NodeGroups { config, file } => Self::NodeGroups { config, file },
            UpdateResources::Targets { config, file } => Self::Targets { config, file },
            UpdateResources::History { config, file } => Self::History { config, file },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum RotateResources {
    /// Rotate credentials.
    Credentials {
        #[command(flatten)]
        connection: Connection,
        #[command(flatten)]
        signing: Signing,
        #[arg(long)]
        identity: String,
        #[arg(long, default_value = "0s")]
        overlap: String,
    },
}

impl From<RotateResources> for Commands {
    fn from(command: RotateResources) -> Self {
        match command {
            RotateResources::Credentials {
                connection,
                signing,
                identity,
                overlap,
            } => Self::Certificates {
                command: super::operator::CredentialCommands::Rotate {
                    connection,
                    signing,
                    identity,
                    overlap,
                },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum RevokeResources {
    /// Revoke credentials.
    Credentials {
        #[command(flatten)]
        connection: Connection,
        id: String,
    },
}

impl From<RevokeResources> for Commands {
    fn from(command: RevokeResources) -> Self {
        match command {
            RevokeResources::Credentials { connection, id } => Self::Certificates {
                command: super::operator::CredentialCommands::Delete { connection, id },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum ResumeResources {
    /// Resume credentials.
    Credentials {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        output: PathBuf,
    },
    /// Resume paused Runs or re-admit never-started work after a pure input failure.
    Runs {
        #[arg(skip)]
        config: super::configuration::Source,
        run_id: String,
    },
}

impl From<ResumeResources> for Commands {
    fn from(command: ResumeResources) -> Self {
        match command {
            ResumeResources::Credentials { connection, output } => Self::Certificates {
                command: super::operator::CredentialCommands::Resume { connection, output },
            },
            ResumeResources::Runs { config, run_id } => Self::Pause {
                config,
                run_id,
                expected_revision: None,
                resume: true,
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum LogsResources {
    /// Read saved logs by step/node or exact Work; defaults to the last 100 lines per stream.
    #[command(after_help = "Examples:
  converyn logs runs RUN
  converyn logs runs RUN --step verify --node single-agent-1
  converyn logs runs RUN --step verify --follow
  converyn logs runs RUN --tail 200 --stream stderr
  converyn logs runs RUN --full
  converyn logs runs RUN --full --output-dir ./run-logs
  converyn logs runs RUN --follow -o json
Full means all saved bytes, subject to the existing 8 MiB stream cap and retention. Full reads/export never fall back to receipt tails. Default tail reads label any receipt fallback. Follow reconnects at byte cursors; Ctrl+C only stops viewing.")]
    Runs {
        #[arg(skip)]
        config: super::configuration::Source,
        run_id: String,
        #[command(flatten)]
        options: super::task_logs::Options,
    },
}

impl From<LogsResources> for Commands {
    fn from(command: LogsResources) -> Self {
        match command {
            LogsResources::Runs {
                config,
                run_id,
                options,
            } => Self::Logs {
                config,
                run_id,
                options,
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum WatchResources {
    /// Wait for actual Run evidence. Exit 0=success, 2=failed/canceled, 3=attention, 124=timeout.
    Runs {
        #[arg(skip)]
        config: super::configuration::Source,
        run_id: String,
        #[arg(long, default_value = "1800")]
        timeout_seconds: u64,
    },
}

impl From<WatchResources> for Commands {
    fn from(command: WatchResources) -> Self {
        match command {
            WatchResources::Runs {
                config,
                run_id,
                timeout_seconds,
            } => Self::Wait {
                config,
                run_id,
                timeout_seconds,
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum CancelResources {
    /// Cancel runs.
    Runs {
        #[arg(skip)]
        config: super::configuration::Source,
        run_id: String,
    },
}

impl From<CancelResources> for Commands {
    fn from(command: CancelResources) -> Self {
        match command {
            CancelResources::Runs { config, run_id } => Self::Cancel { config, run_id },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum PauseResources {
    /// Pause runs.
    Runs {
        #[arg(skip)]
        config: super::configuration::Source,
        run_id: String,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(skip)]
        resume: bool,
    },
}

impl From<PauseResources> for Commands {
    fn from(command: PauseResources) -> Self {
        match command {
            PauseResources::Runs {
                config,
                run_id,
                expected_revision,
                resume,
            } => Self::Pause {
                config,
                run_id,
                expected_revision,
                resume,
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum ResolveResources {
    /// Resolve runs.
    Runs {
        #[command(flatten)]
        connection: Connection,
        run_id: String,
        #[arg(long)]
        work_id: String,
        #[arg(long, value_parser=["succeeded","failed","canceled"])]
        status: String,
        #[arg(long)]
        evidence: String,
    },
}

impl From<ResolveResources> for Commands {
    fn from(command: ResolveResources) -> Self {
        match command {
            ResolveResources::Runs {
                connection,
                run_id,
                work_id,
                status,
                evidence,
            } => Self::Workflow {
                command: super::operator::WorkflowCommands::Resolve {
                    connection,
                    run_id,
                    work_id,
                    status,
                    evidence,
                },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum ReplaceResources {
    /// Submit an audited controller replacement with membership and certificate checks.
    Members {
        #[arg(skip)]
        config: super::configuration::Source,
        #[arg(short, long)]
        file: PathBuf,
    },
}

impl From<ReplaceResources> for Commands {
    fn from(command: ReplaceResources) -> Self {
        match command {
            ReplaceResources::Members { config, file } => Self::Members { config, file },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum RetireResources {
    /// Retire nodes.
    Nodes {
        #[arg(skip)]
        config: super::configuration::Source,
        node: String,
        #[arg(long)]
        expected_fingerprint: String,
        #[arg(long)]
        evidence: String,
    },
}

impl From<RetireResources> for Commands {
    fn from(command: RetireResources) -> Self {
        match command {
            RetireResources::Nodes {
                config,
                node,
                expected_fingerprint,
                evidence,
            } => Self::RetireNode {
                config,
                node,
                expected_fingerprint,
                evidence,
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum CompileResources {
    /// Compile local PlanInput YAML/JSON into an immutable Plan; does not execute it.
    Plans {
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Compile a plugin spec into a fixed call descriptor; does not execute it.
    Plugins {
        id: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        spec: PathBuf,
    },
}

impl From<CompileResources> for Commands {
    fn from(command: CompileResources) -> Self {
        match command {
            CompileResources::Plans { file } => Self::Plan { file },
            CompileResources::Plugins { id, action, spec } => Self::Plugins {
                command: super::definition::PluginCommands::Call { id, action, spec },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum PrepareResources {
    /// Generate local private plugin inputs; does not contact the control plane.
    Plugins {
        id: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

impl From<PrepareResources> for Commands {
    fn from(command: PrepareResources) -> Self {
        match command {
            PrepareResources::Plugins {
                id,
                action,
                input,
                output,
            } => Self::Plugins {
                command: super::definition::PluginCommands::Prepare {
                    id,
                    action,
                    input,
                    output,
                },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum ExpandResources {
    /// Generate a local Playbook and embedded definitions; does not execute them.
    Plugins {
        id: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

impl From<ExpandResources> for Commands {
    fn from(command: ExpandResources) -> Self {
        match command {
            ExpandResources::Plugins {
                id,
                action,
                input,
                output,
            } => Self::Plugins {
                command: super::definition::PluginCommands::Expand {
                    id,
                    action,
                    input,
                    output,
                },
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum ExportResources {
    /// Export a public certificate, CA, or private PKCS#12 bundle.
    #[command(after_help = "Examples:
  converyn export credentials --id CREDENTIAL_ID --output ./public.crt
  converyn export credentials --ca --output ./ca.crt
  converyn export credentials --current --output ./browser.p12
  converyn --context qa export credentials --current --output ./qa.p12
  converyn export credentials --directory /secure/alice --output ./alice.p12
The server cannot recover private keys or tokens. --current reads your selected local config; --directory reads the original private bundle. PKCS#12 export uses installed OpenSSL and prompts for a password unless --password-file is supplied.")]
    Credentials(super::certificate_export::ExportArgs),
}
impl From<ExportResources> for Commands {
    fn from(command: ExportResources) -> Self {
        match command {
            ExportResources::Credentials(args) => Self::Certificates {
                command: super::operator::CredentialCommands::Export(args),
            },
        }
    }
}
#[derive(Subcommand)]
pub(crate) enum RunResources {
    /// Submit a workflow; --wait waits for its result and --dry-run only previews.
    Workflows(super::operator::RunArgs),
}
impl From<RunResources> for Commands {
    fn from(command: RunResources) -> Self {
        match command {
            RunResources::Workflows(args) => Self::Workflow {
                command: super::operator::WorkflowCommands::Run(args),
            },
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum DeleteResources {
    /// Retire an unreferenced node group, retaining its UID and audit history.
    NodeGroups {
        #[arg(skip)]
        config: super::configuration::Source,
        id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        reason: String,
    },
}
impl From<DeleteResources> for Commands {
    fn from(command: DeleteResources) -> Self {
        match command {
            DeleteResources::NodeGroups {
                config,
                id,
                expected_revision,
                reason,
            } => Self::DeleteNodeGroup {
                config,
                id,
                expected_revision,
                reason,
            },
        }
    }
}
