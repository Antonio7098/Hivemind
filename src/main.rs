//! Hivemind CLI entrypoint.

use chrono::{DateTime, Utc};
use clap::error::ErrorKind;
use clap::Parser;
use hivemind::cli::commands::{
    AttemptCommands, AttemptInspectArgs, CheckpointCommands, Cli, Commands, ConstitutionCommands,
    EventCommands, FlowCommands, GlobalCommands, GlobalNotepadCommands, GlobalSkillCommands,
    GlobalSkillRegistryCommands, GlobalSystemPromptCommands, GlobalTemplateCommands, GraphCommands,
    GraphSnapshotCommands, MergeCommands, MergeExecuteModeArg, ProjectCommands,
    ProjectGovernanceAttachmentCommands, ProjectGovernanceCommands,
    ProjectGovernanceDocumentCommands, ProjectGovernanceNotepadCommands,
    ProjectGovernanceRepairCommands, ProjectGovernanceSnapshotCommands, RunModeArg,
    RuntimeCommands, RuntimeRoleArg, ServeArgs, TaskAbortArgs, TaskCloseArgs, TaskCommands,
    TaskCompleteArgs, TaskCreateArgs, TaskInspectArgs, TaskListArgs, TaskRetryArgs, TaskStartArgs,
    TaskUpdateArgs, VerifyCommands, WorktreeCommands,
};
use hivemind::cli::output::{output, output_error, OutputFormat};
use hivemind::core::error::ExitCode;
use hivemind::core::events::RuntimeRole;
use hivemind::core::flow::{RetryMode, RunMode};
use hivemind::core::registry::{
    MergeExecuteMode, MergeExecuteOptions, ProjectGovernanceInitResult,
    ProjectGovernanceInspectResult, ProjectGovernanceMigrateResult, Registry,
};
use hivemind::core::scope::RepoAccessMode;
use hivemind::core::scope::Scope;
use hivemind::core::skill_registry;
use hivemind::core::state::{AttemptState, Project, Task, TaskState};
use std::ffi::OsString;
use std::fs;
use std::process;
use uuid::Uuid;

fn parse_format_from_args(args: &[OsString]) -> OutputFormat {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        let s = arg.to_string_lossy();

        if s == "-f" || s == "--format" {
            if let Some(value) = iter.next() {
                return parse_format_value(&value.to_string_lossy());
            }
        }

        if let Some(value) = s.strip_prefix("--format=") {
            return parse_format_value(value);
        }
    }

    OutputFormat::Table
}

fn parse_format_value(value: &str) -> OutputFormat {
    let v = value.to_lowercase();
    if v == "json" {
        OutputFormat::Json
    } else if v == "yaml" || v == "yml" {
        OutputFormat::Yaml
    } else {
        OutputFormat::Table
    }
}

fn print_structured<T: serde::Serialize>(value: &T, format: OutputFormat, context: &str) {
    match format {
        OutputFormat::Table => {
            if let Ok(json) = serde_json::to_string_pretty(value) {
                println!("{json}");
            }
        }
        _ => {
            if let Err(err) = output(value, format) {
                eprintln!("Failed to render {context}: {err}");
            }
        }
    }
}

fn output_help(help: &str, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            print!("{help}");
        }
        OutputFormat::Json => {
            let response = hivemind::cli::output::CliResponse::success(serde_json::json!({
                "help": help
            }));
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let response = hivemind::cli::output::CliResponse::success(serde_json::json!({
                "help": help
            }));
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                print!("{yaml}");
            }
        }
    }
}

fn output_version(format: OutputFormat) {
    let version = env!("CARGO_PKG_VERSION");
    match format {
        OutputFormat::Table => {
            println!("hivemind {version}");
        }
        OutputFormat::Json => {
            let response = hivemind::cli::output::CliResponse::success(serde_json::json!({
                "name": "hivemind",
                "version": version
            }));
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let response = hivemind::cli::output::CliResponse::success(serde_json::json!({
                "name": "hivemind",
                "version": version
            }));
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                print!("{yaml}");
            }
        }
    }
}

fn handle_clap_error(err: &clap::Error, format: OutputFormat) -> ExitCode {
    match err.kind() {
        ErrorKind::DisplayHelp => {
            let rendered = err.render().to_string();
            output_help(&rendered, format);
            ExitCode::Success
        }
        ErrorKind::DisplayVersion => {
            output_version(format);
            ExitCode::Success
        }
        _ => {
            eprintln!("{}", err.render());
            ExitCode::Error
        }
    }
}

fn main() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let is_broken_pipe = info
            .payload()
            .downcast_ref::<&str>()
            .is_some_and(|s| s.contains("Broken pipe"))
            || info
                .payload()
                .downcast_ref::<String>()
                .is_some_and(|s| s.contains("Broken pipe"));

        if is_broken_pipe {
            return;
        }

        default_hook(info);
    }));

    let args: Vec<OsString> = std::env::args_os().collect();
    let format = parse_format_from_args(&args);

    let result = std::panic::catch_unwind(|| Cli::try_parse_from(&args).map(run));

    match result {
        Ok(Ok(exit_code)) => process::exit(i32::from(exit_code)),
        Ok(Err(e)) => process::exit(i32::from(handle_clap_error(&e, format))),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("panic");

            if msg.contains("Broken pipe") {
                process::exit(0);
            }

            std::panic::resume_unwind(payload);
        }
    }
}

#[allow(clippy::too_many_lines)]
fn handle_worktree(cmd: WorktreeCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        WorktreeCommands::List(args) => match registry.worktree_list(&args.flow_id) {
            Ok(statuses) => match format {
                OutputFormat::Json => {
                    let response = hivemind::cli::output::CliResponse::success(&statuses);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Yaml => {
                    let response = hivemind::cli::output::CliResponse::success(&statuses);
                    if let Ok(yaml) = serde_yaml::to_string(&response) {
                        print!("{yaml}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    if let Ok(json) = serde_json::to_string_pretty(&statuses) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
            },
            Err(e) => output_error(&e, format),
        },
        WorktreeCommands::Inspect(args) => match registry.worktree_inspect(&args.task_id) {
            Ok(status) => match format {
                OutputFormat::Json => {
                    let response = hivemind::cli::output::CliResponse::success(&status);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Yaml => {
                    let response = hivemind::cli::output::CliResponse::success(&status);
                    if let Ok(yaml) = serde_yaml::to_string(&response) {
                        print!("{yaml}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    if let Ok(json) = serde_json::to_string_pretty(&status) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
            },
            Err(e) => output_error(&e, format),
        },
        WorktreeCommands::Cleanup(args) => {
            match registry.worktree_cleanup(&args.flow_id, args.force, args.dry_run) {
                Ok(result) => {
                    match format {
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "success": true,
                                    "flow_id": result.flow_id,
                                    "force": result.forced,
                                    "dry_run": result.dry_run,
                                    "cleaned_worktrees": result.cleaned_worktrees
                                })
                            );
                        }
                        OutputFormat::Yaml => {
                            if let Ok(yaml) = serde_yaml::to_string(&serde_json::json!({
                                "success": true,
                                "flow_id": result.flow_id,
                                "force": result.forced,
                                "dry_run": result.dry_run,
                                "cleaned_worktrees": result.cleaned_worktrees
                            })) {
                                print!("{yaml}");
                            }
                        }
                        OutputFormat::Table => {
                            if result.dry_run {
                                println!(
                                    "Dry run complete. {} worktree(s) would be cleaned.",
                                    result.cleaned_worktrees
                                );
                            } else {
                                println!(
                                    "Cleanup complete. {} worktree(s) cleaned.",
                                    result.cleaned_worktrees
                                );
                            }
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

fn print_graph_id(graph_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"graph_id": graph_id}));
        }
        OutputFormat::Table => {
            println!("Graph ID: {graph_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"graph_id": graph_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}

fn print_flow_id(flow_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"flow_id": flow_id}));
        }
        OutputFormat::Table => {
            println!("Flow ID: {flow_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"flow_id": flow_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}

fn print_graphs(graphs: &[hivemind::core::graph::TaskGraph], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if graphs.is_empty() {
                println!("No graphs found.");
                return;
            }
            println!("{:<36}  {:<36}  {:<10}  NAME", "ID", "PROJECT", "STATE");
            println!("{}", "-".repeat(110));
            for g in graphs {
                println!(
                    "{:<36}  {:<36}  {:<10}  {}",
                    g.id,
                    g.project_id,
                    format!("{:?}", g.state).to_lowercase(),
                    g.name
                );
            }
        }
        _ => {
            if let Err(err) = output(graphs, format) {
                eprintln!("Failed to render graphs: {err}");
            }
        }
    }
}

fn print_flows(flows: &[hivemind::core::flow::TaskFlow], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if flows.is_empty() {
                println!("No flows found.");
                return;
            }
            println!(
                "{:<36}  {:<36}  {:<10}  {:<6}  GRAPH",
                "ID", "PROJECT", "STATE", "MODE"
            );
            println!("{}", "-".repeat(110));
            for f in flows {
                println!(
                    "{:<36}  {:<36}  {:<10}  {:<6}  {}",
                    f.id,
                    f.project_id,
                    format!("{:?}", f.state).to_lowercase(),
                    format!("{:?}", f.run_mode).to_lowercase(),
                    f.graph_id
                );
            }
        }
        _ => {
            if let Err(err) = output(flows, format) {
                eprintln!("Failed to render flows: {err}");
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn handle_graph(cmd: GraphCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        GraphCommands::Create(args) => {
            let mut task_ids = Vec::new();
            for raw in &args.from_tasks {
                let Ok(id) = Uuid::parse_str(raw) else {
                    return output_error(
                        &hivemind::core::error::HivemindError::user(
                            "invalid_task_id",
                            format!("'{raw}' is not a valid task ID"),
                            "cli:graph:create",
                        ),
                        format,
                    );
                };
                task_ids.push(id);
            }

            match registry.create_graph(&args.project, &args.name, &task_ids) {
                Ok(graph) => {
                    print_graph_id(graph.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GraphCommands::Snapshot(cmd) => match cmd {
            GraphSnapshotCommands::Refresh(args) => {
                match registry.graph_snapshot_refresh(&args.project, "manual_refresh") {
                    Ok(result) => {
                        if format == OutputFormat::Table {
                            println!("Project:              {}", result.project_id);
                            println!("Snapshot path:        {}", result.path);
                            println!("Trigger:              {}", result.trigger);
                            println!("Repository count:     {}", result.repository_count);
                            println!("UCP profile:          {}", result.profile_version);
                            println!("UCP engine:           {}", result.ucp_engine_version);
                            println!("Fingerprint:          {}", result.canonical_fingerprint);
                            println!("Artifact revision:    {}", result.revision);
                        } else {
                            print_structured(&result, format, "graph snapshot refresh result");
                        }
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        GraphCommands::AddDependency(args) => {
            match registry.add_graph_dependency(&args.graph_id, &args.from_task, &args.to_task) {
                Ok(graph) => {
                    print_graph_id(graph.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GraphCommands::AddCheck(args) => {
            let mut check = hivemind::core::verification::CheckConfig::new(
                args.name.clone(),
                args.command.clone(),
            );
            check.required = args.required;
            check.timeout_ms = args.timeout_ms;

            match registry.add_graph_task_check(&args.graph_id, &args.task_id, check) {
                Ok(graph) => {
                    print_graph_id(graph.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GraphCommands::Validate(args) => match registry.validate_graph(&args.graph_id) {
            Ok(result) => match format {
                OutputFormat::Json => {
                    if let Ok(json) = serde_json::to_string_pretty(&result) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Yaml => {
                    if let Ok(yaml) = serde_yaml::to_string(&result) {
                        print!("{yaml}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    if result.valid {
                        println!("valid");
                    } else {
                        println!("invalid");
                        for issue in result.issues {
                            println!("- {issue}");
                        }
                    }
                    ExitCode::Success
                }
            },
            Err(e) => output_error(&e, format),
        },
        GraphCommands::List(args) => match registry.list_graphs(args.project.as_deref()) {
            Ok(graphs) => {
                print_graphs(&graphs, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GraphCommands::Delete(args) => match registry.delete_graph(&args.graph_id) {
            Ok(graph_id) => {
                print_graph_id(graph_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}

#[allow(clippy::too_many_lines)]
fn handle_flow(cmd: FlowCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        FlowCommands::Create(args) => {
            match registry.create_flow(&args.graph_id, args.name.as_deref()) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::List(args) => match registry.list_flows(args.project.as_deref()) {
            Ok(flows) => {
                print_flows(&flows, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Start(args) => match registry.start_flow(&args.flow_id) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Tick(args) => {
            match registry.tick_flow(&args.flow_id, args.interactive, args.max_parallel) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Pause(args) => {
            let _ = args.wait;
            match registry.pause_flow(&args.flow_id) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Resume(args) => match registry.resume_flow(&args.flow_id) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Abort(args) => {
            match registry.abort_flow(&args.flow_id, args.reason.as_deref(), args.force) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Restart(args) => {
            match registry.restart_flow(&args.flow_id, args.name.as_deref(), args.start) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Status(args) => match registry.get_flow(&args.flow_id) {
            Ok(flow) => match format {
                OutputFormat::Json | OutputFormat::Yaml => {
                    if let Err(err) = output(&flow, format) {
                        eprintln!("Failed to render flow status: {err}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    println!("ID:      {}", flow.id);
                    println!("Graph:   {}", flow.graph_id);
                    println!("Project: {}", flow.project_id);
                    println!("State:   {:?}", flow.state);
                    println!("RunMode: {:?}", flow.run_mode);
                    if !flow.depends_on_flows.is_empty() {
                        let mut deps: Vec<_> = flow.depends_on_flows.iter().copied().collect();
                        deps.sort();
                        println!(
                            "FlowDeps: {}",
                            deps.iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                    let mut counts = std::collections::HashMap::new();
                    for exec in flow.task_executions.values() {
                        *counts.entry(exec.state).or_insert(0usize) += 1;
                    }
                    println!("Tasks:");
                    let mut keys: Vec<_> = counts.keys().copied().collect();
                    keys.sort_by_key(|k| format!("{k:?}"));
                    for k in keys {
                        println!("  - {:?}: {}", k, counts[&k]);
                    }
                    ExitCode::Success
                }
            },
            Err(e) => output_error(&e, format),
        },
        FlowCommands::SetRunMode(args) => {
            let mode = parse_run_mode(args.mode);
            match registry.flow_set_run_mode(&args.flow_id, mode) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::AddDependency(args) => {
            match registry.flow_add_dependency(&args.flow_id, &args.depends_on_flow_id) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::RuntimeSet(args) => {
            let role = parse_runtime_role(args.role);
            let result = if args.clear {
                registry.flow_runtime_clear(&args.flow_id, role)
            } else {
                registry.flow_runtime_set(
                    &args.flow_id,
                    role,
                    &args.adapter,
                    &args.binary_path,
                    args.model,
                    &args.args,
                    &args.env,
                    args.timeout_ms,
                    args.max_parallel_tasks,
                )
            };
            match result {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Delete(args) => match registry.delete_flow(&args.flow_id) {
            Ok(flow_id) => {
                print_flow_id(flow_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}

fn run(cli: Cli) -> ExitCode {
    let format = cli.format;

    match cli.command {
        Some(Commands::Version) => {
            output_version(format);
            ExitCode::Success
        }
        Some(Commands::Serve(args)) => handle_serve(args, format),
        Some(Commands::Project(cmd)) => handle_project(cmd, format),
        Some(Commands::Global(cmd)) => handle_global(cmd, format),
        Some(Commands::Constitution(cmd)) => handle_constitution(cmd, format),
        Some(Commands::Task(cmd)) => handle_task(cmd, format),
        Some(Commands::Graph(cmd)) => handle_graph(cmd, format),
        Some(Commands::Flow(cmd)) => handle_flow(cmd, format),
        Some(Commands::Events(cmd)) => handle_events(cmd, format),
        Some(Commands::Runtime(cmd)) => handle_runtime(cmd, format),
        Some(Commands::Verify(cmd)) => handle_verify(cmd, format),
        Some(Commands::Merge(cmd)) => handle_merge(cmd, format),
        Some(Commands::Attempt(cmd)) => handle_attempt(cmd, format),
        Some(Commands::Checkpoint(cmd)) => handle_checkpoint(cmd, format),
        Some(Commands::Worktree(cmd)) => handle_worktree(cmd, format),
        None => {
            println!("hivemind {}", env!("CARGO_PKG_VERSION"));
            println!("Use --help for usage information.");
            ExitCode::Success
        }
    }
}

fn handle_serve(args: ServeArgs, format: OutputFormat) -> ExitCode {
    if format != OutputFormat::Table {
        eprintln!("Warning: 'serve' always logs to stderr; output format is ignored.");
    }

    let config = hivemind::server::ServeConfig {
        host: args.host,
        port: args.port,
        events_limit: args.events_limit,
    };

    match hivemind::server::serve(&config) {
        Ok(()) => ExitCode::Success,
        Err(e) => output_error(&e, format),
    }
}

fn get_registry(format: OutputFormat) -> Option<Registry> {
    match Registry::open() {
        Ok(r) => Some(r),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

fn print_project(project: &Project, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("ID:          {}", project.id);
            println!("Name:        {}", project.name);
            if let Some(desc) = &project.description {
                println!("Description: {desc}");
            }
            println!("Created:     {}", project.created_at);
            println!("Updated:     {}", project.updated_at);
            if !project.repositories.is_empty() {
                println!("Repositories:");
                for repo in &project.repositories {
                    let access = match repo.access_mode {
                        RepoAccessMode::ReadOnly => "readonly",
                        RepoAccessMode::ReadWrite => "readwrite",
                    };
                    println!("  - {} ({}) [{access}]", repo.name, repo.path);
                }
            }
        }
        _ => {
            if let Err(err) = output(project, format) {
                eprintln!("Failed to render project: {err}");
            }
        }
    }
}

fn print_project_id(project_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"project_id": project_id}));
        }
        OutputFormat::Table => {
            println!("Project ID: {project_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"project_id": project_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}

fn print_projects(projects: &[Project], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if projects.is_empty() {
                println!("No projects found.");
                return;
            }
            println!("{:<36}  {:<20}  DESCRIPTION", "ID", "NAME");
            println!("{}", "-".repeat(80));
            for p in projects {
                let desc = p.description.as_deref().unwrap_or("");
                println!("{:<36}  {:<20}  {}", p.id, p.name, desc);
            }
        }
        _ => {
            if let Err(err) = output(projects, format) {
                eprintln!("Failed to render projects: {err}");
            }
        }
    }
}

fn print_project_governance_init(result: &ProjectGovernanceInitResult, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("Project ID:        {}", result.project_id);
            println!("Governance Root:   {}", result.root_path);
            println!("Schema Version:    {}", result.schema_version);
            println!("Projection Version:{}", result.projection_version);
            if result.created_paths.is_empty() {
                println!("Created Paths:     none (already initialized)");
            } else {
                println!("Created Paths:");
                for path in &result.created_paths {
                    println!("  - {path}");
                }
            }
        }
        _ => {
            if let Err(err) = output(result, format) {
                eprintln!("Failed to render governance init result: {err}");
            }
        }
    }
}

fn print_project_governance_migrate(result: &ProjectGovernanceMigrateResult, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("Project ID:        {}", result.project_id);
            println!("From Layout:       {}", result.from_layout);
            println!("To Layout:         {}", result.to_layout);
            println!("Schema Version:    {}", result.schema_version);
            println!("Projection Version:{}", result.projection_version);
            if result.migrated_paths.is_empty() {
                println!("Migrated Paths:    none (no legacy artifacts found)");
            } else {
                println!("Migrated Paths:");
                for path in &result.migrated_paths {
                    println!("  - {path}");
                }
            }
            println!("Rollback Hint:     {}", result.rollback_hint);
        }
        _ => {
            if let Err(err) = output(result, format) {
                eprintln!("Failed to render governance migration result: {err}");
            }
        }
    }
}

fn print_project_governance_inspect(result: &ProjectGovernanceInspectResult, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("Project ID:        {}", result.project_id);
            println!("Governance Root:   {}", result.root_path);
            println!("Initialized:       {}", result.initialized);
            println!("Schema Version:    {}", result.schema_version);
            println!("Projection Version:{}", result.projection_version);
            println!("Worktree Base Dir: {}", result.worktree_base_dir);
            println!("Boundary:          {}", result.export_import_boundary);

            println!("Artifacts:");
            for artifact in &result.artifacts {
                println!(
                    "  - [{}] {}:{} -> {} (exists={}, projected={}, revision={})",
                    artifact.scope,
                    artifact.artifact_kind,
                    artifact.artifact_key,
                    artifact.path,
                    artifact.exists,
                    artifact.projected,
                    artifact.revision
                );
            }

            if result.migrations.is_empty() {
                println!("Migrations:        none");
            } else {
                println!("Migrations:");
                for migration in &result.migrations {
                    println!(
                        "  - {} -> {} @ {}",
                        migration.from_layout, migration.to_layout, migration.migrated_at
                    );
                    if !migration.migrated_paths.is_empty() {
                        println!("    paths: {}", migration.migrated_paths.join(", "));
                    }
                }
            }

            if result.legacy_candidates.is_empty() {
                println!("Legacy Candidates: none");
            } else {
                println!("Legacy Candidates:");
                for path in &result.legacy_candidates {
                    println!("  - {path}");
                }
            }
        }
        _ => {
            if let Err(err) = output(result, format) {
                eprintln!("Failed to render governance inspect result: {err}");
            }
        }
    }
}

fn resolve_required_selector(
    positional: Option<&str>,
    flag_value: Option<&str>,
    flag_name: &str,
    noun: &str,
    origin: &str,
) -> Result<String, hivemind::core::error::HivemindError> {
    let pos = positional.map(str::trim).filter(|s| !s.is_empty());
    let flag = flag_value.map(str::trim).filter(|s| !s.is_empty());
    match (pos, flag) {
        (Some(a), Some(b)) if a != b => Err(hivemind::core::error::HivemindError::user(
            "selector_conflict",
            format!("Conflicting {noun} values provided via positional argument and {flag_name}"),
            origin,
        )
        .with_context("positional", a)
        .with_context("flag", b)),
        (Some(a), _) => Ok(a.to_string()),
        (None, Some(b)) => Ok(b.to_string()),
        (None, None) => Err(hivemind::core::error::HivemindError::user(
            "missing_required_selector",
            format!("Provide {noun} as a positional argument or via {flag_name}"),
            origin,
        )),
    }
}

#[allow(clippy::too_many_lines)]
fn handle_project(cmd: ProjectCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        ProjectCommands::Create(args) => {
            match registry.create_project(&args.name, args.description.as_deref()) {
                Ok(project) => {
                    print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::List => match registry.list_projects() {
            Ok(projects) => {
                print_projects(&projects, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Inspect(args) => match registry.get_project(&args.project) {
            Ok(project) => {
                print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Update(args) => {
            match registry.update_project(
                &args.project,
                args.name.as_deref(),
                args.description.as_deref(),
            ) {
                Ok(project) => {
                    print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::RuntimeSet(args) => match registry.project_runtime_set_role(
            &args.project,
            parse_runtime_role(args.role),
            &args.adapter,
            &args.binary_path,
            args.model,
            &args.args,
            &args.env,
            args.timeout_ms,
            args.max_parallel_tasks,
        ) {
            Ok(project) => {
                print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::AttachRepo(args) => {
            let access_mode = match args.access.to_lowercase().as_str() {
                "ro" => RepoAccessMode::ReadOnly,
                "rw" => RepoAccessMode::ReadWrite,
                other => {
                    return output_error(
                        &hivemind::core::error::HivemindError::user(
                            "invalid_access_mode",
                            format!("Invalid access mode: '{other}'"),
                            "cli:project:attach-repo",
                        )
                        .with_hint("Use --access ro or --access rw"),
                        format,
                    );
                }
            };

            match registry.attach_repo(&args.project, &args.path, args.name.as_deref(), access_mode)
            {
                Ok(project) => {
                    print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::DetachRepo(args) => {
            match registry.detach_repo(&args.project, &args.repo_name) {
                Ok(project) => {
                    print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::Delete(args) => match registry.delete_project(&args.project) {
            Ok(project_id) => {
                print_project_id(project_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Governance(cmd) => match cmd {
            ProjectGovernanceCommands::Init(args) => {
                let project = match resolve_required_selector(
                    args.project.as_deref(),
                    args.project_flag.as_deref(),
                    "--project",
                    "project",
                    "cli:project:governance:init",
                ) {
                    Ok(project) => project,
                    Err(e) => return output_error(&e, format),
                };
                match registry.project_governance_init(&project) {
                    Ok(result) => {
                        print_project_governance_init(&result, format);
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Migrate(args) => {
                match registry.project_governance_migrate(&args.project) {
                    Ok(result) => {
                        print_project_governance_migrate(&result, format);
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Inspect(args) => {
                match registry.project_governance_inspect(&args.project) {
                    Ok(result) => {
                        print_project_governance_inspect(&result, format);
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Diagnose(args) => {
                match registry.project_governance_diagnose(&args.project) {
                    Ok(result) => {
                        print_structured(&result, format, "project governance diagnostics");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Replay(args) => {
                match registry.project_governance_replay(&args.project, args.verify) {
                    Ok(result) => {
                        print_structured(&result, format, "project governance replay result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Snapshot(cmd) => match cmd {
                ProjectGovernanceSnapshotCommands::Create(args) => {
                    match registry
                        .project_governance_snapshot_create(&args.project, args.interval_minutes)
                    {
                        Ok(result) => {
                            print_structured(&result, format, "governance snapshot create result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceSnapshotCommands::List(args) => {
                    match registry.project_governance_snapshot_list(&args.project, args.limit) {
                        Ok(result) => {
                            print_structured(&result, format, "governance snapshot list result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceSnapshotCommands::Restore(args) => match registry
                    .project_governance_snapshot_restore(
                        &args.project,
                        &args.snapshot_id,
                        args.confirm,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance snapshot restore result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
            },
            ProjectGovernanceCommands::Repair(cmd) => match cmd {
                ProjectGovernanceRepairCommands::Detect(args) => {
                    match registry.project_governance_repair_detect(&args.project) {
                        Ok(result) => {
                            print_structured(&result, format, "governance repair detect result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceRepairCommands::Preview(args) => match registry
                    .project_governance_repair_preview(&args.project, args.snapshot_id.as_deref())
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance repair preview result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceRepairCommands::Apply(args) => match registry
                    .project_governance_repair_apply(
                        &args.project,
                        args.snapshot_id.as_deref(),
                        args.confirm,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance repair apply result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
            },
            ProjectGovernanceCommands::Document(cmd) => match cmd {
                ProjectGovernanceDocumentCommands::Create(args) => match registry
                    .project_governance_document_create(
                        &args.project,
                        &args.document_id,
                        &args.title,
                        &args.owner,
                        &args.tags,
                        &args.content,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance document create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceDocumentCommands::List(args) => {
                    match registry.project_governance_document_list(&args.project) {
                        Ok(result) => {
                            print_structured(&result, format, "governance document list");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceDocumentCommands::Inspect(args) => match registry
                    .project_governance_document_inspect(&args.project, &args.document_id)
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance document inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceDocumentCommands::Update(args) => match registry
                    .project_governance_document_update(
                        &args.project,
                        &args.document_id,
                        args.title.as_deref(),
                        args.owner.as_deref(),
                        args.tags.as_deref(),
                        args.content.as_deref(),
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance document update result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceDocumentCommands::Delete(args) => match registry
                    .project_governance_document_delete(&args.project, &args.document_id)
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance document delete result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
            },
            ProjectGovernanceCommands::Attachment(cmd) => match cmd {
                ProjectGovernanceAttachmentCommands::Include(args) => match registry
                    .project_governance_attachment_set_document(
                        &args.project,
                        &args.task_id,
                        &args.document_id,
                        true,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance attachment include result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceAttachmentCommands::Exclude(args) => match registry
                    .project_governance_attachment_set_document(
                        &args.project,
                        &args.task_id,
                        &args.document_id,
                        false,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance attachment exclude result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
            },
            ProjectGovernanceCommands::Notepad(cmd) => match cmd {
                ProjectGovernanceNotepadCommands::Create(args) => {
                    match registry.project_governance_notepad_create(&args.project, &args.content) {
                        Ok(result) => {
                            print_structured(&result, format, "project notepad create result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceNotepadCommands::Show(args) => {
                    match registry.project_governance_notepad_show(&args.project) {
                        Ok(result) => {
                            print_structured(&result, format, "project notepad show result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceNotepadCommands::Update(args) => {
                    match registry.project_governance_notepad_update(&args.project, &args.content) {
                        Ok(result) => {
                            print_structured(&result, format, "project notepad update result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceNotepadCommands::Delete(args) => {
                    match registry.project_governance_notepad_delete(&args.project) {
                        Ok(result) => {
                            print_structured(&result, format, "project notepad delete result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
            },
        },
    }
}

fn read_constitution_payload(
    content: Option<&str>,
    from_file: Option<&str>,
    origin: &'static str,
) -> std::result::Result<Option<String>, hivemind::core::error::HivemindError> {
    if let Some(raw) = content {
        return Ok(Some(raw.to_string()));
    }
    let Some(path) = from_file else {
        return Ok(None);
    };
    let payload = fs::read_to_string(path).map_err(|e| {
        hivemind::core::error::HivemindError::user(
            "constitution_input_read_failed",
            format!("Failed to read constitution file '{path}': {e}"),
            origin,
        )
        .with_hint("Ensure --from-file points to a readable YAML file")
    })?;
    Ok(Some(payload))
}

fn handle_constitution(cmd: ConstitutionCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        ConstitutionCommands::Init(args) => {
            let payload = match read_constitution_payload(
                args.content.as_deref(),
                args.from_file.as_deref(),
                "cli:constitution:init",
            ) {
                Ok(value) => value,
                Err(err) => return output_error(&err, format),
            };
            match registry.constitution_init(
                &args.project,
                payload.as_deref(),
                args.confirm,
                args.actor.as_deref(),
                args.intent.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "constitution init result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ConstitutionCommands::Show(args) => match registry.constitution_show(&args.project) {
            Ok(result) => {
                print_structured(&result, format, "constitution show result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ConstitutionCommands::Validate(args) => {
            match registry.constitution_validate(&args.project, None) {
                Ok(result) => {
                    print_structured(&result, format, "constitution validate result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ConstitutionCommands::Check(args) => match registry.constitution_check(&args.project) {
            Ok(result) => {
                print_structured(&result, format, "constitution check result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ConstitutionCommands::Update(args) => {
            let payload = match read_constitution_payload(
                args.content.as_deref(),
                args.from_file.as_deref(),
                "cli:constitution:update",
            ) {
                Ok(Some(value)) => value,
                Ok(None) => {
                    return output_error(
                        &hivemind::core::error::HivemindError::user(
                            "constitution_content_missing",
                            "Constitution update requires --content or --from-file",
                            "cli:constitution:update",
                        )
                        .with_hint(
                            "Provide a YAML payload via --content '<yaml>' or --from-file <path>",
                        ),
                        format,
                    );
                }
                Err(err) => return output_error(&err, format),
            };

            match registry.constitution_update(
                &args.project,
                &payload,
                args.confirm,
                args.actor.as_deref(),
                args.intent.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "constitution update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn handle_global(cmd: GlobalCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        GlobalCommands::Skill(cmd) => match cmd {
            GlobalSkillCommands::Create(args) => {
                match registry.global_skill_create(
                    &args.skill_id,
                    &args.name,
                    &args.tags,
                    &args.content,
                ) {
                    Ok(result) => {
                        print_structured(&result, format, "global skill create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSkillCommands::List => match registry.global_skill_list() {
                Ok(result) => {
                    print_structured(&result, format, "global skill list");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalSkillCommands::Inspect(args) => {
                match registry.global_skill_inspect(&args.skill_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global skill inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSkillCommands::Update(args) => match registry.global_skill_update(
                &args.skill_id,
                args.name.as_deref(),
                args.tags.as_deref(),
                args.content.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "global skill update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalSkillCommands::Delete(args) => match registry.global_skill_delete(&args.skill_id)
            {
                Ok(result) => {
                    print_structured(&result, format, "global skill delete result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalSkillCommands::Registry(cmd) => handle_skill_registry(cmd, &registry, format),
        },
        GlobalCommands::SystemPrompt(cmd) => match cmd {
            GlobalSystemPromptCommands::Create(args) => {
                match registry.global_system_prompt_create(&args.prompt_id, &args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "global system prompt create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSystemPromptCommands::List => match registry.global_system_prompt_list() {
                Ok(result) => {
                    print_structured(&result, format, "global system prompt list");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalSystemPromptCommands::Inspect(args) => {
                match registry.global_system_prompt_inspect(&args.prompt_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global system prompt inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSystemPromptCommands::Update(args) => {
                match registry.global_system_prompt_update(&args.prompt_id, &args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "global system prompt update result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSystemPromptCommands::Delete(args) => {
                match registry.global_system_prompt_delete(&args.prompt_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global system prompt delete result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        GlobalCommands::Template(cmd) => match cmd {
            GlobalTemplateCommands::Create(args) => match registry.global_template_create(
                &args.template_id,
                &args.system_prompt_id,
                &args.skill_ids,
                &args.document_ids,
                args.description.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "global template create result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalTemplateCommands::List => match registry.global_template_list() {
                Ok(result) => {
                    print_structured(&result, format, "global template list");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalTemplateCommands::Inspect(args) => {
                match registry.global_template_inspect(&args.template_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global template inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalTemplateCommands::Update(args) => match registry.global_template_update(
                &args.template_id,
                args.system_prompt_id.as_deref(),
                args.skill_ids.as_deref(),
                args.document_ids.as_deref(),
                args.description.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "global template update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalTemplateCommands::Delete(args) => {
                match registry.global_template_delete(&args.template_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global template delete result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalTemplateCommands::Instantiate(args) => {
                match registry.global_template_instantiate(&args.project, &args.template_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global template instantiate result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        GlobalCommands::Notepad(cmd) => match cmd {
            GlobalNotepadCommands::Create(args) => {
                match registry.global_notepad_create(&args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "global notepad create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalNotepadCommands::Show => match registry.global_notepad_show() {
                Ok(result) => {
                    print_structured(&result, format, "global notepad show result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalNotepadCommands::Update(args) => {
                match registry.global_notepad_update(&args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "global notepad update result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalNotepadCommands::Delete => match registry.global_notepad_delete() {
                Ok(result) => {
                    print_structured(&result, format, "global notepad delete result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
        },
    }
}

#[allow(clippy::too_many_lines)]
fn handle_skill_registry(
    cmd: GlobalSkillRegistryCommands,
    registry: &Registry,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        GlobalSkillRegistryCommands::RegistryList => {
            let registries = skill_registry::list_registries();
            match format {
                OutputFormat::Table => {
                    println!("{:<15}  {:<12}  DESCRIPTION", "NAME", "TYPE");
                    println!("{}", "-".repeat(80));
                    for r in &registries {
                        println!("{:<15}  {:<12}  {}", r.name, r.registry_type, r.description);
                    }
                }
                _ => {
                    if let Err(err) = output(&registries, format) {
                        eprintln!("Failed to render registries: {err}");
                    }
                }
            }
            ExitCode::Success
        }
        GlobalSkillRegistryCommands::Search(args) => {
            match skill_registry::search_skills(args.registry.as_deref(), args.query.as_deref()) {
                Ok(skills) => match format {
                    OutputFormat::Table => {
                        if skills.is_empty() {
                            println!("No skills found.");
                        } else {
                            println!("{:<25}  {:<12}  DESCRIPTION", "NAME", "REGISTRY");
                            println!("{}", "-".repeat(90));
                            for s in &skills {
                                let desc = if s.description.len() > 50 {
                                    format!("{}...", &s.description[..47])
                                } else {
                                    s.description.clone()
                                };
                                println!("{:<25}  {:<12}  {}", s.name, s.registry, desc);
                            }
                        }
                    }
                    _ => {
                        if let Err(err) = output(&skills, format) {
                            eprintln!("Failed to render skills: {err}");
                        }
                    }
                },
                Err(e) => return output_error(&e, format),
            }
            ExitCode::Success
        }
        GlobalSkillRegistryCommands::RegistryInspect(args) => {
            match skill_registry::inspect_remote_skill(&args.registry, &args.skill_name) {
                Ok(detail) => {
                    print_structured(&detail, format, "remote skill inspect result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalSkillRegistryCommands::Pull(args) => {
            let skill_id = args.skill_id.as_deref().unwrap_or(&args.skill_name);
            let detail =
                match skill_registry::inspect_remote_skill(&args.registry, &args.skill_name) {
                    Ok(d) => d,
                    Err(e) => return output_error(&e, format),
                };

            match registry.global_skill_create(
                skill_id,
                &detail.skill.name,
                &detail.skill.tags,
                &detail.content,
            ) {
                Ok(result) => {
                    match format {
                        OutputFormat::Table => {
                            println!(
                                "Pulled skill '{}' from {} and saved as '{}'",
                                detail.skill.name, args.registry, skill_id
                            );
                        }
                        _ => {
                            print_structured(&result, format, "skill pull result");
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalSkillRegistryCommands::PullGithub(args) => {
            let global_root = registry.governance_global_root();
            let skills_dir = global_root.join("skills");

            match skill_registry::pull_from_github(&args.repo, args.skill.as_deref(), &skills_dir) {
                Ok(results) => {
                    match format {
                        OutputFormat::Table => {
                            if results.is_empty() {
                                println!("No skills found in repository.");
                            } else {
                                println!("Pulled {} skill(s) from {}:", results.len(), args.repo);
                                for r in &results {
                                    println!("  - {} -> {}", r.name, r.path);
                                }
                            }
                        }
                        _ => {
                            print_structured(&results, format, "github pull result");
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

fn print_task(task: &Task, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("ID:          {}", task.id);
            println!("Project:     {}", task.project_id);
            println!("Title:       {}", task.title);
            if let Some(desc) = &task.description {
                println!("Description: {desc}");
            }
            println!("State:       {:?}", task.state);
            println!("RunMode:     {:?}", task.run_mode);
            println!("Created:     {}", task.created_at);
        }
        _ => {
            if let Err(err) = output(task, format) {
                eprintln!("Failed to render task: {err}");
            }
        }
    }
}

fn print_tasks(tasks: &[Task], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if tasks.is_empty() {
                println!("No tasks found.");
                return;
            }
            println!("{:<36}  {:<8}  {:<6}  TITLE", "ID", "STATE", "MODE");
            println!("{}", "-".repeat(80));
            for t in tasks {
                let state = match t.state {
                    TaskState::Open => "open",
                    TaskState::Closed => "closed",
                };
                println!(
                    "{:<36}  {:<8}  {:<6}  {}",
                    t.id,
                    state,
                    format!("{:?}", t.run_mode).to_lowercase(),
                    t.title
                );
            }
        }
        _ => {
            if let Err(err) = output(tasks, format) {
                eprintln!("Failed to render tasks: {err}");
            }
        }
    }
}

fn print_task_id(task_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"task_id": task_id}));
        }
        OutputFormat::Table => {
            println!("Task ID: {task_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"task_id": task_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}

fn parse_task_state(s: &str) -> Option<TaskState> {
    match s.to_lowercase().as_str() {
        "open" => Some(TaskState::Open),
        "closed" => Some(TaskState::Closed),
        _ => None,
    }
}

fn parse_runtime_role(role: RuntimeRoleArg) -> RuntimeRole {
    match role {
        RuntimeRoleArg::Worker => RuntimeRole::Worker,
        RuntimeRoleArg::Validator => RuntimeRole::Validator,
    }
}

fn parse_run_mode(mode: RunModeArg) -> RunMode {
    match mode {
        RunModeArg::Manual => RunMode::Manual,
        RunModeArg::Auto => RunMode::Auto,
    }
}

fn parse_merge_execute_mode(mode: MergeExecuteModeArg) -> MergeExecuteMode {
    match mode {
        MergeExecuteModeArg::Local => MergeExecuteMode::Local,
        MergeExecuteModeArg::Pr => MergeExecuteMode::Pr,
    }
}

fn parse_scope_arg(scope: Option<&str>, format: OutputFormat) -> Result<Option<Scope>, ExitCode> {
    let Some(raw) = scope else {
        return Ok(None);
    };

    match serde_json::from_str::<Scope>(raw) {
        Ok(s) => Ok(Some(s)),
        Err(e) => Err(output_error(
            &hivemind::core::error::HivemindError::user(
                "invalid_scope",
                format!("Invalid scope definition: {e}"),
                "cli:task:create",
            ),
            format,
        )),
    }
}

fn print_attempt_id(attempt_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let info = serde_json::json!({"attempt_id": attempt_id});
            if let Ok(json) = serde_json::to_string_pretty(&info) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let info = serde_json::json!({"attempt_id": attempt_id});
            if let Ok(yaml) = serde_yaml::to_string(&info) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            println!("Attempt ID: {attempt_id}");
        }
    }
}

fn handle_task_create(
    registry: &Registry,
    args: &TaskCreateArgs,
    format: OutputFormat,
) -> ExitCode {
    let scope = match parse_scope_arg(args.scope.as_deref(), format) {
        Ok(s) => s,
        Err(code) => return code,
    };

    match registry.create_task(
        &args.project,
        &args.title,
        args.description.as_deref(),
        scope,
    ) {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_list(registry: &Registry, args: &TaskListArgs, format: OutputFormat) -> ExitCode {
    let state_filter = args.state.as_ref().and_then(|s| parse_task_state(s));
    match registry.list_tasks(&args.project, state_filter) {
        Ok(tasks) => {
            print_tasks(&tasks, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_inspect(
    registry: &Registry,
    args: &TaskInspectArgs,
    format: OutputFormat,
) -> ExitCode {
    match registry.get_task(&args.task_id) {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_update(
    registry: &Registry,
    args: &TaskUpdateArgs,
    format: OutputFormat,
) -> ExitCode {
    match registry.update_task(
        &args.task_id,
        args.title.as_deref(),
        args.description.as_deref(),
    ) {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_runtime_set(
    registry: &Registry,
    args: &hivemind::cli::commands::TaskRuntimeSetArgs,
    format: OutputFormat,
) -> ExitCode {
    let role = parse_runtime_role(args.role);
    let result = if args.clear {
        registry.task_runtime_clear_role(&args.task_id, role)
    } else {
        registry.task_runtime_set_role(
            &args.task_id,
            role,
            &args.adapter,
            &args.binary_path,
            args.model.clone(),
            &args.args,
            &args.env,
            args.timeout_ms,
        )
    };

    match result {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_close(registry: &Registry, args: &TaskCloseArgs, format: OutputFormat) -> ExitCode {
    match registry.close_task(&args.task_id, args.reason.as_deref()) {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn resolve_task_id_with_legacy_project(
    registry: &Registry,
    project_or_task: &str,
    legacy_task_id: Option<&str>,
    origin: &str,
) -> Result<String, hivemind::core::error::HivemindError> {
    if let Some(task_id) = legacy_task_id {
        let project = registry.get_project(project_or_task)?;
        let task = registry.get_task(task_id)?;
        if task.project_id != project.id {
            return Err(hivemind::core::error::HivemindError::user(
                "task_project_mismatch",
                format!(
                    "Task '{task_id}' does not belong to project '{}'",
                    project_or_task
                ),
                origin,
            )
            .with_hint("Pass the matching project/task pair or use `hivemind task <op> <task-id>`")
            .with_context("project", project_or_task)
            .with_context("task_id", task_id));
        }
        return Ok(task_id.to_string());
    }
    Ok(project_or_task.to_string())
}

fn handle_task_start(registry: &Registry, args: &TaskStartArgs, format: OutputFormat) -> ExitCode {
    let task_id = match resolve_task_id_with_legacy_project(
        registry,
        &args.task_id,
        args.legacy_task_id.as_deref(),
        "cli:task:start",
    ) {
        Ok(task_id) => task_id,
        Err(e) => return output_error(&e, format),
    };

    match registry.start_task_execution(&task_id) {
        Ok(attempt_id) => {
            print_attempt_id(attempt_id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_complete(
    registry: &Registry,
    args: &TaskCompleteArgs,
    format: OutputFormat,
) -> ExitCode {
    let task_id = match resolve_task_id_with_legacy_project(
        registry,
        &args.task_id,
        args.legacy_task_id.as_deref(),
        "cli:task:complete",
    ) {
        Ok(task_id) => task_id,
        Err(e) => return output_error(&e, format),
    };

    if matches!(args.success, Some(false)) {
        return match registry.close_task(&task_id, args.message.as_deref()) {
            Ok(task) => {
                print_task(&task, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        };
    }

    match registry.complete_task_execution(&task_id) {
        Ok(flow) => {
            print_flow_id(flow.id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_retry(registry: &Registry, args: &TaskRetryArgs, format: OutputFormat) -> ExitCode {
    let task_id = match resolve_task_id_with_legacy_project(
        registry,
        &args.task_id,
        args.legacy_task_id.as_deref(),
        "cli:task:retry",
    ) {
        Ok(task_id) => task_id,
        Err(e) => return output_error(&e, format),
    };

    let mode = match args.mode {
        hivemind::cli::commands::TaskRetryMode::Clean => RetryMode::Clean,
        hivemind::cli::commands::TaskRetryMode::Continue => RetryMode::Continue,
    };

    match registry.retry_task(&task_id, args.reset_count, mode) {
        Ok(flow) => {
            print_flow_id(flow.id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_abort(registry: &Registry, args: &TaskAbortArgs, format: OutputFormat) -> ExitCode {
    match registry.abort_task(&args.task_id, args.reason.as_deref()) {
        Ok(flow) => {
            print_flow_id(flow.id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task(cmd: TaskCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        TaskCommands::Create(args) => handle_task_create(&registry, &args, format),
        TaskCommands::List(args) => handle_task_list(&registry, &args, format),
        TaskCommands::Inspect(args) => handle_task_inspect(&registry, &args, format),
        TaskCommands::Update(args) => handle_task_update(&registry, &args, format),
        TaskCommands::RuntimeSet(args) => handle_task_runtime_set(&registry, &args, format),
        TaskCommands::Close(args) => handle_task_close(&registry, &args, format),
        TaskCommands::Start(args) => handle_task_start(&registry, &args, format),
        TaskCommands::Complete(args) => handle_task_complete(&registry, &args, format),
        TaskCommands::Retry(args) => handle_task_retry(&registry, &args, format),
        TaskCommands::Abort(args) => handle_task_abort(&registry, &args, format),
        TaskCommands::SetRunMode(args) => {
            match registry.task_set_run_mode(&args.task_id, parse_run_mode(args.mode)) {
                Ok(task) => {
                    print_task(&task, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        TaskCommands::Delete(args) => match registry.delete_task(&args.task_id) {
            Ok(task_id) => {
                print_task_id(task_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}

fn handle_runtime(cmd: RuntimeCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        RuntimeCommands::List => {
            let rows = registry.runtime_list();
            match format {
                OutputFormat::Table => {
                    if rows.is_empty() {
                        println!("No runtimes available.");
                    } else {
                        println!(
                            "{:<14}  {:<20}  {:<10}  OCODE_FAMILY",
                            "ADAPTER", "BINARY", "AVAILABLE"
                        );
                        println!("{}", "-".repeat(72));
                        for row in rows {
                            println!(
                                "{:<14}  {:<20}  {:<10}  {}",
                                row.adapter_name,
                                row.default_binary,
                                if row.available { "yes" } else { "no" },
                                if row.opencode_compatible { "yes" } else { "no" }
                            );
                        }
                    }
                    ExitCode::Success
                }
                _ => output(&rows, format)
                    .map(|()| ExitCode::Success)
                    .unwrap_or(ExitCode::Error),
            }
        }
        RuntimeCommands::Health(args) => {
            match registry.runtime_health_with_role(
                args.project.as_deref(),
                args.task.as_deref(),
                args.flow.as_deref(),
                parse_runtime_role(args.role),
            ) {
                Ok(status) => {
                    match format {
                        OutputFormat::Table => {
                            println!("Adapter: {}", status.adapter_name);
                            println!("Binary: {}", status.binary_path);
                            println!("Healthy: {}", if status.healthy { "yes" } else { "no" });
                            if let Some(ref target) = status.target {
                                println!("Target: {target}");
                            }
                            if let Some(ref details) = status.details {
                                println!("Details: {details}");
                            }
                        }
                        _ => {
                            if output(&status, format).is_err() {
                                return ExitCode::Error;
                            }
                        }
                    }
                    if status.healthy {
                        ExitCode::Success
                    } else {
                        ExitCode::Error
                    }
                }
                Err(e) => output_error(&e, format),
            }
        }
        RuntimeCommands::DefaultsSet(args) => match registry.runtime_defaults_set(
            parse_runtime_role(args.role),
            &args.adapter,
            &args.binary_path,
            args.model,
            &args.args,
            &args.env,
            args.timeout_ms,
            args.max_parallel_tasks,
        ) {
            Ok(()) => {
                if matches!(format, OutputFormat::Table) {
                    println!("ok");
                }
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}

#[allow(clippy::too_many_lines)]
fn event_type_label(payload: &hivemind::core::events::EventPayload) -> &'static str {
    use hivemind::core::events::EventPayload;
    match payload {
        EventPayload::ErrorOccurred { .. } => "error_occurred",
        EventPayload::ProjectCreated { .. } => "project_created",
        EventPayload::ProjectUpdated { .. } => "project_updated",
        EventPayload::ProjectDeleted { .. } => "project_deleted",
        EventPayload::ProjectRuntimeConfigured { .. } => "project_runtime_configured",
        EventPayload::ProjectRuntimeRoleConfigured { .. } => "project_runtime_role_configured",
        EventPayload::GlobalRuntimeConfigured { .. } => "global_runtime_configured",
        EventPayload::TaskCreated { .. } => "task_created",
        EventPayload::TaskUpdated { .. } => "task_updated",
        EventPayload::TaskRuntimeConfigured { .. } => "task_runtime_configured",
        EventPayload::TaskRuntimeRoleConfigured { .. } => "task_runtime_role_configured",
        EventPayload::TaskRuntimeCleared { .. } => "task_runtime_cleared",
        EventPayload::TaskRuntimeRoleCleared { .. } => "task_runtime_role_cleared",
        EventPayload::TaskRunModeSet { .. } => "task_run_mode_set",
        EventPayload::TaskClosed { .. } => "task_closed",
        EventPayload::TaskDeleted { .. } => "task_deleted",
        EventPayload::RepositoryAttached { .. } => "repo_attached",
        EventPayload::RepositoryDetached { .. } => "repo_detached",
        EventPayload::GovernanceProjectStorageInitialized { .. } => {
            "governance_project_storage_initialized"
        }
        EventPayload::GovernanceArtifactUpserted { .. } => "governance_artifact_upserted",
        EventPayload::GovernanceArtifactDeleted { .. } => "governance_artifact_deleted",
        EventPayload::GovernanceAttachmentLifecycleUpdated { .. } => {
            "governance_attachment_lifecycle_updated"
        }
        EventPayload::GovernanceStorageMigrated { .. } => "governance_storage_migrated",
        EventPayload::GovernanceSnapshotCreated { .. } => "governance_snapshot_created",
        EventPayload::GovernanceSnapshotRestored { .. } => "governance_snapshot_restored",
        EventPayload::GovernanceDriftDetected { .. } => "governance_drift_detected",
        EventPayload::GovernanceRepairApplied { .. } => "governance_repair_applied",
        EventPayload::GraphSnapshotStarted { .. } => "graph_snapshot_started",
        EventPayload::GraphSnapshotCompleted { .. } => "graph_snapshot_completed",
        EventPayload::GraphSnapshotFailed { .. } => "graph_snapshot_failed",
        EventPayload::GraphSnapshotDiffDetected { .. } => "graph_snapshot_diff_detected",
        EventPayload::ConstitutionInitialized { .. } => "constitution_initialized",
        EventPayload::ConstitutionUpdated { .. } => "constitution_updated",
        EventPayload::ConstitutionValidated { .. } => "constitution_validated",
        EventPayload::ConstitutionViolationDetected { .. } => "constitution_violation_detected",
        EventPayload::TemplateInstantiated { .. } => "template_instantiated",
        EventPayload::AttemptContextOverridesApplied { .. } => "attempt_context_overrides_applied",
        EventPayload::AttemptContextAssembled { .. } => "attempt_context_assembled",
        EventPayload::AttemptContextTruncated { .. } => "attempt_context_truncated",
        EventPayload::AttemptContextDelivered { .. } => "attempt_context_delivered",
        EventPayload::TaskGraphCreated { .. } => "graph_created",
        EventPayload::TaskAddedToGraph { .. } => "graph_task_added",
        EventPayload::DependencyAdded { .. } => "graph_dependency_added",
        EventPayload::GraphTaskCheckAdded { .. } => "graph_task_check_added",
        EventPayload::ScopeAssigned { .. } => "graph_scope_assigned",
        EventPayload::TaskGraphValidated { .. } => "graph_validated",
        EventPayload::TaskGraphLocked { .. } => "graph_locked",
        EventPayload::TaskGraphDeleted { .. } => "graph_deleted",
        EventPayload::TaskFlowCreated { .. } => "flow_created",
        EventPayload::TaskFlowDependencyAdded { .. } => "flow_dependency_added",
        EventPayload::TaskFlowRunModeSet { .. } => "flow_run_mode_set",
        EventPayload::TaskFlowRuntimeConfigured { .. } => "flow_runtime_configured",
        EventPayload::TaskFlowRuntimeCleared { .. } => "flow_runtime_cleared",
        EventPayload::TaskFlowStarted { .. } => "flow_started",
        EventPayload::TaskFlowPaused { .. } => "flow_paused",
        EventPayload::TaskFlowResumed { .. } => "flow_resumed",
        EventPayload::TaskFlowCompleted { .. } => "flow_completed",
        EventPayload::TaskFlowAborted { .. } => "flow_aborted",
        EventPayload::TaskFlowDeleted { .. } => "flow_deleted",
        EventPayload::TaskReady { .. } => "task_ready",
        EventPayload::TaskBlocked { .. } => "task_blocked",
        EventPayload::ScopeConflictDetected { .. } => "scope_conflict_detected",
        EventPayload::TaskSchedulingDeferred { .. } => "task_scheduling_deferred",
        EventPayload::TaskExecutionStateChanged { .. } => "task_execution_state_changed",
        EventPayload::TaskExecutionStarted { .. } => "task_execution_started",
        EventPayload::TaskExecutionSucceeded { .. } => "task_execution_succeeded",
        EventPayload::TaskExecutionFailed { .. } => "task_execution_failed",
        EventPayload::AttemptStarted { .. } => "attempt_started",
        EventPayload::BaselineCaptured { .. } => "baseline_captured",
        EventPayload::FileModified { .. } => "file_modified",
        EventPayload::DiffComputed { .. } => "diff_computed",
        EventPayload::CheckStarted { .. } => "check_started",
        EventPayload::CheckCompleted { .. } => "check_completed",
        EventPayload::MergeCheckStarted { .. } => "merge_check_started",
        EventPayload::MergeCheckCompleted { .. } => "merge_check_completed",
        EventPayload::CheckpointDeclared { .. } => "checkpoint_declared",
        EventPayload::CheckpointActivated { .. } => "checkpoint_activated",
        EventPayload::CheckpointCompleted { .. } => "checkpoint_completed",
        EventPayload::AllCheckpointsCompleted { .. } => "all_checkpoints_completed",
        EventPayload::CheckpointCommitCreated { .. } => "checkpoint_commit_created",
        EventPayload::ScopeValidated { .. } => "scope_validated",
        EventPayload::ScopeViolationDetected { .. } => "scope_violation_detected",
        EventPayload::RetryContextAssembled { .. } => "retry_context_assembled",
        EventPayload::TaskRetryRequested { .. } => "task_retried",
        EventPayload::TaskAborted { .. } => "task_aborted",
        EventPayload::HumanOverride { .. } => "human_override",
        EventPayload::MergePrepared { .. } => "merge_prepared",
        EventPayload::TaskExecutionFrozen { .. } => "task_execution_frozen",
        EventPayload::TaskIntegratedIntoFlow { .. } => "task_integrated_into_flow",
        EventPayload::MergeConflictDetected { .. } => "merge_conflict_detected",
        EventPayload::FlowFrozenForMerge { .. } => "flow_frozen_for_merge",
        EventPayload::FlowIntegrationLockAcquired { .. } => "flow_integration_lock_acquired",
        EventPayload::MergeApproved { .. } => "merge_approved",
        EventPayload::MergeCompleted { .. } => "merge_completed",
        EventPayload::WorktreeCleanupPerformed { .. } => "worktree_cleanup_performed",
        EventPayload::RuntimeStarted { .. } => "runtime_started",
        EventPayload::RuntimeOutputChunk { .. } => "runtime_output_chunk",
        EventPayload::RuntimeInputProvided { .. } => "runtime_input_provided",
        EventPayload::RuntimeInterrupted { .. } => "runtime_interrupted",
        EventPayload::RuntimeExited { .. } => "runtime_exited",
        EventPayload::RuntimeTerminated { .. } => "runtime_terminated",
        EventPayload::RuntimeErrorClassified { .. } => "runtime_error_classified",
        EventPayload::RuntimeRecoveryScheduled { .. } => "runtime_recovery_scheduled",
        EventPayload::RuntimeFilesystemObserved { .. } => "runtime_filesystem_observed",
        EventPayload::RuntimeCommandObserved { .. } => "runtime_command_observed",
        EventPayload::RuntimeToolCallObserved { .. } => "runtime_tool_call_observed",
        EventPayload::RuntimeTodoSnapshotUpdated { .. } => "runtime_todo_snapshot_updated",
        EventPayload::RuntimeNarrativeOutputObserved { .. } => "runtime_narrative_output_observed",
        EventPayload::Unknown => "unknown",
    }
}

fn print_events_table(events: Vec<hivemind::core::events::Event>) {
    if events.is_empty() {
        println!("No events found.");
        return;
    }
    println!("{:<6}  {:<24}  {:<22}  PROJECT", "SEQ", "TYPE", "TIMESTAMP",);
    println!("{}", "-".repeat(90));
    for ev in events {
        let seq = ev.metadata.sequence.unwrap_or(0);
        let typ = event_type_label(&ev.payload);
        let ts = ev.metadata.timestamp.to_rfc3339();
        let proj = ev
            .metadata
            .correlation
            .project_id
            .map_or_else(|| "-".to_string(), |p| p.to_string());
        println!("{seq:<6}  {typ:<24}  {ts:<22}  {proj}");
    }
}

fn parse_event_uuid(
    raw: &str,
    code: &str,
    noun: &str,
    origin: &str,
) -> Result<Uuid, hivemind::core::error::HivemindError> {
    Uuid::parse_str(raw).map_err(|_| {
        hivemind::core::error::HivemindError::user(
            code,
            format!("'{raw}' is not a valid {noun} ID"),
            origin,
        )
    })
}

fn parse_event_time(
    raw: &str,
    flag: &str,
    origin: &str,
) -> Result<DateTime<Utc>, hivemind::core::error::HivemindError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| {
            hivemind::core::error::HivemindError::user(
                "invalid_timestamp",
                format!("Invalid {flag} timestamp '{raw}'. Expected RFC3339 format."),
                origin,
            )
        })
}

fn parse_non_empty_filter(
    raw: &str,
    code: &str,
    flag: &str,
    origin: &str,
) -> Result<String, hivemind::core::error::HivemindError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(hivemind::core::error::HivemindError::user(
            code,
            format!("{flag} cannot be empty"),
            origin,
        ));
    }
    Ok(normalized.to_string())
}

#[allow(clippy::too_many_arguments)]
fn build_event_filter(
    registry: &Registry,
    origin: &str,
    project: Option<&str>,
    graph: Option<&str>,
    flow: Option<&str>,
    task: Option<&str>,
    attempt: Option<&str>,
    artifact_id: Option<&str>,
    template_id: Option<&str>,
    rule_id: Option<&str>,
    error_type: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    limit: usize,
) -> Result<hivemind::storage::event_store::EventFilter, hivemind::core::error::HivemindError> {
    use hivemind::storage::event_store::EventFilter;

    let mut filter = EventFilter::all();
    filter.limit = Some(limit);

    if let Some(project) = project {
        filter.project_id = Some(registry.get_project(project)?.id);
    }
    if let Some(graph) = graph {
        filter.graph_id = Some(parse_event_uuid(
            graph,
            "invalid_graph_id",
            "graph",
            origin,
        )?);
    }
    if let Some(flow) = flow {
        filter.flow_id = Some(parse_event_uuid(flow, "invalid_flow_id", "flow", origin)?);
    }
    if let Some(task) = task {
        filter.task_id = Some(parse_event_uuid(task, "invalid_task_id", "task", origin)?);
    }
    if let Some(attempt) = attempt {
        filter.attempt_id = Some(parse_event_uuid(
            attempt,
            "invalid_attempt_id",
            "attempt",
            origin,
        )?);
    }
    if let Some(artifact_id) = artifact_id {
        filter.artifact_id = Some(parse_non_empty_filter(
            artifact_id,
            "invalid_artifact_id",
            "--artifact-id",
            origin,
        )?);
    }
    if let Some(template_id) = template_id {
        filter.template_id = Some(parse_non_empty_filter(
            template_id,
            "invalid_template_id",
            "--template-id",
            origin,
        )?);
    }
    if let Some(rule_id) = rule_id {
        filter.rule_id = Some(parse_non_empty_filter(
            rule_id,
            "invalid_rule_id",
            "--rule-id",
            origin,
        )?);
    }
    if let Some(error_type) = error_type {
        filter.error_type = Some(parse_non_empty_filter(
            error_type,
            "invalid_error_type",
            "--error-type",
            origin,
        )?);
    }
    if let Some(since) = since {
        filter.since = Some(parse_event_time(since, "--since", origin)?);
    }
    if let Some(until) = until {
        filter.until = Some(parse_event_time(until, "--until", origin)?);
    }

    if filter
        .since
        .zip(filter.until)
        .is_some_and(|(since, until)| since > until)
    {
        return Err(hivemind::core::error::HivemindError::user(
            "invalid_time_range",
            "`--since` must be earlier than or equal to `--until`",
            origin,
        ));
    }

    Ok(filter)
}

#[allow(clippy::too_many_lines)]
fn handle_events(cmd: EventCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        EventCommands::List(args) => {
            let filter = match build_event_filter(
                &registry,
                "cli:events:list",
                args.project.as_deref(),
                args.graph.as_deref(),
                args.flow.as_deref(),
                args.task.as_deref(),
                args.attempt.as_deref(),
                args.artifact_id.as_deref(),
                args.template_id.as_deref(),
                args.rule_id.as_deref(),
                args.error_type.as_deref(),
                args.since.as_deref(),
                args.until.as_deref(),
                args.limit,
            ) {
                Ok(f) => f,
                Err(e) => return output_error(&e, format),
            };

            let events = match registry.read_events(&filter) {
                Ok(evs) => evs,
                Err(e) => return output_error(&e, format),
            };

            match format {
                OutputFormat::Json => {
                    let response = hivemind::cli::output::CliResponse::success(&events);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                }
                OutputFormat::Yaml => {
                    let response = hivemind::cli::output::CliResponse::success(&events);
                    if let Ok(yaml) = serde_yaml::to_string(&response) {
                        print!("{yaml}");
                    }
                }
                OutputFormat::Table => print_events_table(events),
            }

            ExitCode::Success
        }
        EventCommands::Inspect(args) => {
            let event = match registry.get_event(&args.event_id) {
                Ok(ev) => ev,
                Err(e) => return output_error(&e, format),
            };

            match format {
                OutputFormat::Json => {
                    let response = hivemind::cli::output::CliResponse::success(&event);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                }
                OutputFormat::Yaml => {
                    if let Ok(yaml) = serde_yaml::to_string(&event) {
                        print!("{yaml}");
                    }
                }
                OutputFormat::Table => {
                    if let Ok(json) = serde_json::to_string_pretty(&event) {
                        println!("{json}");
                    }
                }
            }

            ExitCode::Success
        }
        EventCommands::Stream(args) => {
            let filter = match build_event_filter(
                &registry,
                "cli:events:stream",
                args.project.as_deref(),
                args.graph.as_deref(),
                args.flow.as_deref(),
                args.task.as_deref(),
                args.attempt.as_deref(),
                args.artifact_id.as_deref(),
                args.template_id.as_deref(),
                args.rule_id.as_deref(),
                args.error_type.as_deref(),
                args.since.as_deref(),
                args.until.as_deref(),
                args.limit,
            ) {
                Ok(f) => f,
                Err(e) => return output_error(&e, format),
            };

            let rx = match registry.stream_events(&filter) {
                Ok(r) => r,
                Err(e) => return output_error(&e, format),
            };

            let mut events = Vec::new();
            let idle_timeout = std::time::Duration::from_millis(1000);
            while let Ok(ev) = rx.recv_timeout(idle_timeout) {
                events.push(ev);
                if events.len() >= args.limit {
                    break;
                }
            }

            match format {
                OutputFormat::Json => {
                    let response = hivemind::cli::output::CliResponse::success(&events);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                }
                OutputFormat::Yaml => {
                    let response = hivemind::cli::output::CliResponse::success(&events);
                    if let Ok(yaml) = serde_yaml::to_string(&response) {
                        print!("{yaml}");
                    }
                }
                OutputFormat::Table => print_events_table(events),
            }

            ExitCode::Success
        }
        EventCommands::Replay(args) => {
            let replayed = match registry.replay_flow(&args.flow_id) {
                Ok(f) => f,
                Err(e) => return output_error(&e, format),
            };

            if args.verify {
                let current = match registry.get_flow(&args.flow_id) {
                    Ok(f) => f,
                    Err(e) => return output_error(&e, format),
                };

                let match_ok = replayed.state == current.state
                    && replayed.task_executions.len() == current.task_executions.len()
                    && replayed.task_executions.iter().all(|(tid, exec)| {
                        current
                            .task_executions
                            .get(tid)
                            .is_some_and(|ce| ce.state == exec.state)
                    });

                if match_ok {
                    println!("Verification passed: replayed state matches current state.");
                } else {
                    eprintln!("Verification FAILED: replayed state differs from current state.");
                    return ExitCode::Error;
                }
            }

            match format {
                OutputFormat::Json => {
                    if let Ok(json) = serde_json::to_string_pretty(&replayed) {
                        println!("{json}");
                    }
                }
                OutputFormat::Yaml => {
                    if let Ok(yaml) = serde_yaml::to_string(&replayed) {
                        print!("{yaml}");
                    }
                }
                OutputFormat::Table => {
                    println!("ID:      {}", replayed.id);
                    println!("Graph:   {}", replayed.graph_id);
                    println!("State:   {:?}", replayed.state);
                    let mut counts = std::collections::HashMap::new();
                    for exec in replayed.task_executions.values() {
                        *counts.entry(exec.state).or_insert(0usize) += 1;
                    }
                    println!("Tasks:");
                    let mut keys: Vec<_> = counts.keys().copied().collect();
                    keys.sort_by_key(|k| format!("{k:?}"));
                    for k in keys {
                        println!("  - {:?}: {}", k, counts[&k]);
                    }
                }
            }

            ExitCode::Success
        }
        EventCommands::Verify(_args) => match registry.events_verify() {
            Ok(result) => {
                print_structured(&result, format, "events verify result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        EventCommands::Recover(args) => {
            if !args.from_mirror {
                return output_error(
                    &hivemind::core::error::HivemindError::user(
                        "events_recover_source_required",
                        "Specify an explicit source for recovery",
                        "cli:events:recover",
                    )
                    .with_hint("Use `hivemind events recover --from-mirror --confirm`"),
                    format,
                );
            }

            match registry.events_recover_from_mirror(args.confirm) {
                Ok(result) => {
                    print_structured(&result, format, "events recover result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

fn handle_verify(cmd: VerifyCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        VerifyCommands::Override(args) => {
            match registry.verify_override(&args.task_id, &args.decision, &args.reason) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        VerifyCommands::Run(args) => match registry.verify_run(&args.task_id) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        VerifyCommands::Results(args) => match registry.get_attempt(&args.attempt_id) {
            Ok(attempt) => {
                let check_results: Vec<serde_json::Value> = attempt
                    .check_results
                    .iter()
                    .map(|r| {
                        if args.output {
                            serde_json::json!(r)
                        } else {
                            serde_json::json!({
                                "name": r.name,
                                "passed": r.passed,
                                "exit_code": r.exit_code,
                                "duration_ms": r.duration_ms,
                                "required": r.required,
                            })
                        }
                    })
                    .collect();

                let info = serde_json::json!({
                    "attempt_id": attempt.id,
                    "task_id": attempt.task_id,
                    "flow_id": attempt.flow_id,
                    "attempt_number": attempt.attempt_number,
                    "check_results": check_results,
                });

                match format {
                    OutputFormat::Json => {
                        if let Ok(json) = serde_json::to_string_pretty(&info) {
                            println!("{json}");
                        }
                    }
                    OutputFormat::Yaml => {
                        if let Ok(yaml) = serde_yaml::to_string(&info) {
                            print!("{yaml}");
                        }
                    }
                    OutputFormat::Table => {
                        println!("Attempt:  {}", attempt.id);
                        println!("Task:     {}", attempt.task_id);
                        println!("Flow:     {}", attempt.flow_id);
                        println!("Number:   {}", attempt.attempt_number);
                        if attempt.check_results.is_empty() {
                            println!("No check results found.");
                        } else {
                            println!("Checks:");
                            for r in &attempt.check_results {
                                let status = if r.passed { "PASS" } else { "FAIL" };
                                let required = if r.required { "required" } else { "optional" };
                                println!(
                                    "  - {}: {} ({required}) exit={} duration={}ms",
                                    r.name, status, r.exit_code, r.duration_ms
                                );
                                if args.output && !r.output.trim().is_empty() {
                                    println!("    Output:\n{}", r.output.trim_end());
                                }
                            }
                        }
                    }
                }

                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}

fn handle_merge(cmd: MergeCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        MergeCommands::Prepare(args) => {
            match registry.merge_prepare(&args.flow_id, args.target.as_deref()) {
                Ok(ms) => {
                    match format {
                        OutputFormat::Json => {
                            if let Ok(json) = serde_json::to_string_pretty(&ms) {
                                println!("{json}");
                            }
                        }
                        OutputFormat::Yaml => {
                            if let Ok(yaml) = serde_yaml::to_string(&ms) {
                                print!("{yaml}");
                            }
                        }
                        OutputFormat::Table => {
                            println!("Flow:     {}", ms.flow_id);
                            println!("Status:   {:?}", ms.status);
                            if let Some(ref branch) = ms.target_branch {
                                println!("Target:   {branch}");
                            }
                            if !ms.conflicts.is_empty() {
                                println!("Conflicts:");
                                for c in &ms.conflicts {
                                    println!("  - {c}");
                                }
                            }
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        MergeCommands::Approve(args) => match registry.merge_approve(&args.flow_id) {
            Ok(ms) => {
                match format {
                    OutputFormat::Json => {
                        if let Ok(json) = serde_json::to_string_pretty(&ms) {
                            println!("{json}");
                        }
                    }
                    OutputFormat::Yaml => {
                        if let Ok(yaml) = serde_yaml::to_string(&ms) {
                            print!("{yaml}");
                        }
                    }
                    OutputFormat::Table => {
                        println!("Flow:   {}", ms.flow_id);
                        println!("Status: {:?}", ms.status);
                    }
                }
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        MergeCommands::Execute(args) => match registry.merge_execute_with_options(
            &args.flow_id,
            MergeExecuteOptions {
                mode: parse_merge_execute_mode(args.mode),
                monitor_ci: args.monitor_ci,
                auto_merge: args.auto_merge,
                pull_after: args.pull_after,
            },
        ) {
            Ok(ms) => {
                match format {
                    OutputFormat::Json => {
                        if let Ok(json) = serde_json::to_string_pretty(&ms) {
                            println!("{json}");
                        }
                    }
                    OutputFormat::Yaml => {
                        if let Ok(yaml) = serde_yaml::to_string(&ms) {
                            print!("{yaml}");
                        }
                    }
                    OutputFormat::Table => {
                        println!("Flow:   {}", ms.flow_id);
                        println!("Status: {:?}", ms.status);
                        if !ms.commits.is_empty() {
                            println!("Commits:");
                            for c in &ms.commits {
                                println!("  - {c}");
                            }
                        }
                    }
                }
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}

fn handle_attempt(cmd: AttemptCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        AttemptCommands::List(args) => {
            match registry.list_attempts(args.flow.as_deref(), args.task.as_deref(), args.limit) {
                Ok(attempts) => match format {
                    OutputFormat::Table => {
                        if attempts.is_empty() {
                            println!("No attempts found.");
                        } else {
                            println!(
                                "{:<36}  {:<36}  {:<36}  {:<8}  CHECKPOINTS",
                                "ATTEMPT", "FLOW", "TASK", "NUMBER"
                            );
                            println!("{}", "-".repeat(170));
                            for attempt in attempts {
                                println!(
                                    "{:<36}  {:<36}  {:<36}  {:<8}  {}",
                                    attempt.attempt_id,
                                    attempt.flow_id,
                                    attempt.task_id,
                                    attempt.attempt_number,
                                    if attempt.all_checkpoints_completed {
                                        "all_completed"
                                    } else {
                                        "incomplete"
                                    }
                                );
                            }
                        }
                        ExitCode::Success
                    }
                    _ => output(&attempts, format)
                        .map(|()| ExitCode::Success)
                        .unwrap_or(ExitCode::Error),
                },
                Err(e) => output_error(&e, format),
            }
        }
        AttemptCommands::Inspect(args) => handle_attempt_inspect(&registry, &args, format),
    }
}

fn handle_checkpoint(cmd: CheckpointCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        CheckpointCommands::List(args) => match registry.list_checkpoints(&args.attempt_id) {
            Ok(checkpoints) => match format {
                OutputFormat::Table => {
                    if checkpoints.is_empty() {
                        println!("No checkpoints declared for this attempt.");
                    } else {
                        println!(
                            "{:<32}  {:<9}  {:<11}  {:<12}  SUMMARY",
                            "CHECKPOINT", "ORDER", "STATE", "COMMIT"
                        );
                        println!("{}", "-".repeat(120));
                        for checkpoint in checkpoints {
                            let commit = checkpoint.commit_hash.unwrap_or_else(|| "-".to_string());
                            let summary = checkpoint.summary.unwrap_or_default();
                            println!(
                                "{:<32}  {:<9}  {:<11}  {:<12}  {}",
                                checkpoint.checkpoint_id,
                                format!("{}/{}", checkpoint.order, checkpoint.total),
                                format!("{:?}", checkpoint.state).to_lowercase(),
                                commit,
                                summary
                            );
                        }
                    }
                    ExitCode::Success
                }
                _ => output(&checkpoints, format)
                    .map(|()| ExitCode::Success)
                    .unwrap_or(ExitCode::Error),
            },
            Err(e) => output_error(&e, format),
        },
        CheckpointCommands::Complete(args) => {
            let attempt_id = args
                .attempt_id
                .or_else(|| std::env::var("HIVEMIND_ATTEMPT_ID").ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let Some(attempt_id) = attempt_id else {
                return output_error(
                    &hivemind::core::error::HivemindError::user(
                        "attempt_id_required",
                        "Attempt ID is required (pass --attempt-id or set HIVEMIND_ATTEMPT_ID)",
                        "cli:checkpoint:complete",
                    ),
                    format,
                );
            };

            match registry.checkpoint_complete(
                &attempt_id,
                &args.checkpoint_id,
                args.summary.as_deref(),
            ) {
                Ok(result) => {
                    match format {
                        OutputFormat::Json | OutputFormat::Yaml => {
                            if let Err(err) = output(&result, format) {
                                eprintln!("Failed to render checkpoint result: {err}");
                            }
                        }
                        OutputFormat::Table => {
                            println!("Attempt:    {}", result.attempt_id);
                            println!(
                                "Checkpoint: {} ({}/{})",
                                result.checkpoint_id, result.order, result.total
                            );
                            println!("Commit:     {}", result.commit_hash);
                            if let Some(next) = result.next_checkpoint_id {
                                println!("Next:       {next}");
                            } else {
                                println!("Next:       none");
                            }
                            println!("All done:   {}", result.all_completed);
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

fn handle_attempt_inspect(
    registry: &Registry,
    args: &AttemptInspectArgs,
    format: OutputFormat,
) -> ExitCode {
    let attempt_id = &args.attempt_id;
    let Ok(parsed) = Uuid::parse_str(attempt_id) else {
        return output_error(
            &hivemind::core::error::HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                "cli:attempt:inspect",
            ),
            format,
        );
    };

    match try_print_attempt_from_events(registry, parsed, args, format) {
        Ok(Some(code)) => return code,
        Ok(None) => {}
        Err(e) => return output_error(&e, format),
    }

    match registry.get_attempt(attempt_id) {
        Ok(attempt) => {
            print_attempt_inspect_attempt(registry, &attempt, args.diff, args.context, format)
        }
        Err(e) if e.code == "attempt_not_found" => {
            print_attempt_inspect_task_fallback(registry, parsed, args.diff, format, &e)
        }
        Err(e) => output_error(&e, format),
    }
}

fn print_attempt_inspect_attempt(
    registry: &Registry,
    attempt: &AttemptState,
    show_diff: bool,
    show_context: bool,
    format: OutputFormat,
) -> ExitCode {
    let diff = if show_diff {
        match registry.get_attempt_diff(&attempt.id.to_string()) {
            Ok(d) => d,
            Err(e) => return output_error(&e, format),
        }
    } else {
        None
    };
    let context_value = if show_context {
        attempt_context_from_events(registry, attempt.id)
    } else {
        None
    };

    let info = serde_json::json!({
        "attempt_id": attempt.id,
        "task_id": attempt.task_id,
        "flow_id": attempt.flow_id,
        "attempt_number": attempt.attempt_number,
        "started_at": attempt.started_at,
        "baseline_id": attempt.baseline_id,
        "diff_id": attempt.diff_id,
        "diff": diff,
        "context": context_value,
    });

    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(&info) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(&info) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            println!("Attempt:  {}", attempt.id);
            println!("Task:     {}", attempt.task_id);
            println!("Flow:     {}", attempt.flow_id);
            println!("Number:   {}", attempt.attempt_number);
            println!("Started:  {}", attempt.started_at);
            if let Some(b) = attempt.baseline_id {
                println!("Baseline: {b}");
            }
            if let Some(did) = attempt.diff_id {
                println!("Diff:     {did}");
            }
            if let Some(ctx) = context_value {
                if let Ok(rendered) = serde_json::to_string_pretty(&ctx) {
                    println!("Context:\n{rendered}");
                }
            }
            if let Some(d) = diff {
                println!("{d}");
            }
        }
    }

    ExitCode::Success
}

struct AttemptInspectCollected {
    stdout: String,
    stderr: String,
    adapter_name: Option<String>,
    task_id: Option<Uuid>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    terminated_reason: Option<String>,
    files_created: Vec<std::path::PathBuf>,
    files_modified: Vec<std::path::PathBuf>,
    files_deleted: Vec<std::path::PathBuf>,
    retry_context: Option<String>,
    context_manifest: Option<serde_json::Value>,
    context_manifest_hash: Option<String>,
    context_inputs_hash: Option<String>,
    delivered_context_hash: Option<String>,
}

fn collect_attempt_runtime_data(
    events: &[hivemind::core::events::Event],
) -> AttemptInspectCollected {
    use hivemind::core::events::{EventPayload, RuntimeOutputStream};

    let mut collected = AttemptInspectCollected {
        stdout: String::new(),
        stderr: String::new(),
        adapter_name: None,
        task_id: None,
        exit_code: None,
        duration_ms: None,
        terminated_reason: None,
        files_created: Vec::new(),
        files_modified: Vec::new(),
        files_deleted: Vec::new(),
        retry_context: None,
        context_manifest: None,
        context_manifest_hash: None,
        context_inputs_hash: None,
        delivered_context_hash: None,
    };

    for ev in events {
        match &ev.payload {
            EventPayload::RuntimeStarted {
                adapter_name,
                task_id,
                ..
            } => {
                collected.adapter_name = Some(adapter_name.clone());
                collected.task_id = Some(*task_id);
            }
            EventPayload::RuntimeOutputChunk {
                attempt_id: _,
                stream,
                content,
            } => match stream {
                RuntimeOutputStream::Stdout => {
                    collected.stdout.push_str(content);
                    collected.stdout.push('\n');
                }
                RuntimeOutputStream::Stderr => {
                    collected.stderr.push_str(content);
                    collected.stderr.push('\n');
                }
            },
            EventPayload::RuntimeExited {
                attempt_id: _,
                exit_code,
                duration_ms,
            } => {
                collected.exit_code = Some(*exit_code);
                collected.duration_ms = Some(*duration_ms);
            }
            EventPayload::RuntimeTerminated {
                attempt_id: _,
                reason,
            } => {
                collected.terminated_reason = Some(reason.clone());
            }
            EventPayload::RuntimeFilesystemObserved {
                attempt_id: _,
                files_created,
                files_modified,
                files_deleted,
            } => {
                collected.files_created.clone_from(files_created);
                collected.files_modified.clone_from(files_modified);
                collected.files_deleted.clone_from(files_deleted);
            }
            EventPayload::RetryContextAssembled { context, .. } => {
                collected.retry_context = Some(context.clone());
            }
            EventPayload::AttemptContextAssembled {
                manifest_hash,
                inputs_hash,
                manifest_json,
                ..
            } => {
                collected.context_manifest_hash = Some(manifest_hash.clone());
                collected.context_inputs_hash = Some(inputs_hash.clone());
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(manifest_json) {
                    collected.context_manifest = Some(parsed);
                } else {
                    collected.context_manifest =
                        Some(serde_json::Value::String(manifest_json.clone()));
                }
            }
            EventPayload::AttemptContextDelivered { context_hash, .. } => {
                collected.delivered_context_hash = Some(context_hash.clone());
            }
            _ => {}
        }
    }

    collected
}

fn build_attempt_inspect_json(
    attempt_id: Uuid,
    corr: &hivemind::core::events::CorrelationIds,
    collected: &AttemptInspectCollected,
    args: &hivemind::cli::commands::AttemptInspectArgs,
) -> serde_json::Value {
    let mut info = serde_json::Map::new();

    info.insert(
        "attempt_id".to_string(),
        serde_json::Value::String(attempt_id.to_string()),
    );
    if let Some(pid) = corr.project_id {
        info.insert(
            "project_id".to_string(),
            serde_json::Value::String(pid.to_string()),
        );
    }
    if let Some(gid) = corr.graph_id {
        info.insert(
            "graph_id".to_string(),
            serde_json::Value::String(gid.to_string()),
        );
    }
    if let Some(fid) = corr.flow_id {
        info.insert(
            "flow_id".to_string(),
            serde_json::Value::String(fid.to_string()),
        );
    }
    if let Some(tid) = collected.task_id.or(corr.task_id) {
        info.insert(
            "task_id".to_string(),
            serde_json::Value::String(tid.to_string()),
        );
    }
    if let Some(an) = collected.adapter_name.clone() {
        info.insert("adapter_name".to_string(), serde_json::Value::String(an));
    }
    if let Some(ec) = collected.exit_code {
        info.insert(
            "exit_code".to_string(),
            serde_json::Value::Number(ec.into()),
        );
    }
    if let Some(dm) = collected.duration_ms {
        info.insert(
            "duration_ms".to_string(),
            serde_json::Value::Number(dm.into()),
        );
    }
    if let Some(reason) = collected.terminated_reason.clone() {
        info.insert(
            "terminated_reason".to_string(),
            serde_json::Value::String(reason),
        );
    }

    if args.output {
        info.insert(
            "stdout".to_string(),
            serde_json::Value::String(collected.stdout.clone()),
        );
        info.insert(
            "stderr".to_string(),
            serde_json::Value::String(collected.stderr.clone()),
        );
    }
    if args.diff {
        info.insert(
            "files_created".to_string(),
            serde_json::to_value(&collected.files_created).unwrap_or(serde_json::Value::Null),
        );
        info.insert(
            "files_modified".to_string(),
            serde_json::to_value(&collected.files_modified).unwrap_or(serde_json::Value::Null),
        );
        info.insert(
            "files_deleted".to_string(),
            serde_json::to_value(&collected.files_deleted).unwrap_or(serde_json::Value::Null),
        );
    }
    if args.context {
        info.insert(
            "context".to_string(),
            serde_json::json!({
                "retry": collected.retry_context.clone(),
                "manifest": collected.context_manifest.clone(),
                "manifest_hash": collected.context_manifest_hash.clone(),
                "inputs_hash": collected.context_inputs_hash.clone(),
                "delivered_context_hash": collected.delivered_context_hash.clone(),
            }),
        );
    }

    serde_json::Value::Object(info)
}

fn print_attempt_inspect_table(
    attempt_id: Uuid,
    corr: &hivemind::core::events::CorrelationIds,
    collected: &AttemptInspectCollected,
    args: &hivemind::cli::commands::AttemptInspectArgs,
) {
    println!("Attempt:  {attempt_id}");
    if let Some(fid) = corr.flow_id {
        println!("Flow:     {fid}");
    }
    if let Some(tid) = collected.task_id.or(corr.task_id) {
        println!("Task:     {tid}");
    }
    if let Some(an) = collected.adapter_name.as_ref() {
        println!("Adapter:  {an}");
    }
    if let Some(ec) = collected.exit_code {
        println!("Exit:     {ec}");
    }
    if let Some(dm) = collected.duration_ms {
        println!("Duration: {dm}ms");
    }
    if let Some(reason) = collected.terminated_reason.as_ref() {
        println!("Terminated: {reason}");
    }
    if args.diff {
        println!("Changes:");
        if !collected.files_created.is_empty() {
            println!("  Created:");
            for p in &collected.files_created {
                println!("    - {}", p.display());
            }
        }
        if !collected.files_modified.is_empty() {
            println!("  Modified:");
            for p in &collected.files_modified {
                println!("    - {}", p.display());
            }
        }
        if !collected.files_deleted.is_empty() {
            println!("  Deleted:");
            for p in &collected.files_deleted {
                println!("    - {}", p.display());
            }
        }
    }
    if args.output {
        println!("Stdout:\n{}", collected.stdout);
        println!("Stderr:\n{}", collected.stderr);
    }
    if args.context {
        println!("Context:");
        if let Some(hash) = collected.context_manifest_hash.as_ref() {
            println!("  Manifest hash: {hash}");
        }
        if let Some(hash) = collected.context_inputs_hash.as_ref() {
            println!("  Inputs hash:   {hash}");
        }
        if let Some(hash) = collected.delivered_context_hash.as_ref() {
            println!("  Delivered hash:{hash}");
        }
        if let Some(ctx) = collected.retry_context.as_ref() {
            println!("  Retry:\n{ctx}");
        } else {
            println!("  Retry: (none)");
        }
        if let Some(manifest) = collected.context_manifest.as_ref() {
            if let Ok(rendered) = serde_json::to_string_pretty(manifest) {
                println!("  Manifest:\n{rendered}");
            }
        } else {
            println!("  Manifest: (none)");
        }
    }
}

fn attempt_context_from_events(registry: &Registry, attempt_id: Uuid) -> Option<serde_json::Value> {
    use hivemind::core::events::EventPayload;
    use hivemind::storage::event_store::EventFilter;

    let mut filter = EventFilter::all();
    filter.attempt_id = Some(attempt_id);
    let events = registry.read_events(&filter).ok()?;
    let mut retry_context: Option<String> = None;
    let mut manifest: Option<serde_json::Value> = None;
    let mut manifest_hash: Option<String> = None;
    let mut inputs_hash: Option<String> = None;
    let mut delivered_context_hash: Option<String> = None;

    for event in events {
        match event.payload {
            EventPayload::RetryContextAssembled { context, .. } => {
                retry_context = Some(context);
            }
            EventPayload::AttemptContextAssembled {
                manifest_hash: hash,
                inputs_hash: in_hash,
                manifest_json,
                ..
            } => {
                manifest_hash = Some(hash);
                inputs_hash = Some(in_hash);
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&manifest_json) {
                    manifest = Some(parsed);
                } else {
                    manifest = Some(serde_json::Value::String(manifest_json));
                }
            }
            EventPayload::AttemptContextDelivered { context_hash, .. } => {
                delivered_context_hash = Some(context_hash);
            }
            _ => {}
        }
    }

    if retry_context.is_none() && manifest.is_none() {
        None
    } else {
        Some(serde_json::json!({
            "retry": retry_context,
            "manifest": manifest,
            "manifest_hash": manifest_hash,
            "inputs_hash": inputs_hash,
            "delivered_context_hash": delivered_context_hash,
        }))
    }
}

fn try_print_attempt_from_events(
    registry: &Registry,
    attempt_id: Uuid,
    args: &hivemind::cli::commands::AttemptInspectArgs,
    format: OutputFormat,
) -> Result<Option<ExitCode>, hivemind::core::error::HivemindError> {
    use hivemind::storage::event_store::EventFilter;

    let mut filter = EventFilter::all();
    filter.attempt_id = Some(attempt_id);
    let events = registry.read_events(&filter)?;
    if events.is_empty() {
        return Ok(None);
    }

    let corr = events[0].metadata.correlation.clone();

    let collected = collect_attempt_runtime_data(&events);
    let info = build_attempt_inspect_json(attempt_id, &corr, &collected, args);
    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(&info) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(&info) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            print_attempt_inspect_table(attempt_id, &corr, &collected, args);

            if args.diff {
                if let Some(diff) = registry.get_attempt_diff(&attempt_id.to_string())? {
                    println!("{diff}");
                }
            }
        }
    }

    Ok(Some(ExitCode::Success))
}

fn print_attempt_inspect_task_fallback(
    registry: &Registry,
    task_id: Uuid,
    show_diff: bool,
    format: OutputFormat,
    original_error: &hivemind::core::error::HivemindError,
) -> ExitCode {
    let state = match registry.state() {
        Ok(s) => s,
        Err(e) => return output_error(&e, format),
    };

    let exec_info = state
        .flows
        .values()
        .find_map(|flow| flow.task_executions.get(&task_id).map(|exec| (flow, exec)));

    let Some((flow, exec)) = exec_info else {
        return output_error(original_error, format);
    };

    let latest_attempt = state
        .attempts
        .values()
        .filter(|a| a.task_id == task_id && a.flow_id == flow.id)
        .max_by_key(|a| a.started_at);

    let diff = if show_diff {
        latest_attempt
            .and_then(|a| a.diff_id.map(|_| a.id))
            .and_then(|aid| registry.get_attempt_diff(&aid.to_string()).ok())
            .flatten()
    } else {
        None
    };

    let info = serde_json::json!({
        "task_id": exec.task_id,
        "state": format!("{:?}", exec.state),
        "attempt_count": exec.attempt_count,
        "blocked_reason": exec.blocked_reason,
        "flow_id": flow.id,
        "diff": diff,
    });

    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(&info) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(&info) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            println!("Task:     {}", exec.task_id);
            println!("Flow:     {}", flow.id);
            println!("State:    {:?}", exec.state);
            println!("Attempts: {}", exec.attempt_count);
            if let Some(ref reason) = exec.blocked_reason {
                println!("Blocked:  {reason}");
            }
            if let Some(d) = info.get("diff").and_then(|v| v.as_str()) {
                println!("{d}");
            }
        }
    }

    ExitCode::Success
}
