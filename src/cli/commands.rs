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
    /// Create a new graph from a set of tasks
    Create(GraphCreateArgs),
    /// Add a dependency edge to a graph
    AddDependency(GraphAddDependencyArgs),
    /// Validate a graph (cycle detection, missing nodes)
    Validate(GraphValidateArgs),
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
    /// Graph ID (positional)
    #[arg(index = 1)]
    pub graph_id: Option<String>,
    /// Dependent task (positional). Semantics: `from_task` depends on `to_task`.
    #[arg(index = 2)]
    pub from_task: Option<String>,
    /// Dependency task (positional). Semantics: `from_task` depends on `to_task`.
    #[arg(index = 3)]
    pub to_task: Option<String>,

    #[arg(long = "graph-id")]
    pub graph_id_flag: Option<String>,
    #[arg(long = "from-task")]
    pub from_task_flag: Option<String>,
    #[arg(long = "to-task")]
    pub to_task_flag: Option<String>,
}

#[derive(Args)]
pub struct GraphValidateArgs {
    /// Graph ID (positional)
    #[arg(index = 1)]
    pub graph_id: Option<String>,

    #[arg(long = "graph-id")]
    pub graph_id_flag: Option<String>,
}

#[derive(Subcommand)]
pub enum FlowCommands {
    /// Create a new flow from a graph (locks the graph)
    Create(FlowCreateArgs),
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
pub struct FlowStartArgs {
    /// Flow ID
    pub flow_id: String,
}

#[derive(Args)]
pub struct FlowTickArgs {
    /// Flow ID
    pub flow_id: String,

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

#[derive(Args)]
pub struct TaskRetryArgs {
    pub task_id: String,
    #[arg(long)]
    pub reset_count: bool,
}

#[derive(Args)]
pub struct TaskAbortArgs {
    pub task_id: String,
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

    /// Extra environment variables for the runtime in KEY=VALUE form (repeatable)
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
    pub task_id: String,
}

#[derive(Args)]
pub struct TaskCompleteArgs {
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
    /// Override verification for a task
    Override(VerifyOverrideArgs),
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

/// Merge subcommands.
#[derive(Subcommand)]
pub enum MergeCommands {
    /// Prepare a merge for a completed flow (creates an integration branch)
    Prepare(MergePrepareArgs),

    /// Approve a prepared merge (explicit human gate)
    Approve(MergeApproveArgs),

    /// Execute an approved merge (fast-forward target branch)
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
