//! CLI command definitions.
//!
//! All features are accessible via CLI. The UI is a projection, not a controller.

use super::output::OutputFormat;
use clap::{Args, Parser, Subcommand};

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
    /// Complete the currently active checkpoint for an attempt
    Complete(CheckpointCompleteArgs),
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
}

#[derive(Subcommand)]
pub enum GraphCommands {
    /// Create a new task graph from project tasks
    Create(GraphCreateArgs),
    /// Add a dependency edge to a draft graph (fails once graph is locked by a flow)
    AddDependency(GraphAddDependencyArgs),
    /// Add a verification check to a task in a draft graph
    AddCheck(GraphAddCheckArgs),
    /// Validate a graph for cycles and structural issues
    Validate(GraphValidateArgs),
    /// List graphs (optionally filtered by project)
    List(GraphListArgs),
}

#[derive(Args)]
pub struct GraphCreateArgs {
    /// Project ID or name
    pub project: String,
    /// Human-friendly graph name
    pub name: String,

    /// Task IDs to include in the graph
    #[arg(long, num_args = 1..)]
    pub from_tasks: Vec<String>,
}

#[derive(Args)]
pub struct GraphAddDependencyArgs {
    /// Graph ID
    pub graph_id: String,
    /// Dependent task. Semantics: `from_task` depends on `to_task`.
    pub from_task: String,
    /// Dependency task. Semantics: `from_task` depends on `to_task`.
    pub to_task: String,
}

#[derive(Args)]
pub struct GraphAddCheckArgs {
    /// Graph ID
    pub graph_id: String,
    /// Task ID within the graph
    pub task_id: String,

    /// Check name
    #[arg(long)]
    pub name: String,

    /// Shell command to execute for the check
    #[arg(long)]
    pub command: String,

    /// Whether this check is required for verification success
    #[arg(long, default_value_t = true)]
    pub required: bool,

    /// Optional timeout in milliseconds for this check
    #[arg(long)]
    pub timeout_ms: Option<u64>,
}

#[derive(Args)]
pub struct GraphValidateArgs {
    /// Graph ID
    pub graph_id: String,
}

#[derive(Args)]
pub struct GraphListArgs {
    /// Optional project ID or name to filter graphs
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Subcommand)]
pub enum FlowCommands {
    /// Create a new flow from a graph (locks the graph)
    Create(FlowCreateArgs),
    /// List flows (optionally filtered by project)
    List(FlowListArgs),
    /// Start a flow (transitions it into running state)
    Start(FlowStartArgs),
    /// Advance execution by one scheduling/execution step
    Tick(FlowTickArgs),
    /// Pause scheduling new tasks (running tasks may continue)
    Pause(FlowPauseArgs),
    /// Resume a paused flow
    Resume(FlowResumeArgs),
    /// Abort a flow
    Abort(FlowAbortArgs),
    /// Show flow state and per-task execution state
    Status(FlowStatusArgs),
}

#[derive(Args)]
pub struct FlowCreateArgs {
    /// Graph ID
    pub graph_id: String,
    /// Optional flow name
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Args)]
pub struct FlowListArgs {
    /// Optional project ID or name to filter flows
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct FlowStartArgs {
    /// Flow ID
    pub flow_id: String,
}

#[derive(Args)]
pub struct FlowTickArgs {
    /// Flow ID
    pub flow_id: String,

    /// Enable interactive mode (prompt between steps)
    #[arg(long)]
    pub interactive: bool,
}

#[derive(Args)]
pub struct FlowPauseArgs {
    /// Flow ID
    pub flow_id: String,
    /// If set, wait for running tasks to finish before returning
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args)]
pub struct FlowResumeArgs {
    /// Flow ID
    pub flow_id: String,
}

#[derive(Args)]
pub struct FlowAbortArgs {
    /// Flow ID
    pub flow_id: String,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TaskRetryMode {
    Clean,
    Continue,
}

#[derive(Args)]
pub struct TaskRetryArgs {
    pub task_id: String,
    #[arg(long)]
    pub reset_count: bool,
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

/// Task subcommands.
#[derive(Subcommand)]
pub enum TaskCommands {
    /// Create a new task
    Create(TaskCreateArgs),

    /// List tasks in a project
    List(TaskListArgs),

    /// Show task details
    Inspect(TaskInspectArgs),

    /// Update a task
    Update(TaskUpdateArgs),

    /// Close a task
    Close(TaskCloseArgs),

    /// Start executing a ready task
    Start(TaskStartArgs),
    /// Mark a running task as complete
    Complete(TaskCompleteArgs),

    /// Request a retry for a failed task
    Retry(TaskRetryArgs),
    /// Abort a task within its flow
    Abort(TaskAbortArgs),
}

#[derive(Args)]
pub struct TaskStartArgs {
    /// Task ID
    pub task_id: String,
}

#[derive(Args)]
pub struct TaskCompleteArgs {
    /// Task ID
    pub task_id: String,
}

/// Arguments for task create.
#[derive(Args)]
pub struct TaskCreateArgs {
    /// Project ID or name
    pub project: String,

    /// Task title
    pub title: String,

    /// Task description
    #[arg(long, short = 'd')]
    pub description: Option<String>,

    /// Scope contract as a JSON string.
    /// Required shape: {"filesystem":{"rules":[...]},"repositories":[],"git":{"permissions":[]},"execution":{"allowed":[],"denied":[]}}
    #[arg(long)]
    pub scope: Option<String>,
}

/// Arguments for task list.
#[derive(Args)]
pub struct TaskListArgs {
    /// Project ID or name
    pub project: String,

    /// Filter by state
    #[arg(long)]
    pub state: Option<String>,
}

/// Arguments for task inspect.
#[derive(Args)]
pub struct TaskInspectArgs {
    /// Task ID
    pub task_id: String,
}

/// Arguments for task update.
#[derive(Args)]
pub struct TaskUpdateArgs {
    /// Task ID
    pub task_id: String,

    /// New title
    #[arg(long)]
    pub title: Option<String>,

    /// New description
    #[arg(long, short = 'd')]
    pub description: Option<String>,
}

/// Arguments for task close.
#[derive(Args)]
pub struct TaskCloseArgs {
    /// Task ID
    pub task_id: String,

    /// Optional reason
    #[arg(long)]
    pub reason: Option<String>,
}

/// Event subcommands.
#[derive(Subcommand)]
pub enum EventCommands {
    /// List events
    List(EventListArgs),

    /// Show event details
    Inspect(EventInspectArgs),

    /// Stream events with filters
    Stream(EventStreamArgs),

    /// Replay events to reconstruct state
    Replay(EventReplayArgs),
}

/// Arguments for event list.
#[derive(Args)]
pub struct EventListArgs {
    /// Filter by project
    #[arg(long)]
    pub project: Option<String>,

    /// Maximum number of events
    #[arg(long, default_value = "50")]
    pub limit: usize,
}

/// Arguments for event inspect.
#[derive(Args)]
pub struct EventInspectArgs {
    /// Event ID
    pub event_id: String,
}

/// Arguments for event stream.
#[derive(Args)]
pub struct EventStreamArgs {
    /// Filter by flow ID
    #[arg(long)]
    pub flow: Option<String>,

    /// Filter by task ID
    #[arg(long)]
    pub task: Option<String>,

    /// Filter by project
    #[arg(long)]
    pub project: Option<String>,

    /// Filter by graph ID
    #[arg(long)]
    pub graph: Option<String>,

    /// Maximum number of events
    #[arg(long, default_value = "100")]
    pub limit: usize,
}

/// Arguments for event replay.
#[derive(Args)]
pub struct EventReplayArgs {
    /// Flow ID to replay
    pub flow_id: String,

    /// Verify replayed state against current state
    #[arg(long)]
    pub verify: bool,
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
}

/// Attempt subcommands.
#[derive(Subcommand)]
pub enum AttemptCommands {
    /// Inspect an attempt
    Inspect(AttemptInspectArgs),
}

/// Arguments for attempt inspect.
#[derive(Args)]
pub struct AttemptInspectArgs {
    /// Attempt ID
    pub attempt_id: String,

    /// Show retry context
    #[arg(long)]
    pub context: bool,

    /// Show changes diff
    #[arg(long)]
    pub diff: bool,

    /// Show runtime output
    #[arg(long)]
    pub output: bool,
}
