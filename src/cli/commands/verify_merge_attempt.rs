use super::*;

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
