use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum RuntimeRoleArg {
    Worker,
    Validator,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum RunModeArg {
    Manual,
    Auto,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum MergeExecuteModeArg {
    Local,
    Pr,
}
#[derive(Parser)]
#[command(version, propagate_version = true)]
pub struct Cli {
    /// Output format
    #[arg(long, short = 'f', global = true, default_value = "table")]
    pub format: OutputFormat,

    /// Verbose output
    #[arg(long, short = 'v', global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}
/// Top-level commands.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
pub enum Commands {
    /// Show version information
    Version,

    /// Run the HTTP server (API + UI)
    Serve(ServeArgs),

    /// Project management commands
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Global governance artifact registry commands
    #[command(subcommand)]
    Global(GlobalCommands),

    /// Project constitution lifecycle and validation commands
    #[command(subcommand)]
    Constitution(ConstitutionCommands),

    /// Task management commands
    #[command(subcommand)]
    Task(TaskCommands),

    /// Task graphs (planning): define tasks + dependencies as a DAG
    #[command(subcommand)]
    Graph(GraphCommands),

    /// Task flows (execution): run a graph using a configured runtime adapter
    #[command(subcommand)]
    Flow(FlowCommands),

    /// Workflow definitions and workflow run lifecycle commands
    #[command(subcommand)]
    Workflow(WorkflowCommands),

    /// Event inspection commands
    #[command(subcommand)]
    Events(EventCommands),

    /// Runtime adapter discovery and health checks
    #[command(subcommand)]
    Runtime(RuntimeCommands),

    /// Verification commands
    #[command(subcommand)]
    Verify(VerifyCommands),

    /// Merge commands
    #[command(subcommand)]
    Merge(MergeCommands),

    /// Attempt inspection commands
    #[command(subcommand)]
    Attempt(AttemptCommands),

    /// Checkpoint lifecycle commands
    #[command(subcommand)]
    Checkpoint(CheckpointCommands),

    /// Inspect and manage git worktrees used for task execution
    #[command(subcommand)]
    Worktree(WorktreeCommands),
}
#[derive(Args)]
pub struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    #[arg(long, default_value_t = 8787)]
    pub port: u16,

    #[arg(long, default_value_t = 200)]
    pub events_limit: usize,
}
