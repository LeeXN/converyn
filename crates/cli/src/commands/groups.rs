//! Task and administration command groups.
use super::*;

#[derive(Subcommand)]
pub(crate) enum AdminCommands {
    /// Offline backed-up removal of obsolete Agent authorization settings.
    MigrateAgent {
        #[arg(long)]
        file: PathBuf,
    },
    /// Generate or rotate independent output encryption keys; distribute privately to all controllers.
    OutputKey {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        rotate: bool,
    },
    /// Generate fresh controller bundles, bootstrap credentials and a personal issuer.
    Init {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        cluster: String,
        #[arg(long = "member")]
        members: Vec<String>,
        #[arg(long, default_value = "default", hide = true)]
        project: String,
        #[arg(long, default_value = "hosts", hide = true)]
        business_cluster: String,
        #[arg(long = "target")]
        targets: Vec<String>,
        /// Lifetime for initial leaf certificates and the online Node issuer, in seconds (300..31536000).
        #[arg(long, default_value_t = converyn_runtime::credentials::pki::DEFAULT_LEAF_LIFETIME_SECONDS)]
        leaf_lifetime_seconds: u64,
    },
    Initialize {
        #[arg(skip)]
        config: super::configuration::Source,
        #[arg(long)]
        members: PathBuf,
    },
    /// Create a private one-use enrollment bundle; bearer values are written only to the output file.
    Bootstrap {
        #[arg(skip)]
        config: super::configuration::Source,
        #[arg(long)]
        node: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "900")]
        ttl_seconds: u64,
        #[arg(long)]
        recover: bool,
    },
    /// Enroll a managed host; approved host steps may run as root. Retains journal and Node UID.
    Enroll {
        #[arg(long)]
        bootstrap: PathBuf,
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        template: Option<PathBuf>,
        #[arg(long)]
        hardware_id_file: Option<PathBuf>,
    },
    /// Stage and activate a new Node certificate while retaining the Node identity.
    Renew {
        #[arg(skip)]
        config: PathBuf,
    },
    /// Install this binary as a root supervisor, with a dedicated non-root task account.
    InstallLocal {
        #[arg(long)]
        hardware_id_file: Option<PathBuf>,
        #[arg(long, conflicts_with = "bootstrap_stdin")]
        bootstrap: Option<PathBuf>,
        #[arg(long)]
        bootstrap_stdin: bool,
    },
    /// Install over strict OpenSSH using a key/SSH agent; root or passwordless sudo is required.
    InstallSsh {
        #[arg(long)]
        hardware_id_file: Option<PathBuf>,
        #[arg(long)]
        host: String,
        #[arg(long)]
        user: String,
        #[arg(long, default_value = "22")]
        port: u16,
        #[arg(long)]
        identity_file: Option<PathBuf>,
        #[arg(long)]
        known_hosts: PathBuf,
        #[arg(long)]
        binary: Option<PathBuf>,
        #[arg(long)]
        bootstrap: PathBuf,
    },
    /// Remove the standard Agent service; preserve task history and managed applications.
    UninstallLocal {
        #[arg(long)]
        node: String,
    },
    /// Read the local Agent's last confirmed identity, wait reason and active intent.
    AgentInfo {
        #[arg(long)]
        state_dir: PathBuf,
    },

    /// Export a checksum-verified logical backup from the quorum-holding controller.
    Backup {
        #[arg(skip)]
        config: super::configuration::Source,
        #[arg(long)]
        output: PathBuf,
    },
    /// Restore into a new directory; old controllers must be externally fenced first.
    Restore {
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        data_dir: PathBuf,
        #[arg(long)]
        node_id: u64,
        #[arg(long)]
        old_control_plane_fenced: bool,
        #[arg(long)]
        keyring: Option<PathBuf>,
    },

    /// Generate a private CA directory using built-in Rust cryptography.
    PkiInit {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        name: String,
    },
    /// Issue a client or server identity in a new private directory.
    PkiIssue {
        #[arg(long)]
        authority: PathBuf,
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        ip: Vec<std::net::IpAddr>,
        /// Leaf certificate lifetime in seconds (300..31536000), capped by the CA expiry.
        #[arg(long, default_value_t = converyn_runtime::credentials::pki::DEFAULT_LEAF_LIFETIME_SECONDS)]
        leaf_lifetime_seconds: u64,
    },
    /// Download and verify a content-addressed artifact using bounded memory.
    Fetch {
        #[arg(long)]
        url: String,
        #[arg(long)]
        sha256: String,
        #[arg(long)]
        size: u64,
        #[arg(long)]
        cache_dir: PathBuf,
        #[arg(long, default_value = "3")]
        attempts: u32,
        #[arg(long)]
        bytes_per_second: Option<u64>,
    },
    /// Execute an explicitly authorized script with a durable, non-replaying receipt (lab only).
    RunScript {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        operation_id: String,
        #[arg(long)]
        script: PathBuf,
        #[arg(long)]
        sha256: String,
        #[arg(long, default_value = "60")]
        timeout: u64,
    },
    /// Write the exact embedded OpenAPI contract to stdout.
    ApiSchema,
}

impl From<AdminCommands> for Commands {
    fn from(command: AdminCommands) -> Self {
        match command {
            AdminCommands::MigrateAgent { file } => Commands::MigrateAgent { file },
            AdminCommands::OutputKey { file, rotate } => Commands::OutputKey { file, rotate },
            AdminCommands::Init {
                directory,
                cluster,
                members,
                project,
                business_cluster,
                targets,
                leaf_lifetime_seconds,
            } => Commands::Init {
                directory,
                cluster,
                members,
                project,
                business_cluster,
                targets,
                leaf_lifetime_seconds,
            },
            AdminCommands::Initialize { config, members } => {
                Commands::Initialize { config, members }
            }
            AdminCommands::Bootstrap {
                config,
                node,
                output,
                ttl_seconds,
                recover,
            } => Commands::Bootstrap {
                config,
                node,
                output,
                ttl_seconds,
                recover,
            },
            AdminCommands::Enroll {
                bootstrap,
                state_dir,
                output,
                template,
                hardware_id_file,
            } => Commands::Enroll {
                bootstrap,
                state_dir,
                output,
                template,
                hardware_id_file,
            },
            AdminCommands::Renew { config } => Commands::Renew { config },
            AdminCommands::InstallLocal {
                hardware_id_file,
                bootstrap,
                bootstrap_stdin,
            } => Commands::InstallLocal {
                hardware_id_file,
                bootstrap,
                bootstrap_stdin,
            },
            AdminCommands::InstallSsh {
                hardware_id_file,
                host,
                user,
                port,
                identity_file,
                known_hosts,
                binary,
                bootstrap,
            } => Commands::InstallSsh {
                hardware_id_file,
                host,
                user,
                port,
                identity_file,
                known_hosts,
                binary,
                bootstrap,
            },
            AdminCommands::UninstallLocal { node } => Commands::UninstallLocal { node },
            AdminCommands::AgentInfo { state_dir } => Commands::AgentInfo { state_dir },
            AdminCommands::Backup { config, output } => Commands::Backup { config, output },
            AdminCommands::Restore {
                backup,
                data_dir,
                node_id,
                old_control_plane_fenced,
                keyring,
            } => Commands::Restore {
                backup,
                data_dir,
                node_id,
                old_control_plane_fenced,
                keyring,
            },
            AdminCommands::PkiInit { directory, name } => Commands::PkiInit { directory, name },
            AdminCommands::PkiIssue {
                authority,
                directory,
                name,
                ip,
                leaf_lifetime_seconds,
            } => Commands::PkiIssue {
                authority,
                directory,
                name,
                ip,
                leaf_lifetime_seconds,
            },
            AdminCommands::Fetch {
                url,
                sha256,
                size,
                cache_dir,
                attempts,
                bytes_per_second,
            } => Commands::Fetch {
                url,
                sha256,
                size,
                cache_dir,
                attempts,
                bytes_per_second,
            },
            AdminCommands::RunScript {
                state_dir,
                operation_id,
                script,
                sha256,
                timeout,
            } => Commands::RunScript {
                state_dir,
                operation_id,
                script,
                sha256,
                timeout,
            },
            AdminCommands::ApiSchema => Commands::ApiSchema,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum HelperCommands {
    /// Merge a JSON object from stdin into this managed task's private result file.
    Emit {
        #[arg(long, required = true)]
        stdin: bool,
        #[arg(long = "context-update")]
        output_context: bool,
        #[arg(long)]
        key: Option<String>,
    },
    /// Invoke the registered local script with explicitly bound JSON context inputs.
    Invoke {
        #[arg(long = "environment-json-env")]
        environments: Vec<String>,
        #[arg(long = "fixed-json-env")]
        fixed: Vec<String>,
        #[arg(long)]
        script: PathBuf,
        #[arg(long)]
        publish_context: bool,
    },
    /// Read a field from the current task context, including this task's own updates.
    Context {
        #[arg(long)]
        get: String,
        #[arg(long)]
        raw: bool,
    },
}
impl From<HelperCommands> for Commands {
    fn from(c: HelperCommands) -> Self {
        match c {
            HelperCommands::Emit {
                output_context,
                key,
                ..
            } => Commands::Emit {
                context: output_context,
                key,
            },
            HelperCommands::Invoke {
                environments,
                fixed,
                script,
                publish_context,
            } => Commands::Invoke {
                environments,
                fixed,
                script,
                publish_context,
            },
            HelperCommands::Context { get, raw } => Commands::TaskContext { get, raw },
        }
    }
}
