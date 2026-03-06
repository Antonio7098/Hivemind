use super::*;

#[derive(Subcommand)]
pub enum GraphCommands {
    Create(GraphCreateArgs),
    #[command(subcommand)]
    Query(GraphQueryCommands),
    #[command(subcommand)]
    Snapshot(GraphSnapshotCommands),
    AddDependency(GraphAddDependencyArgs),
    AddCheck(GraphAddCheckArgs),
    Validate(GraphValidateArgs),
    List(GraphListArgs),
    Delete(GraphDeleteArgs),
}

#[derive(Subcommand)]
pub enum GraphSnapshotCommands {
    Refresh(GraphSnapshotRefreshArgs),
}

#[derive(Subcommand)]
pub enum GraphQueryCommands {
    Neighbors(GraphQueryNeighborsArgs),
    Dependents(GraphQueryDependentsArgs),
    Subgraph(GraphQuerySubgraphArgs),
    Filter(GraphQueryFilterArgs),
}

#[derive(Args)]
pub struct GraphSnapshotRefreshArgs {
    pub project: String,
}

#[derive(Args)]
pub struct GraphQueryNeighborsArgs {
    pub project: String,
    #[arg(long)]
    pub node: String,
    #[arg(long = "edge-type")]
    pub edge_types: Vec<String>,
    #[arg(long, default_value_t = 200)]
    pub max_results: usize,
}

#[derive(Args)]
pub struct GraphQueryDependentsArgs {
    pub project: String,
    #[arg(long)]
    pub node: String,
    #[arg(long = "edge-type")]
    pub edge_types: Vec<String>,
    #[arg(long, default_value_t = 200)]
    pub max_results: usize,
}

#[derive(Args)]
pub struct GraphQuerySubgraphArgs {
    pub project: String,
    #[arg(long)]
    pub seed: String,
    #[arg(long)]
    pub depth: usize,
    #[arg(long = "edge-type")]
    pub edge_types: Vec<String>,
    #[arg(long, default_value_t = 200)]
    pub max_results: usize,
}

#[derive(Args)]
pub struct GraphQueryFilterArgs {
    pub project: String,
    #[arg(long = "type")]
    pub node_type: Option<String>,
    #[arg(long)]
    pub path: Option<String>,
    #[arg(long)]
    pub partition: Option<String>,
    #[arg(long, default_value_t = 200)]
    pub max_results: usize,
}

#[derive(Args)]
pub struct GraphCreateArgs {
    pub project: String,
    pub name: String,
    #[arg(long, num_args = 1..)]
    pub from_tasks: Vec<String>,
}

#[derive(Args)]
pub struct GraphAddDependencyArgs {
    pub graph_id: String,
    pub from_task: String,
    pub to_task: String,
}

#[derive(Args)]
pub struct GraphAddCheckArgs {
    pub graph_id: String,
    pub task_id: String,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub command: String,
    #[arg(long, default_value_t = true)]
    pub required: bool,
    #[arg(long)]
    pub timeout_ms: Option<u64>,
}

#[derive(Args)]
pub struct GraphValidateArgs {
    pub graph_id: String,
}

#[derive(Args)]
pub struct GraphListArgs {
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct GraphDeleteArgs {
    pub graph_id: String,
}

#[derive(Subcommand)]
pub enum FlowCommands {
    Create(FlowCreateArgs),
    List(FlowListArgs),
    Start(FlowStartArgs),
    Tick(FlowTickArgs),
    Pause(FlowPauseArgs),
    Resume(FlowResumeArgs),
    Abort(FlowAbortArgs),
    Restart(FlowRestartArgs),
    Status(FlowStatusArgs),
    SetRunMode(FlowSetRunModeArgs),
    AddDependency(FlowAddDependencyArgs),
    RuntimeSet(FlowRuntimeSetArgs),
    Delete(FlowDeleteArgs),
}

#[derive(Args)]
pub struct FlowCreateArgs {
    pub graph_id: String,
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Args)]
pub struct FlowListArgs {
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct FlowStartArgs {
    pub flow_id: String,
}

#[derive(Args)]
pub struct FlowTickArgs {
    pub flow_id: String,
    #[arg(long)]
    pub interactive: bool,
    #[arg(long)]
    pub max_parallel: Option<u16>,
}

#[derive(Args)]
pub struct FlowPauseArgs {
    pub flow_id: String,
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args)]
pub struct FlowResumeArgs {
    pub flow_id: String,
}

#[derive(Args)]
pub struct FlowAbortArgs {
    pub flow_id: String,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Args)]
pub struct FlowRestartArgs {
    pub flow_id: String,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long, default_value_t = false)]
    pub start: bool,
}

#[derive(Args)]
pub struct FlowSetRunModeArgs {
    pub flow_id: String,
    #[arg(value_enum)]
    pub mode: RunModeArg,
}

#[derive(Args)]
pub struct FlowAddDependencyArgs {
    pub flow_id: String,
    pub depends_on_flow_id: String,
}

#[derive(Args)]
pub struct FlowRuntimeSetArgs {
    pub flow_id: String,
    #[arg(long)]
    pub clear: bool,
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
pub struct FlowDeleteArgs {
    pub flow_id: String,
}
