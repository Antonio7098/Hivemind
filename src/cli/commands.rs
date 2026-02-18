//! CLI command definitions.
//!
//! All features are accessible via CLI. The UI is a projection, not a controller.

use super::output::OutputFormat;
use clap::{Args, Parser, Subcommand};

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

/// Constitution subcommands.
#[derive(Subcommand)]
pub enum ConstitutionCommands {
    /// Initialize a project constitution (requires explicit confirmation)
    Init(ConstitutionInitArgs),
    /// Show the current project constitution
    Show(ConstitutionShowArgs),
    /// Validate the current project constitution schema and semantics
    Validate(ConstitutionValidateArgs),
    /// Evaluate constitution rules against the current graph snapshot
    Check(ConstitutionCheckArgs),
    /// Update the current project constitution (requires explicit confirmation)
    Update(ConstitutionUpdateArgs),
}

#[derive(Args)]
pub struct ConstitutionInitArgs {
    /// Project ID or name
    pub project: String,

    /// Optional YAML content payload for initialization
    #[arg(long, conflicts_with = "from_file")]
    pub content: Option<String>,

    /// Optional path to YAML constitution content
    #[arg(long = "from-file", conflicts_with = "content")]
    pub from_file: Option<String>,

    /// Explicit confirmation gate for constitution mutation
    #[arg(long, default_value_t = false)]
    pub confirm: bool,

    /// Optional actor attribution override (defaults to OS user)
    #[arg(long)]
    pub actor: Option<String>,

    /// Optional mutation intent note for audit trail
    #[arg(long)]
    pub intent: Option<String>,
}

#[derive(Args)]
pub struct ConstitutionShowArgs {
    /// Project ID or name
    pub project: String,
}

#[derive(Args)]
pub struct ConstitutionValidateArgs {
    /// Project ID or name
    pub project: String,
}

#[derive(Args)]
pub struct ConstitutionCheckArgs {
    /// Project ID or name
    #[arg(long)]
    pub project: String,
}

#[derive(Args)]
pub struct ConstitutionUpdateArgs {
    /// Project ID or name
    pub project: String,

    /// Inline YAML constitution content
    #[arg(long, conflicts_with = "from_file")]
    pub content: Option<String>,

    /// Path to YAML constitution content
    #[arg(long = "from-file", conflicts_with = "content")]
    pub from_file: Option<String>,

    /// Explicit confirmation gate for constitution mutation
    #[arg(long, default_value_t = false)]
    pub confirm: bool,

    /// Optional actor attribution override (defaults to OS user)
    #[arg(long)]
    pub actor: Option<String>,

    /// Optional mutation intent note for audit trail
    #[arg(long)]
    pub intent: Option<String>,
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

#[derive(Subcommand)]
pub enum GraphCommands {
    /// Create a new task graph from project tasks
    Create(GraphCreateArgs),
    /// Code graph snapshot lifecycle commands
    #[command(subcommand)]
    Snapshot(GraphSnapshotCommands),
    /// Add a dependency edge to a draft graph (fails once graph is locked by a flow)
    AddDependency(GraphAddDependencyArgs),
    /// Add a verification check to a task in a draft graph
    AddCheck(GraphAddCheckArgs),
    /// Validate a graph for cycles and structural issues
    Validate(GraphValidateArgs),
    /// List graphs (optionally filtered by project)
    List(GraphListArgs),
    /// Delete a graph that is not referenced by any flow
    Delete(GraphDeleteArgs),
}

#[derive(Subcommand)]
pub enum GraphSnapshotCommands {
    /// Rebuild the UCP-backed static code graph snapshot for a project
    Refresh(GraphSnapshotRefreshArgs),
}

#[derive(Args)]
pub struct GraphSnapshotRefreshArgs {
    /// Project ID or name
    pub project: String,
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
    /// Upstream task. Semantics: `to_task` depends on `from_task`.
    pub from_task: String,
    /// Downstream task that depends on `from_task`.
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

#[derive(Args)]
pub struct GraphDeleteArgs {
    /// Graph ID
    pub graph_id: String,
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
    /// Restart an aborted flow by creating a new flow from the same graph
    Restart(FlowRestartArgs),
    /// Show flow state and per-task execution state
    Status(FlowStatusArgs),
    /// Set flow run mode (`manual` or `auto`)
    SetRunMode(FlowSetRunModeArgs),
    /// Add a dependency between flows (`flow` depends on `depends_on_flow`)
    AddDependency(FlowAddDependencyArgs),
    /// Configure or clear a flow-level runtime default by role
    RuntimeSet(FlowRuntimeSetArgs),
    /// Delete a flow that is not currently active
    Delete(FlowDeleteArgs),
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

    /// Deprecated: interactive mode is no longer supported
    #[arg(long)]
    pub interactive: bool,

    /// Override max tasks to schedule this tick (must be >= 1)
    #[arg(long)]
    pub max_parallel: Option<u16>,
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
pub struct FlowRestartArgs {
    /// Aborted flow ID
    pub flow_id: String,
    /// Optional name for the restarted flow
    #[arg(long)]
    pub name: Option<String>,
    /// Start the restarted flow immediately
    #[arg(long, default_value_t = false)]
    pub start: bool,
}

#[derive(Args)]
pub struct FlowSetRunModeArgs {
    /// Flow ID
    pub flow_id: String,
    /// Run mode
    #[arg(value_enum)]
    pub mode: RunModeArg,
}

#[derive(Args)]
pub struct FlowAddDependencyArgs {
    /// Dependent flow ID
    pub flow_id: String,
    /// Required upstream flow ID
    pub depends_on_flow_id: String,
}

#[derive(Args)]
pub struct FlowRuntimeSetArgs {
    /// Flow ID
    pub flow_id: String,

    /// Clear existing flow-level runtime default for role
    #[arg(long)]
    pub clear: bool,

    /// Runtime role
    #[arg(long, value_enum, default_value = "worker")]
    pub role: RuntimeRoleArg,

    /// Runtime adapter name
    #[arg(long, default_value = "opencode")]
    pub adapter: String,

    /// Path to runtime binary
    #[arg(long, default_value = "opencode")]
    pub binary_path: String,

    /// Optional model identifier
    #[arg(long)]
    pub model: Option<String>,

    /// Extra args to pass to runtime
    #[arg(long = "arg", allow_hyphen_values = true)]
    pub args: Vec<String>,

    /// Extra environment variables in KEY=VALUE form
    #[arg(long = "env")]
    pub env: Vec<String>,

    /// Execution timeout in milliseconds
    #[arg(long, default_value = "600000")]
    pub timeout_ms: u64,

    /// Max number of tasks that may execute concurrently per flow tick
    #[arg(long, default_value_t = 1)]
    pub max_parallel_tasks: u16,
}

#[derive(Args)]
pub struct FlowDeleteArgs {
    /// Flow ID
    pub flow_id: String,
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

    /// Delete a project with no remaining tasks, graphs, or flows
    Delete(ProjectDeleteArgs),
    /// Governance storage lifecycle commands
    #[command(subcommand)]
    Governance(ProjectGovernanceCommands),
}

/// Project governance subcommands.
#[derive(Subcommand)]
pub enum ProjectGovernanceCommands {
    /// Initialize governance storage layout for a project
    Init(ProjectGovernanceInitArgs),
    /// Migrate legacy governance artifacts to the canonical global layout
    Migrate(ProjectGovernanceMigrateArgs),
    /// Inspect governance storage paths and projection metadata for a project
    Inspect(ProjectGovernanceInspectArgs),
    /// Diagnose governance artifacts, references, and snapshot freshness
    Diagnose(ProjectGovernanceDiagnoseArgs),
    /// Project document lifecycle and metadata commands
    #[command(subcommand)]
    Document(ProjectGovernanceDocumentCommands),
    /// Task-level document attachment inclusion/exclusion controls
    #[command(subcommand)]
    Attachment(ProjectGovernanceAttachmentCommands),
    /// Project notepad lifecycle commands (non-executional)
    #[command(subcommand)]
    Notepad(ProjectGovernanceNotepadCommands),
}

/// Global governance artifact commands.
#[derive(Subcommand)]
pub enum GlobalCommands {
    /// Global skill registry lifecycle commands
    #[command(subcommand)]
    Skill(GlobalSkillCommands),
    /// Global system prompt registry lifecycle commands
    #[command(subcommand)]
    SystemPrompt(GlobalSystemPromptCommands),
    /// Global template registry and instantiation commands
    #[command(subcommand)]
    Template(GlobalTemplateCommands),
    /// Global notepad lifecycle commands (non-executional)
    #[command(subcommand)]
    Notepad(GlobalNotepadCommands),
}

#[derive(Subcommand)]
pub enum ProjectGovernanceDocumentCommands {
    /// Create a project document with metadata and revision history
    Create(ProjectGovernanceDocumentCreateArgs),
    /// List project documents
    List(ProjectGovernanceDocumentListArgs),
    /// Inspect a single project document and revision history
    Inspect(ProjectGovernanceDocumentInspectArgs),
    /// Update a project document (creates a new immutable revision)
    Update(ProjectGovernanceDocumentUpdateArgs),
    /// Delete a project document
    Delete(ProjectGovernanceDocumentDeleteArgs),
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentCreateArgs {
    /// Project ID or name
    pub project: String,
    /// Stable document identifier
    pub document_id: String,
    /// Human title for the document
    #[arg(long)]
    pub title: String,
    /// Document owner metadata
    #[arg(long)]
    pub owner: String,
    /// Tags (repeatable)
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    /// Document body content
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentListArgs {
    /// Project ID or name
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentInspectArgs {
    /// Project ID or name
    pub project: String,
    /// Stable document identifier
    pub document_id: String,
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentUpdateArgs {
    /// Project ID or name
    pub project: String,
    /// Stable document identifier
    pub document_id: String,
    /// Optional title override
    #[arg(long)]
    pub title: Option<String>,
    /// Optional owner override
    #[arg(long)]
    pub owner: Option<String>,
    /// Optional tags override (repeatable, when provided)
    #[arg(long = "tag")]
    pub tags: Option<Vec<String>>,
    /// Optional content update (creates next revision)
    #[arg(long)]
    pub content: Option<String>,
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentDeleteArgs {
    /// Project ID or name
    pub project: String,
    /// Stable document identifier
    pub document_id: String,
}

#[derive(Subcommand)]
pub enum ProjectGovernanceAttachmentCommands {
    /// Explicitly include a project document for task execution context
    Include(ProjectGovernanceAttachmentSetArgs),
    /// Explicitly exclude a project document from task execution context
    Exclude(ProjectGovernanceAttachmentSetArgs),
}

#[derive(Args)]
pub struct ProjectGovernanceAttachmentSetArgs {
    /// Project ID or name
    pub project: String,
    /// Task ID
    pub task_id: String,
    /// Document identifier
    pub document_id: String,
}

#[derive(Subcommand)]
pub enum ProjectGovernanceNotepadCommands {
    /// Create project notepad content
    Create(ProjectGovernanceNotepadCreateArgs),
    /// Show project notepad content
    Show(ProjectGovernanceNotepadShowArgs),
    /// Update project notepad content
    Update(ProjectGovernanceNotepadUpdateArgs),
    /// Delete project notepad content
    Delete(ProjectGovernanceNotepadDeleteArgs),
}

#[derive(Args)]
pub struct ProjectGovernanceNotepadCreateArgs {
    /// Project ID or name
    pub project: String,
    /// Notepad content
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct ProjectGovernanceNotepadShowArgs {
    /// Project ID or name
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceNotepadUpdateArgs {
    /// Project ID or name
    pub project: String,
    /// Notepad content
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct ProjectGovernanceNotepadDeleteArgs {
    /// Project ID or name
    pub project: String,
}

#[derive(Subcommand)]
pub enum GlobalSkillCommands {
    /// Create a global reusable skill artifact
    Create(GlobalSkillCreateArgs),
    /// List global skill artifacts
    List,
    /// Inspect a single global skill artifact
    Inspect(GlobalSkillInspectArgs),
    /// Update a global skill artifact
    Update(GlobalSkillUpdateArgs),
    /// Delete a global skill artifact
    Delete(GlobalSkillDeleteArgs),
}

#[derive(Args)]
pub struct GlobalSkillCreateArgs {
    /// Skill identifier
    pub skill_id: String,
    /// Skill display name
    #[arg(long)]
    pub name: String,
    /// Skill body content
    #[arg(long)]
    pub content: String,
    /// Skill tags (repeatable)
    #[arg(long = "tag")]
    pub tags: Vec<String>,
}

#[derive(Args)]
pub struct GlobalSkillInspectArgs {
    /// Skill identifier
    pub skill_id: String,
}

#[derive(Args)]
pub struct GlobalSkillUpdateArgs {
    /// Skill identifier
    pub skill_id: String,
    /// Optional display name update
    #[arg(long)]
    pub name: Option<String>,
    /// Optional content update
    #[arg(long)]
    pub content: Option<String>,
    /// Optional tags update (repeatable, when provided)
    #[arg(long = "tag")]
    pub tags: Option<Vec<String>>,
}

#[derive(Args)]
pub struct GlobalSkillDeleteArgs {
    /// Skill identifier
    pub skill_id: String,
}

#[derive(Subcommand)]
pub enum GlobalSystemPromptCommands {
    /// Create a global system prompt artifact
    Create(GlobalSystemPromptCreateArgs),
    /// List global system prompt artifacts
    List,
    /// Inspect a global system prompt artifact
    Inspect(GlobalSystemPromptInspectArgs),
    /// Update a global system prompt artifact
    Update(GlobalSystemPromptUpdateArgs),
    /// Delete a global system prompt artifact
    Delete(GlobalSystemPromptDeleteArgs),
}

#[derive(Args)]
pub struct GlobalSystemPromptCreateArgs {
    /// System prompt identifier
    pub prompt_id: String,
    /// System prompt body content
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct GlobalSystemPromptInspectArgs {
    /// System prompt identifier
    pub prompt_id: String,
}

#[derive(Args)]
pub struct GlobalSystemPromptUpdateArgs {
    /// System prompt identifier
    pub prompt_id: String,
    /// Updated prompt body content
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct GlobalSystemPromptDeleteArgs {
    /// System prompt identifier
    pub prompt_id: String,
}

#[derive(Subcommand)]
pub enum GlobalTemplateCommands {
    /// Create a global template artifact
    Create(GlobalTemplateCreateArgs),
    /// List global template artifacts
    List,
    /// Inspect a global template artifact
    Inspect(GlobalTemplateInspectArgs),
    /// Update a global template artifact
    Update(GlobalTemplateUpdateArgs),
    /// Delete a global template artifact
    Delete(GlobalTemplateDeleteArgs),
    /// Instantiate template references for a project and emit resolved artifact event
    Instantiate(GlobalTemplateInstantiateArgs),
}

#[derive(Args)]
pub struct GlobalTemplateCreateArgs {
    /// Template identifier
    pub template_id: String,
    /// Required global system prompt identifier
    #[arg(long)]
    pub system_prompt_id: String,
    /// Referenced global skill identifiers (repeatable)
    #[arg(long = "skill-id")]
    pub skill_ids: Vec<String>,
    /// Referenced project document identifiers (repeatable)
    #[arg(long = "document-id")]
    pub document_ids: Vec<String>,
    /// Optional template description/body
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Args)]
pub struct GlobalTemplateInspectArgs {
    /// Template identifier
    pub template_id: String,
}

#[derive(Args)]
pub struct GlobalTemplateUpdateArgs {
    /// Template identifier
    pub template_id: String,
    /// Optional system prompt identifier override
    #[arg(long)]
    pub system_prompt_id: Option<String>,
    /// Optional skill identifiers override (repeatable, when provided)
    #[arg(long = "skill-id")]
    pub skill_ids: Option<Vec<String>>,
    /// Optional document identifiers override (repeatable, when provided)
    #[arg(long = "document-id")]
    pub document_ids: Option<Vec<String>>,
    /// Optional template description/body override
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Args)]
pub struct GlobalTemplateDeleteArgs {
    /// Template identifier
    pub template_id: String,
}

#[derive(Args)]
pub struct GlobalTemplateInstantiateArgs {
    /// Project ID or name used for document resolution
    pub project: String,
    /// Template identifier
    pub template_id: String,
}

#[derive(Subcommand)]
pub enum GlobalNotepadCommands {
    /// Create global notepad content
    Create(GlobalNotepadCreateArgs),
    /// Show global notepad content
    Show,
    /// Update global notepad content
    Update(GlobalNotepadUpdateArgs),
    /// Delete global notepad content
    Delete,
}

#[derive(Args)]
pub struct GlobalNotepadCreateArgs {
    /// Notepad content
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct GlobalNotepadUpdateArgs {
    /// Notepad content
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct ProjectGovernanceInitArgs {
    /// Project ID or name
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceMigrateArgs {
    /// Project ID or name
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceInspectArgs {
    /// Project ID or name
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceDiagnoseArgs {
    /// Project ID or name
    pub project: String,
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

    /// Configure or clear a task-level runtime override
    RuntimeSet(TaskRuntimeSetArgs),

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
    /// Set task run mode (`manual` or `auto`)
    SetRunMode(TaskSetRunModeArgs),

    /// Delete a task that is not referenced by a graph or flow
    Delete(TaskDeleteArgs),
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

/// Arguments for task runtime override.
#[derive(Args)]
pub struct TaskRuntimeSetArgs {
    /// Task ID
    pub task_id: String,

    /// Clear any existing task runtime override
    #[arg(long)]
    pub clear: bool,

    /// Runtime adapter name
    #[arg(long, default_value = "opencode")]
    pub adapter: String,

    /// Path to runtime binary
    #[arg(long, default_value = "opencode")]
    pub binary_path: String,

    /// Optional model identifier
    #[arg(long)]
    pub model: Option<String>,

    /// Extra args to pass to runtime
    #[arg(long = "arg", allow_hyphen_values = true)]
    pub args: Vec<String>,

    /// Extra environment variables in KEY=VALUE form
    #[arg(long = "env")]
    pub env: Vec<String>,

    /// Execution timeout in milliseconds
    #[arg(long, default_value = "600000")]
    pub timeout_ms: u64,

    /// Runtime role
    #[arg(long, value_enum, default_value = "worker")]
    pub role: RuntimeRoleArg,
}

#[derive(Args)]
pub struct TaskSetRunModeArgs {
    /// Task ID
    pub task_id: String,
    /// Run mode
    #[arg(value_enum)]
    pub mode: RunModeArg,
}

#[derive(Args)]
pub struct TaskDeleteArgs {
    /// Task ID
    pub task_id: String,
}

/// Runtime subcommands.
#[derive(Subcommand)]
pub enum RuntimeCommands {
    /// List built-in runtime adapters and local availability
    List,
    /// Run runtime adapter health checks
    Health(RuntimeHealthArgs),
    /// Configure global runtime defaults by role
    DefaultsSet(RuntimeDefaultsSetArgs),
}

/// Arguments for runtime health checks.
#[derive(Args)]
pub struct RuntimeHealthArgs {
    /// Optional project ID or name to health-check configured runtime
    #[arg(long)]
    pub project: Option<String>,

    /// Optional task ID to health-check effective runtime (task override or project default)
    #[arg(long)]
    pub task: Option<String>,

    /// Optional flow ID to health-check effective runtime defaults
    #[arg(long)]
    pub flow: Option<String>,

    /// Runtime role to evaluate
    #[arg(long, value_enum, default_value = "worker")]
    pub role: RuntimeRoleArg,
}

#[derive(Args)]
pub struct RuntimeDefaultsSetArgs {
    /// Runtime role
    #[arg(long, value_enum, default_value = "worker")]
    pub role: RuntimeRoleArg,

    /// Runtime adapter name
    #[arg(long, default_value = "opencode")]
    pub adapter: String,

    /// Path to the runtime binary
    #[arg(long, default_value = "opencode")]
    pub binary_path: String,

    /// Optional model identifier for the runtime
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

    /// Max number of tasks that may execute concurrently per flow tick
    #[arg(long, default_value_t = 1)]
    pub max_parallel_tasks: u16,
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

    /// Filter by graph ID
    #[arg(long)]
    pub graph: Option<String>,

    /// Filter by flow ID
    #[arg(long)]
    pub flow: Option<String>,

    /// Filter by task ID
    #[arg(long)]
    pub task: Option<String>,

    /// Filter by attempt ID
    #[arg(long)]
    pub attempt: Option<String>,

    /// Filter by governance artifact ID/key
    #[arg(long = "artifact-id")]
    pub artifact_id: Option<String>,

    /// Filter by template ID
    #[arg(long = "template-id")]
    pub template_id: Option<String>,

    /// Filter by constitution rule ID
    #[arg(long = "rule-id")]
    pub rule_id: Option<String>,

    /// Lower bound timestamp (RFC3339), inclusive
    #[arg(long)]
    pub since: Option<String>,

    /// Upper bound timestamp (RFC3339), inclusive
    #[arg(long)]
    pub until: Option<String>,

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

    /// Filter by attempt ID
    #[arg(long)]
    pub attempt: Option<String>,

    /// Filter by governance artifact ID/key
    #[arg(long = "artifact-id")]
    pub artifact_id: Option<String>,

    /// Filter by template ID
    #[arg(long = "template-id")]
    pub template_id: Option<String>,

    /// Filter by constitution rule ID
    #[arg(long = "rule-id")]
    pub rule_id: Option<String>,

    /// Lower bound timestamp (RFC3339), inclusive
    #[arg(long)]
    pub since: Option<String>,

    /// Upper bound timestamp (RFC3339), inclusive
    #[arg(long)]
    pub until: Option<String>,

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
