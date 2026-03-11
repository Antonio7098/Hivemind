use super::*;

#[derive(Subcommand)]
pub enum WorkflowCommands {
    Create(WorkflowCreateArgs),
    Update(WorkflowUpdateArgs),
    StepAdd(WorkflowStepAddArgs),
    List(WorkflowListArgs),
    Inspect(WorkflowInspectArgs),
    RunCreate(WorkflowRunCreateArgs),
    RunList(WorkflowRunListArgs),
    Status(WorkflowStatusArgs),
    Start(WorkflowRunIdArgs),
    Complete(WorkflowRunIdArgs),
    Pause(WorkflowRunIdArgs),
    Resume(WorkflowRunIdArgs),
    Abort(WorkflowAbortArgs),
    StepSetState(WorkflowStepSetStateArgs),
}

#[derive(Args)]
pub struct WorkflowCreateArgs {
    pub project: String,
    pub name: String,
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Args)]
pub struct WorkflowUpdateArgs {
    pub workflow_id: String,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub clear_description: bool,
}

#[derive(Args)]
pub struct WorkflowStepAddArgs {
    pub workflow_id: String,
    pub name: String,
    #[arg(long, value_enum, default_value = "task")]
    pub kind: WorkflowStepKindArg,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "depends-on")]
    pub depends_on: Vec<String>,
}

#[derive(Args)]
pub struct WorkflowListArgs {
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct WorkflowInspectArgs {
    pub workflow_id: String,
}

#[derive(Args)]
pub struct WorkflowRunCreateArgs {
    pub workflow_id: String,
}

#[derive(Args)]
pub struct WorkflowRunListArgs {
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub workflow: Option<String>,
}

#[derive(Args)]
pub struct WorkflowStatusArgs {
    pub workflow_run_id: String,
}

#[derive(Args)]
pub struct WorkflowRunIdArgs {
    pub workflow_run_id: String,
}

#[derive(Args)]
pub struct WorkflowAbortArgs {
    pub workflow_run_id: String,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Args)]
pub struct WorkflowStepSetStateArgs {
    pub workflow_run_id: String,
    pub step_id: String,
    #[arg(value_enum)]
    pub state: WorkflowStepStateArg,
    #[arg(long)]
    pub reason: Option<String>,
}
