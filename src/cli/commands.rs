//! CLI command definitions.
//!
//! All features are accessible via CLI. The UI is a projection, not a controller.

use super::output::OutputFormat;
use clap::{Args, Parser, Subcommand};

mod graph_flow;
pub use graph_flow::*;

mod governance_global;
pub use governance_global::*;

mod task_runtime_event;
pub use task_runtime_event::*;

#[cfg(test)]
mod tests;

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

/// Hivemind CLI - Structured orchestration for agentic coding workflows.
#[derive(Parser)]
#[command(name = "hivemind")]
#[command(
    version,
    about,
    long_about = "CLI-first orchestration for agentic coding workflows.\n\nStart with: docs/overview/quickstart.md"
)]
#[command(propagate_version = true)]
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

/// Checkpoint subcommands.
#[derive(Subcommand)]
pub enum CheckpointCommands {
    /// List checkpoints for an attempt
    List(CheckpointListArgs),
    /// Complete the currently active checkpoint for an attempt
    Complete(CheckpointCompleteArgs),
}

/// Arguments for checkpoint listing.
#[derive(Args)]
pub struct CheckpointListArgs {
    /// Attempt ID
    pub attempt_id: String,
}

/// Arguments for checkpoint completion.
#[derive(Args)]
pub struct CheckpointCompleteArgs {
    /// Attempt ID (optional when `HIVEMIND_ATTEMPT_ID` is set)
    #[arg(long)]
    pub attempt_id: Option<String>,

    /// Checkpoint ID
    #[arg(long = "id")]
    pub checkpoint_id: String,

    /// Optional completion summary
    #[arg(long)]
    pub summary: Option<String>,
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

#[derive(Subcommand)]
pub enum WorktreeCommands {
    /// List worktree status for each task in a flow
    List(WorktreeListArgs),
    /// Inspect the worktree path and git metadata for a single task
    Inspect(WorktreeInspectArgs),
    /// Remove worktrees for a flow (best-effort)
    Cleanup(WorktreeCleanupArgs),
}

#[derive(Args)]
pub struct WorktreeListArgs {
    /// Flow ID
    pub flow_id: String,
}

#[derive(Args)]
pub struct WorktreeInspectArgs {
    /// Task ID
    pub task_id: String,
}

#[derive(Args)]
pub struct WorktreeCleanupArgs {
    /// Flow ID
    pub flow_id: String,
    /// Force cleanup when flow is still running
    #[arg(long, default_value_t = false)]
    pub force: bool,
    /// Show what would be cleaned without removing worktrees
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TaskRetryMode {
    Clean,
    Continue,
}

#[derive(Args)]
pub struct TaskRetryArgs {
    /// Task ID
    pub task_id: String,
    /// Legacy positional task ID for `task retry <project> <task-id>`
    #[arg(hide = true)]
    pub legacy_task_id: Option<String>,
    /// Reset retry counter before requeueing
    #[arg(long)]
    pub reset_count: bool,
    /// Retry mode (`clean` or `continue`)
    #[arg(long, value_enum, default_value_t = TaskRetryMode::Clean)]
    pub mode: TaskRetryMode,
}

#[derive(Args)]
pub struct TaskAbortArgs {
    /// Task ID
    pub task_id: String,
    /// Optional abort reason
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Args)]
pub struct FlowStatusArgs {
    /// Flow ID
    pub flow_id: String,
}

/// Project subcommands.
#[derive(Subcommand)]
pub enum ProjectCommands {
    /// Create a new project
    Create(ProjectCreateArgs),

    /// List all projects
    List,

    /// Show project details
    Inspect(ProjectInspectArgs),

    /// Update a project
    Update(ProjectUpdateArgs),

    /// Configure the runtime adapter used to execute `TaskFlows`
    RuntimeSet(ProjectRuntimeSetArgs),

    /// Attach a repository to a project
    AttachRepo(AttachRepoArgs),

    /// Detach a repository from a project
    DetachRepo(DetachRepoArgs),

    /// Delete a project with no remaining tasks, graphs, or flows
    Delete(ProjectDeleteArgs),
    /// Governance storage lifecycle commands
    #[command(subcommand)]
    Governance(ProjectGovernanceCommands),
}

/// Arguments for project create.
#[derive(Args)]
pub struct ProjectCreateArgs {
    /// Project name
    pub name: String,

    /// Project description
    #[arg(long, short = 'd')]
    pub description: Option<String>,
}

#[derive(Args)]
pub struct ProjectRuntimeSetArgs {
    /// Project ID or name
    pub project: String,

    /// Runtime adapter name (default: opencode)
    #[arg(long, default_value = "opencode")]
    pub adapter: String,

    /// Path to the runtime binary (default: opencode)
    #[arg(long, default_value = "opencode")]
    pub binary_path: String,

    /// Optional model identifier for the runtime (adapter-specific)
    #[arg(long)]
    pub model: Option<String>,

    /// Extra args to pass to the runtime (repeatable)
    #[arg(long = "arg", allow_hyphen_values = true)]
    pub args: Vec<String>,

    /// Extra environment variables for the runtime in KEY=VALUE form (repeatable).
    /// Empty keys are rejected, empty values are allowed (KEY=), and duplicate keys use the last value.
    #[arg(long = "env")]
    pub env: Vec<String>,

    /// Execution timeout in milliseconds
    #[arg(long, default_value = "600000")]
    pub timeout_ms: u64,

    /// Max number of tasks that may execute concurrently per flow tick
    #[arg(long, default_value_t = 1)]
    pub max_parallel_tasks: u16,

    /// Runtime role
    #[arg(long, value_enum, default_value = "worker")]
    pub role: RuntimeRoleArg,
}

/// Arguments for project inspect.
#[derive(Args)]
pub struct ProjectInspectArgs {
    /// Project ID or name
    pub project: String,
}

/// Arguments for project update.
#[derive(Args)]
pub struct ProjectUpdateArgs {
    /// Project ID or name
    pub project: String,

    /// New name
    #[arg(long)]
    pub name: Option<String>,

    /// New description
    #[arg(long, short = 'd')]
    pub description: Option<String>,
}

/// Arguments for attaching a repository.
#[derive(Args)]
pub struct AttachRepoArgs {
    /// Project ID or name
    pub project: String,

    /// Path to the git repository
    pub path: String,

    /// Optional repository name override
    #[arg(long)]
    pub name: Option<String>,

    /// Access mode (ro|rw)
    #[arg(long, default_value = "rw")]
    pub access: String,
}

/// Arguments for detaching a repository.
#[derive(Args)]
pub struct DetachRepoArgs {
    /// Project ID or name
    pub project: String,

    /// Repository name
    pub repo_name: String,
}

#[derive(Args)]
pub struct ProjectDeleteArgs {
    /// Project ID or name
    pub project: String,
}

/// Verify subcommands.
#[derive(Subcommand)]
pub enum VerifyCommands {
    /// Apply a human override to verification outcome for a task
    Override(VerifyOverrideArgs),

    /// Manually trigger verification for a task in an active flow
    Run(VerifyRunArgs),

    /// Show verification results for an attempt
    Results(VerifyResultsArgs),
}

/// Arguments for verify override.
#[derive(Args)]
pub struct VerifyOverrideArgs {
    /// Task ID
    pub task_id: String,

    /// Decision: pass or fail
    pub decision: String,

    /// Reason for override
    #[arg(long)]
    pub reason: String,
}

#[derive(Args)]
pub struct VerifyRunArgs {
    /// Task ID
    pub task_id: String,
}

#[derive(Args)]
pub struct VerifyResultsArgs {
    /// Attempt ID
    pub attempt_id: String,

    /// Include full check output payloads
    #[arg(long)]
    pub output: bool,
}

/// Merge subcommands.
#[derive(Subcommand)]
pub enum MergeCommands {
    #[command(
        about = "Prepare a completed flow for integration (sandbox merge)",
        long_about = "Prepare a completed flow for merge by building an integration sandbox, applying successful task branches, and running required integration checks before approval."
    )]
    Prepare(MergePrepareArgs),

    #[command(
        about = "Approve a prepared merge (explicit human gate)",
        long_about = "Record the approving HUMAN user (HIVEMIND_USER or USER), ensure no unresolved conflicts/check failures remain, and emit MergeApproved so execute can proceed."
    )]
    Approve(MergeApproveArgs),

    #[command(
        about = "Execute an approved merge (fast-forward target)",
        long_about = "Acquire the per-flow integration lock and fast-forward the chosen target branch from integration/<flow>/prepare, then clean up integration/<flow>/prepare, integration/<flow>/<task>, _integration_prepare, and exec branches. Emits MergeCompleted."
    )]
    Execute(MergeExecuteArgs),
}

/// Arguments for merge prepare.
#[derive(Args)]
pub struct MergePrepareArgs {
    /// Flow ID
    pub flow_id: String,

    /// Target branch (set explicitly if your default branch is not 'main')
    #[arg(long)]
    pub target: Option<String>,
}

/// Arguments for merge approve.
#[derive(Args)]
pub struct MergeApproveArgs {
    /// Flow ID
    pub flow_id: String,
}

/// Arguments for merge execute.
#[derive(Args)]
pub struct MergeExecuteArgs {
    /// Flow ID
    pub flow_id: String,

    /// Merge execution mode (`local` or `pr`)
    #[arg(long, value_enum, default_value = "local")]
    pub mode: MergeExecuteModeArg,

    /// For `pr` mode: wait for CI checks to complete
    #[arg(long)]
    pub monitor_ci: bool,

    /// For `pr` mode: request automatic squash merge
    #[arg(long)]
    pub auto_merge: bool,

    /// After merge, pull target branch from origin
    #[arg(long)]
    pub pull_after: bool,
}

/// Attempt subcommands.
#[derive(Subcommand)]
pub enum AttemptCommands {
    /// List attempts (optionally filtered)
    List(AttemptListArgs),
    /// Inspect an attempt
    Inspect(AttemptInspectArgs),
}

/// Arguments for attempt list.
#[derive(Args)]
pub struct AttemptListArgs {
    /// Optional flow ID filter
    #[arg(long)]
    pub flow: Option<String>,
    /// Optional task ID filter
    #[arg(long)]
    pub task: Option<String>,
    /// Maximum number of attempts
    #[arg(long, default_value = "50")]
    pub limit: usize,
}

/// Arguments for attempt inspect.
#[derive(Args)]
pub struct AttemptInspectArgs {
    /// Attempt ID
    pub attempt_id: String,

    /// Show assembled attempt context (manifest + retry context when present)
    #[arg(long)]
    pub context: bool,

    /// Show changes diff
    #[arg(long)]
    pub diff: bool,

    /// Show runtime output
    #[arg(long)]
    pub output: bool,
}
