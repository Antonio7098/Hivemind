use super::*;
use clap::Parser;

#[test]
fn parses_graph_query_neighbors_command() {
    let cli = Cli::try_parse_from([
        "hivemind",
        "graph",
        "query",
        "neighbors",
        "proj-a",
        "--node",
        "repo::symbol",
        "--edge-type",
        "calls",
        "--max-results",
        "10",
    ])
    .expect("graph query neighbors parse");

    match cli.command.expect("command") {
        Commands::Graph(GraphCommands::Query(GraphQueryCommands::Neighbors(args))) => {
            assert_eq!(args.project, "proj-a");
            assert_eq!(args.node, "repo::symbol");
            assert_eq!(args.edge_types, vec!["calls"]);
            assert_eq!(args.max_results, 10);
        }
        _ => panic!("unexpected parsed command"),
    }
}

#[test]
fn parses_flow_runtime_set_command() {
    let cli = Cli::try_parse_from([
        "hivemind",
        "flow",
        "runtime-set",
        "flow-123",
        "--role",
        "validator",
        "--adapter",
        "mock-runtime",
        "--binary-path",
        "/usr/bin/mock-runtime",
        "--model",
        "model-x",
        "--arg=--safe",
        "--env",
        "KEY=VALUE",
        "--timeout-ms",
        "123",
        "--max-parallel-tasks",
        "2",
    ])
    .expect("flow runtime-set parse");

    match cli.command.expect("command") {
        Commands::Flow(FlowCommands::RuntimeSet(args)) => {
            assert_eq!(args.flow_id, "flow-123");
            assert_eq!(args.role, RuntimeRoleArg::Validator);
            assert_eq!(args.adapter, "mock-runtime");
            assert_eq!(args.binary_path, "/usr/bin/mock-runtime");
            assert_eq!(args.model.as_deref(), Some("model-x"));
            assert_eq!(args.args, vec!["--safe"]);
            assert_eq!(args.env, vec!["KEY=VALUE"]);
            assert_eq!(args.timeout_ms, 123);
            assert_eq!(args.max_parallel_tasks, 2);
        }
        _ => panic!("unexpected parsed command"),
    }
}

#[test]
fn parses_constitution_init_command() {
    let cli = Cli::try_parse_from([
        "hivemind",
        "constitution",
        "init",
        "proj-a",
        "--content",
        "rules: []",
        "--confirm",
        "--actor",
        "antonio",
    ])
    .expect("constitution init parse");

    match cli.command.expect("command") {
        Commands::Constitution(ConstitutionCommands::Init(args)) => {
            assert_eq!(args.project, "proj-a");
            assert_eq!(args.content.as_deref(), Some("rules: []"));
            assert!(args.confirm);
            assert_eq!(args.actor.as_deref(), Some("antonio"));
        }
        _ => panic!("unexpected parsed command"),
    }
}

#[test]
fn parses_project_governance_repair_apply_command() {
    let cli = Cli::try_parse_from([
        "hivemind",
        "project",
        "governance",
        "repair",
        "apply",
        "proj-a",
        "--snapshot-id",
        "snap-1",
        "--confirm",
    ])
    .expect("project governance repair apply parse");

    match cli.command.expect("command") {
        Commands::Project(ProjectCommands::Governance(ProjectGovernanceCommands::Repair(
            ProjectGovernanceRepairCommands::Apply(args),
        ))) => {
            assert_eq!(args.project, "proj-a");
            assert_eq!(args.snapshot_id.as_deref(), Some("snap-1"));
            assert!(args.confirm);
        }
        _ => panic!("unexpected parsed command"),
    }
}

#[test]
fn parses_task_retry_command() {
    let cli = Cli::try_parse_from([
        "hivemind",
        "task",
        "retry",
        "task-1",
        "--reset-count",
        "--mode",
        "continue",
    ])
    .expect("task retry parse");

    match cli.command.expect("command") {
        Commands::Task(TaskCommands::Retry(args)) => {
            assert_eq!(args.task_id, "task-1");
            assert!(args.reset_count);
            assert_eq!(args.mode, TaskRetryMode::Continue);
        }
        _ => panic!("unexpected parsed command"),
    }
}

#[test]
fn parses_event_stream_command() {
    let cli = Cli::try_parse_from([
        "hivemind",
        "events",
        "stream",
        "--flow",
        "flow-1",
        "--artifact-id",
        "artifact-1",
        "--limit",
        "25",
        "--redact-secrets",
    ])
    .expect("event stream parse");

    match cli.command.expect("command") {
        Commands::Events(EventCommands::Stream(args)) => {
            assert_eq!(args.flow.as_deref(), Some("flow-1"));
            assert_eq!(args.artifact_id.as_deref(), Some("artifact-1"));
            assert_eq!(args.limit, 25);
            assert!(args.redact_secrets);
        }
        _ => panic!("unexpected parsed command"),
    }
}

#[test]
fn parses_event_native_summary_command() {
    let cli = Cli::try_parse_from([
        "hivemind",
        "events",
        "native-summary",
        "--flow",
        "flow-1",
        "--limit",
        "25",
        "--verify",
    ])
    .expect("event native-summary parse");

    match cli.command.expect("command") {
        Commands::Events(EventCommands::NativeSummary(args)) => {
            assert_eq!(args.flow.as_deref(), Some("flow-1"));
            assert_eq!(args.limit, 25);
            assert!(args.verify);
        }
        _ => panic!("unexpected parsed command"),
    }
}
