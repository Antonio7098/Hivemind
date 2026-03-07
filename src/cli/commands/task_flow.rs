use super::*;

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
