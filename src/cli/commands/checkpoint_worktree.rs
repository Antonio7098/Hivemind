use super::*;

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
