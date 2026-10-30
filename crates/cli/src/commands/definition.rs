//! Public command contract and internal actions.
use super::*;
#[derive(Subcommand)]
pub(crate) enum CliCommands {
    /// Run resources.
    Run {
        #[command(subcommand)]
        command: super::verbs::RunResources,
    },
    /// Export resources.
    Export {
        #[command(subcommand)]
        command: super::verbs::ExportResources,
    },
    /// Expand resources.
    Expand {
        #[command(subcommand)]
        command: super::verbs::ExpandResources,
    },
    /// Prepare resources.
    Prepare {
        #[command(subcommand)]
        command: super::verbs::PrepareResources,
    },
    /// Compile resources.
    Compile {
        #[command(subcommand)]
        command: super::verbs::CompileResources,
    },
    /// Retire resources.
    Retire {
        #[command(subcommand)]
        command: super::verbs::RetireResources,
    },
    /// Replace resources.
    Replace {
        #[command(subcommand)]
        command: super::verbs::ReplaceResources,
    },
    /// Resolve resources.
    Resolve {
        #[command(subcommand)]
        command: super::verbs::ResolveResources,
    },
    /// Pause resources.
    Pause {
        #[command(subcommand)]
        command: super::verbs::PauseResources,
    },
    /// Cancel resources.
    Cancel {
        #[command(subcommand)]
        command: super::verbs::CancelResources,
    },
    /// Watch resources.
    Watch {
        #[command(subcommand)]
        command: super::verbs::WatchResources,
    },
    /// Logs resources.
    Logs {
        #[command(subcommand)]
        command: super::verbs::LogsResources,
    },
    /// Resume resources.
    Resume {
        #[command(subcommand)]
        command: super::verbs::ResumeResources,
    },
    /// Delete resources.
    Delete {
        #[command(subcommand)]
        command: super::verbs::DeleteResources,
    },
    /// Revoke resources.
    Revoke {
        #[command(subcommand)]
        command: super::verbs::RevokeResources,
    },
    /// Rotate resources.
    Rotate {
        #[command(subcommand)]
        command: super::verbs::RotateResources,
    },
    /// Update resources.
    Update {
        #[command(subcommand)]
        command: super::verbs::UpdateResources,
    },
    /// Create resources.
    Create {
        #[command(subcommand)]
        command: super::verbs::CreateResources,
    },
    /// Disable resources.
    Disable {
        #[command(subcommand)]
        command: super::verbs::DisableResources,
    },
    /// Enable resources.
    Enable {
        #[command(subcommand)]
        command: super::verbs::EnableResources,
    },
    /// Register resources.
    Register {
        #[command(subcommand)]
        command: super::verbs::RegisterResources,
    },
    /// Validate resources.
    Validate {
        #[command(subcommand)]
        command: super::verbs::ValidateResources,
    },
    /// List resources or inspect one by ID (workflows, runs, nodes, node-groups...).
    Get(super::query::GetArgs),
    /// Inspect effective permissions, manage groups and bind roles.
    Auth {
        #[command(subcommand)]
        command: super::access::AuthCommands,
    },
    /// Local script input/output helpers (called by workflow scripts).
    Task {
        #[command(subcommand)]
        command: super::groups::HelperCommands,
    },
    /// Cluster administration, installation, credentials and maintenance tools.
    Admin {
        #[command(subcommand)]
        command: super::groups::AdminCommands,
    },
    /// Import, select, inspect and export portable personal contexts.
    Config {
        #[command(subcommand)]
        command: super::configuration::ConfigCommands,
    },
    #[command(hide = true)]
    TaskEnter {
        #[arg(long)]
        privileged: bool,
        #[arg(long)]
        uid: u32,
        #[arg(long)]
        gid: u32,
        #[arg(long)]
        script: PathBuf,
        #[arg(long)]
        journal: PathBuf,
    },
    #[command(hide = true)]
    TaskComplete {
        #[arg(long)]
        journal: PathBuf,
        #[arg(long)]
        unit: String,
    },
    /// Show the current cluster overview.
    Status {
        #[arg(skip)]
        config: super::configuration::Source,
    },
    /// Run an autonomous mTLS agent with a private JSON configuration.
    Agent {
        #[arg(skip)]
        config: PathBuf,
    },
    /// Start a control-plane member; external listeners require mTLS and role credentials.
    Server {
        #[arg(skip)]
        config: Option<PathBuf>,
        #[arg(long, required_unless_present = "config", conflicts_with = "config")]
        id: Option<u64>,
        #[arg(long, required_unless_present = "config", conflicts_with = "config")]
        listen: Option<SocketAddr>,
        #[arg(long, required_unless_present = "config", conflicts_with = "config")]
        data_dir: Option<PathBuf>,
        #[arg(long, default_value = "converyn-p0")]
        cluster: String,
        #[arg(
            long,
            env = "CONVERYN_TOKEN",
            hide_env_values = true,
            default_value = ""
        )]
        token: String,
        #[arg(long)]
        auth_file: Option<PathBuf>,
        #[arg(long, requires = "issuer_key")]
        issuer_cert: Option<PathBuf>,
        #[arg(long, requires = "issuer_cert")]
        issuer_key: Option<PathBuf>,
        #[arg(long, default_value_t = converyn_runtime::credentials::pki::DEFAULT_LEAF_LIFETIME_SECONDS)]
        leaf_lifetime_seconds: u64,
        #[arg(long, requires_all=["tls_key","tls_ca"])]
        tls_cert: Option<PathBuf>,
        #[arg(long, requires = "tls_cert")]
        tls_key: Option<PathBuf>,
        #[arg(long, requires = "tls_cert")]
        tls_ca: Option<PathBuf>,
    },
}

/// Internal actions share handlers without exposing duplicate CLI spellings.
pub(crate) enum Commands {
    MigrateAgent {
        file: PathBuf,
    },
    RegisterSecret {
        config: super::configuration::Source,
        name: String,
        version: String,
        file: PathBuf,
    },
    RegisterArtifact {
        config: super::configuration::Source,
        file: PathBuf,
    },
    Auth {
        command: super::access::AuthCommands,
    },
    Emit {
        context: bool,
        key: Option<String>,
    },
    Invoke {
        environments: Vec<String>,
        fixed: Vec<String>,
        script: PathBuf,
        publish_context: bool,
    },
    TaskContext {
        get: String,
        raw: bool,
    },
    OutputKey {
        file: PathBuf,
        rotate: bool,
    },
    /// List resources or inspect one by ID (workflows, runs, nodes, node-groups...).
    Get(super::query::GetArgs),
    /// Import, select, inspect and export portable personal contexts.
    Config {
        command: super::configuration::ConfigCommands,
    },
    /// List and execute OP-registered workflows.
    Workflow {
        command: super::operator::WorkflowCommands,
    },
    /// Manage named personal identities; no passwords or project roles.
    Identity {
        command: super::operator::IdentityCommands,
    },
    /// Read immutable audit pages.
    Audit {
        config: super::configuration::Source,
        after: u64,
        limit: usize,
    },
    /// Write the exact embedded OpenAPI contract to stdout.
    ApiSchema,
    /// 查看编入的插件、生成固定调用或展开内嵌脚本包。
    Plugins {
        command: PluginCommands,
    },
    /// 管理通用外部部署目标。
    Targets {
        config: super::configuration::Source,
        file: PathBuf,
    },
    /// Wait for a Run's actual evidence. Exit 0=success, 2=failed/canceled, 3=attention, 124=timeout.
    Wait {
        config: super::configuration::Source,
        run_id: String,
        timeout_seconds: u64,
    },
    /// Inspect or configure retained history; archiving keeps complete proof on each replica's disk.
    History {
        config: super::configuration::Source,
        file: PathBuf,
    },
    /// Retire an unused Node identity with a certificate CAS and an audit reason.
    RetireNode {
        config: super::configuration::Source,
        node: String,
        expected_fingerprint: String,
        evidence: String,
    },
    /// Remove the standard Agent service; preserve task history and managed applications.
    UninstallLocal {
        node: String,
    },
    /// Inspect membership or submit an audited replacement of one controller.
    Members {
        config: super::configuration::Source,
        file: PathBuf,
    },
    /// Export a checksum-verified logical backup from the quorum-holding controller.
    Backup {
        config: super::configuration::Source,
        output: PathBuf,
    },
    /// Restore into a new directory; old controllers must be externally fenced first.
    Restore {
        backup: PathBuf,
        data_dir: PathBuf,
        node_id: u64,
        old_control_plane_fenced: bool,
        keyring: Option<PathBuf>,
    },
    /// Pause or resume new authorizations; already authorized work keeps its durable grant.
    Pause {
        config: super::configuration::Source,
        run_id: String,
        expected_revision: Option<u64>,
        resume: bool,
    },
    /// Install this binary as a root supervisor, with a dedicated non-root task account.
    InstallLocal {
        hardware_id_file: Option<PathBuf>,
        bootstrap: Option<PathBuf>,
        bootstrap_stdin: bool,
    },
    /// Install over strict OpenSSH using a key/SSH agent; root or passwordless sudo is required.
    InstallSsh {
        hardware_id_file: Option<PathBuf>,
        host: String,
        user: String,
        port: u16,
        identity_file: Option<PathBuf>,
        known_hosts: PathBuf,
        binary: Option<PathBuf>,
        bootstrap: PathBuf,
    },
    /// Read the local Agent's last confirmed identity, wait reason and active intent.
    AgentInfo {
        state_dir: PathBuf,
    },
    /// Generate fresh controller bundles, bootstrap credentials and a personal issuer.
    Init {
        directory: PathBuf,
        cluster: String,
        members: Vec<String>,
        project: String,
        business_cluster: String,
        targets: Vec<String>,
        /// Lifetime for initial leaf certificates and the online Node issuer, in seconds (300..31536000).
        leaf_lifetime_seconds: u64,
    },
    /// Commit initial membership after the generated controller profiles are running.
    Initialize {
        config: super::configuration::Source,
        members: PathBuf,
    },
    /// Create a private one-use enrollment bundle; bearer values are written only to the output file.
    Bootstrap {
        config: super::configuration::Source,
        node: String,
        output: PathBuf,
        ttl_seconds: u64,
        recover: bool,
    },
    /// Enroll this host without replacing its execution journal or Node UID.
    Enroll {
        bootstrap: PathBuf,
        state_dir: PathBuf,
        output: PathBuf,
        template: Option<PathBuf>,
        hardware_id_file: Option<PathBuf>,
    },
    /// Stage and activate a new Node certificate while retaining the Node identity.
    Renew {
        config: PathBuf,
    },
    /// Generate a private CA directory using built-in Rust cryptography.
    PkiInit {
        directory: PathBuf,
        name: String,
    },
    /// Issue a client or server identity in a new private directory.
    PkiIssue {
        authority: PathBuf,
        directory: PathBuf,
        name: String,
        ip: Vec<std::net::IpAddr>,
        /// Leaf certificate lifetime in seconds (300..31536000), capped by the CA expiry.
        leaf_lifetime_seconds: u64,
    },
    TaskEnter {
        privileged: bool,
        uid: u32,
        gid: u32,
        script: PathBuf,
        journal: PathBuf,
    },
    TaskComplete {
        journal: PathBuf,
        unit: String,
    },
    /// Read bounded task log tails and stored-byte digests from committed receipts.
    Logs {
        config: super::configuration::Source,
        run_id: String,
        options: super::task_logs::Options,
    },
    /// Manage platform node groups and ownership, or apply a versioned management change.
    DeleteNodeGroup {
        config: super::configuration::Source,
        id: String,
        expected_revision: u64,
        reason: String,
    },
    NodeGroups {
        config: super::configuration::Source,
        file: PathBuf,
    },
    /// Issue, rotate, list, revoke and export personal credential pairs.
    Certificates {
        command: super::operator::CredentialCommands,
    },
    /// Show the current cluster overview.
    Status {
        config: super::configuration::Source,
    },
    /// Request cancellation; this does not falsely mark running work as stopped.
    Cancel {
        config: super::configuration::Source,
        run_id: String,
    },
    /// Run an autonomous mTLS agent with a private JSON configuration.
    Agent {
        config: PathBuf,
    },
    /// Execute an explicitly authorized script with a durable, non-replaying receipt (lab only).
    RunScript {
        state_dir: PathBuf,
        operation_id: String,
        script: PathBuf,
        sha256: String,
        timeout: u64,
    },
    /// Start a control-plane member; external listeners require mTLS and role credentials.
    Server {
        config: Option<PathBuf>,
        id: Option<u64>,
        listen: Option<SocketAddr>,
        data_dir: Option<PathBuf>,
        cluster: String,
        token: String,
        auth_file: Option<PathBuf>,
        issuer_cert: Option<PathBuf>,
        issuer_key: Option<PathBuf>,
        leaf_lifetime_seconds: u64,
        tls_cert: Option<PathBuf>,
        tls_key: Option<PathBuf>,
        tls_ca: Option<PathBuf>,
    },
    /// Validate YAML/JSON inputs and compile an immutable plan; does not modify hosts.
    Plan {
        file: PathBuf,
    },
    /// Download and verify a content-addressed artifact using bounded memory.
    Fetch {
        url: String,
        sha256: String,
        size: u64,
        cache_dir: PathBuf,
        attempts: u32,
        bytes_per_second: Option<u64>,
    },
}

impl From<CliCommands> for Commands {
    fn from(command: CliCommands) -> Self {
        match command {
            CliCommands::Auth { command } => Self::Auth { command },
            CliCommands::Task { command } => command.into(),
            CliCommands::Validate { command } => command.into(),
            CliCommands::Register { command } => command.into(),
            CliCommands::Enable { command } => command.into(),
            CliCommands::Disable { command } => command.into(),
            CliCommands::Create { command } => command.into(),
            CliCommands::Update { command } => command.into(),
            CliCommands::Rotate { command } => command.into(),
            CliCommands::Delete { command } => command.into(),
            CliCommands::Revoke { command } => command.into(),
            CliCommands::Resume { command } => command.into(),
            CliCommands::Logs { command } => command.into(),
            CliCommands::Watch { command } => command.into(),
            CliCommands::Cancel { command } => command.into(),
            CliCommands::Pause { command } => command.into(),
            CliCommands::Resolve { command } => command.into(),
            CliCommands::Replace { command } => command.into(),
            CliCommands::Retire { command } => command.into(),
            CliCommands::Compile { command } => command.into(),
            CliCommands::Prepare { command } => command.into(),
            CliCommands::Expand { command } => command.into(),
            CliCommands::Export { command } => command.into(),
            CliCommands::Run { command } => command.into(),
            CliCommands::Get(args) => Self::Get(args),
            CliCommands::Admin { command } => command.into(),
            CliCommands::Config { command } => Self::Config { command },
            CliCommands::TaskEnter {
                privileged,
                uid,
                gid,
                script,
                journal,
            } => Self::TaskEnter {
                privileged,
                uid,
                gid,
                script,
                journal,
            },
            CliCommands::TaskComplete { journal, unit } => Self::TaskComplete { journal, unit },
            CliCommands::Status { config } => Self::Status { config },
            CliCommands::Agent { config } => Self::Agent { config },
            CliCommands::Server {
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
            } => Self::Server {
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
            },
        }
    }
}

pub(crate) enum PluginCommands {
    Prepare {
        id: String,
        action: String,
        input: PathBuf,
        output: PathBuf,
    },
    Call {
        id: String,
        action: String,
        spec: PathBuf,
    },
    Expand {
        id: String,
        action: String,
        input: PathBuf,
        output: PathBuf,
    },
}
