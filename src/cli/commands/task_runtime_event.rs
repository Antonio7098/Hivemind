use super::*;

#[derive(Subcommand)]
pub enum TaskCommands {
    Create(TaskCreateArgs),
    List(TaskListArgs),
    Inspect(TaskInspectArgs),
    Update(TaskUpdateArgs),
    RuntimeSet(TaskRuntimeSetArgs),
    Close(TaskCloseArgs),
    Start(TaskStartArgs),
    Complete(TaskCompleteArgs),
    Retry(TaskRetryArgs),
    Abort(TaskAbortArgs),
    SetRunMode(TaskSetRunModeArgs),
    Delete(TaskDeleteArgs),
}

#[derive(Args)]
pub struct TaskStartArgs {
    pub task_id: String,
    #[arg(hide = true)]
    pub legacy_task_id: Option<String>,
}

#[derive(Args)]
pub struct TaskCompleteArgs {
    pub task_id: String,
    #[arg(hide = true)]
    pub legacy_task_id: Option<String>,
    #[arg(long, hide = true)]
    pub success: Option<bool>,
    #[arg(long, hide = true)]
    pub message: Option<String>,
}

#[derive(Args)]
pub struct TaskCreateArgs {
    pub project: String,
    pub title: String,
    #[arg(long, short = 'd')]
    pub description: Option<String>,
    #[arg(long)]
    pub scope: Option<String>,
}

#[derive(Args)]
pub struct TaskListArgs {
    pub project: String,
    #[arg(long)]
    pub state: Option<String>,
}

#[derive(Args)]
pub struct TaskInspectArgs {
    pub task_id: String,
}

#[derive(Args)]
pub struct TaskUpdateArgs {
    pub task_id: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long, short = 'd')]
    pub description: Option<String>,
}

#[derive(Args)]
pub struct TaskRuntimeSetArgs {
    pub task_id: String,
    #[arg(long)]
    pub clear: bool,
    #[arg(long, default_value = "opencode")]
    pub adapter: String,
    #[arg(long, default_value = "opencode")]
    pub binary_path: String,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long = "arg", allow_hyphen_values = true)]
    pub args: Vec<String>,
    #[arg(long = "env")]
    pub env: Vec<String>,
    #[arg(long, default_value = "600000")]
    pub timeout_ms: u64,
    #[arg(long, value_enum, default_value = "worker")]
    pub role: RuntimeRoleArg,
}

#[derive(Args)]
pub struct TaskSetRunModeArgs {
    pub task_id: String,
    #[arg(value_enum)]
    pub mode: RunModeArg,
}

#[derive(Args)]
pub struct TaskDeleteArgs {
    pub task_id: String,
}

#[derive(Subcommand)]
pub enum RuntimeCommands {
    List,
    Health(RuntimeHealthArgs),
    DefaultsSet(RuntimeDefaultsSetArgs),
}

#[derive(Args)]
pub struct RuntimeHealthArgs {
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub task: Option<String>,
    #[arg(long)]
    pub flow: Option<String>,
    #[arg(long, value_enum, default_value = "worker")]
    pub role: RuntimeRoleArg,
}

#[derive(Args)]
pub struct RuntimeDefaultsSetArgs {
    #[arg(long, value_enum, default_value = "worker")]
    pub role: RuntimeRoleArg,
    #[arg(long, default_value = "opencode")]
    pub adapter: String,
    #[arg(long, default_value = "opencode")]
    pub binary_path: String,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long = "arg", allow_hyphen_values = true)]
    pub args: Vec<String>,
    #[arg(long = "env")]
    pub env: Vec<String>,
    #[arg(long, default_value = "600000")]
    pub timeout_ms: u64,
    #[arg(long, default_value_t = 1)]
    pub max_parallel_tasks: u16,
}

#[derive(Args)]
pub struct TaskCloseArgs {
    pub task_id: String,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Subcommand)]
pub enum EventCommands {
    List(EventListArgs),
    Inspect(EventInspectArgs),
    Stream(EventStreamArgs),
    Replay(EventReplayArgs),
    Verify(EventVerifyArgs),
    Recover(EventRecoverArgs),
}

#[derive(Args)]
pub struct EventListArgs {
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub graph: Option<String>,
    #[arg(long)]
    pub flow: Option<String>,
    #[arg(long)]
    pub task: Option<String>,
    #[arg(long)]
    pub attempt: Option<String>,
    #[arg(long = "artifact-id")]
    pub artifact_id: Option<String>,
    #[arg(long = "template-id")]
    pub template_id: Option<String>,
    #[arg(long = "rule-id")]
    pub rule_id: Option<String>,
    #[arg(long = "error-type")]
    pub error_type: Option<String>,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub until: Option<String>,
    #[arg(long, default_value = "200")]
    pub limit: usize,
}

#[derive(Args)]
pub struct EventInspectArgs {
    pub event_id: String,
}

#[derive(Args)]
pub struct EventStreamArgs {
    #[arg(long)]
    pub flow: Option<String>,
    #[arg(long)]
    pub task: Option<String>,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub graph: Option<String>,
    #[arg(long)]
    pub attempt: Option<String>,
    #[arg(long = "artifact-id")]
    pub artifact_id: Option<String>,
    #[arg(long = "template-id")]
    pub template_id: Option<String>,
    #[arg(long = "rule-id")]
    pub rule_id: Option<String>,
    #[arg(long = "error-type")]
    pub error_type: Option<String>,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub until: Option<String>,
    #[arg(long, default_value = "100")]
    pub limit: usize,
    #[arg(long, default_value_t = false)]
    pub redact_secrets: bool,
}

#[derive(Args)]
pub struct EventReplayArgs {
    pub flow_id: String,
    #[arg(long)]
    pub verify: bool,
}

#[derive(Args, Default)]
pub struct EventVerifyArgs {}

#[derive(Args)]
pub struct EventRecoverArgs {
    #[arg(long, default_value_t = false)]
    pub from_mirror: bool,
    #[arg(long, default_value_t = false)]
    pub confirm: bool,
}
