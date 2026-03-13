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
    Tick(WorkflowTickArgs),
    Complete(WorkflowRunIdArgs),
    Pause(WorkflowRunIdArgs),
    Resume(WorkflowRunIdArgs),
    Signal(WorkflowSignalArgs),
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
    #[arg(long, allow_hyphen_values = true)]
    pub input_bindings_json: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    pub output_bindings_json: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    pub context_patches_json: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    pub conditional_json: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    pub wait_json: Option<String>,
    #[arg(long)]
    pub child_workflow_id: Option<String>,
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
    #[arg(long)]
    pub context_schema: Option<String>,
    #[arg(long)]
    pub context_schema_version: Option<u32>,
    #[arg(long, allow_hyphen_values = true)]
    pub context_inputs_json: Option<String>,
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
pub struct WorkflowTickArgs {
    pub workflow_run_id: String,
    #[arg(long)]
    pub interactive: bool,
    #[arg(long)]
    pub max_parallel: Option<u16>,
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
pub struct WorkflowSignalArgs {
    pub workflow_run_id: String,
    pub signal_name: String,
    #[arg(long)]
    pub idempotency_key: String,
    #[arg(long, allow_hyphen_values = true)]
    pub payload_json: Option<String>,
    #[arg(long)]
    pub step_id: Option<String>,
    #[arg(long, default_value = "human")]
    pub emitted_by: String,
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
