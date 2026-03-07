use super::*;

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
