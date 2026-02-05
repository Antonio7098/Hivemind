//! Hivemind CLI entrypoint.

use clap::Parser;
use hivemind::cli::commands::{
    Cli, Commands, EventCommands, FlowCommands, GraphCommands, ProjectCommands, TaskCommands,
};
use hivemind::cli::output::{output_error, OutputFormat};
use hivemind::core::error::ExitCode;
use hivemind::core::registry::Registry;
use hivemind::core::scope::RepoAccessMode;
use hivemind::core::scope::Scope;
use hivemind::core::state::{Project, Task, TaskState};
use std::process;
use uuid::Uuid;

fn main() {
    let cli = Cli::parse();
    let exit_code = run(cli);
    process::exit(i32::from(exit_code));
}

fn print_graph_id(graph_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"graph_id": graph_id}));
        }
        OutputFormat::Table => {
            println!("Graph ID: {}", graph_id);
        }
    }
}

fn print_flow_id(flow_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"flow_id": flow_id}));
        }
        OutputFormat::Table => {
            println!("Flow ID: {}", flow_id);
        }
    }
}

fn handle_graph(cmd: GraphCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        GraphCommands::Create(args) => {
            let mut task_ids = Vec::new();
            for raw in &args.from_tasks {
                let id = match Uuid::parse_str(raw) {
                    Ok(id) => id,
                    Err(_) => {
                        return output_error(
                            &hivemind::core::error::HivemindError::user(
                                "invalid_task_id",
                                format!("'{}' is not a valid task ID", raw),
                                "cli:graph:create",
                            ),
                            format,
                        );
                    }
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
        GraphCommands::AddDependency(args) => match registry.add_graph_dependency(
            &args.graph_id,
            &args.from_task,
            &args.to_task,
        ) {
            Ok(graph) => {
                print_graph_id(graph.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GraphCommands::Validate(args) => match registry.validate_graph(&args.graph_id) {
            Ok(result) => match format {
                OutputFormat::Json => {
                    if let Ok(json) = serde_json::to_string_pretty(&result) {
                        println!("{json}");
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
    }
}

fn handle_flow(cmd: FlowCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        FlowCommands::Create(args) => match registry.create_flow(&args.graph_id, args.name.as_deref()) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
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
        FlowCommands::Abort(args) => match registry.abort_flow(
            &args.flow_id,
            args.reason.as_deref(),
            args.force,
        ) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Status(args) => match registry.get_flow(&args.flow_id) {
            Ok(flow) => match format {
                OutputFormat::Json => {
                    if let Ok(json) = serde_json::to_string_pretty(&flow) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    println!("ID:      {}", flow.id);
                    println!("Graph:   {}", flow.graph_id);
                    println!("Project: {}", flow.project_id);
                    println!("State:   {:?}", flow.state);
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
    }
}

fn run(cli: Cli) -> ExitCode {
    let format = cli.format;

    match cli.command {
        Some(Commands::Version) => {
            println!("hivemind {}", env!("CARGO_PKG_VERSION"));
            ExitCode::Success
        }
        Some(Commands::Project(cmd)) => handle_project(cmd, format),
        Some(Commands::Task(cmd)) => handle_task(cmd, format),
        Some(Commands::Graph(cmd)) => handle_graph(cmd, format),
        Some(Commands::Flow(cmd)) => handle_flow(cmd, format),
        Some(Commands::Events(cmd)) => handle_events(cmd, format),
        None => {
            println!("hivemind {}", env!("CARGO_PKG_VERSION"));
            println!("Use --help for usage information.");
            ExitCode::Success
        }
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
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(project) {
                println!("{json}");
            }
        }
        OutputFormat::Table => {
            println!("ID:          {}", project.id);
            println!("Name:        {}", project.name);
            if let Some(desc) = &project.description {
                println!("Description: {desc}");
            }
            println!("Created:     {}", project.created_at);
            if !project.repositories.is_empty() {
                println!("Repositories:");
                for repo in &project.repositories {
                    println!("  - {} ({})", repo.name, repo.path);
                }
            }
        }
    }
}

fn print_projects(projects: &[Project], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(projects) {
                println!("{json}");
            }
        }
        OutputFormat::Table => {
            if projects.is_empty() {
                println!("No projects found.");
                return;
            }
            println!("{:<36}  {:<20}  {}", "ID", "NAME", "DESCRIPTION");
            println!("{}", "-".repeat(80));
            for p in projects {
                let desc = p.description.as_deref().unwrap_or("");
                println!("{:<36}  {:<20}  {}", p.id, p.name, desc);
            }
        }
    }
}

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

            match registry.attach_repo(
                &args.project,
                &args.path,
                args.name.as_deref(),
                access_mode,
            ) {
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
    }
}

fn print_task(task: &Task, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(task) {
                println!("{json}");
            }
        }
        OutputFormat::Table => {
            println!("ID:          {}", task.id);
            println!("Project:     {}", task.project_id);
            println!("Title:       {}", task.title);
            if let Some(desc) = &task.description {
                println!("Description: {desc}");
            }
            println!("State:       {:?}", task.state);
            println!("Created:     {}", task.created_at);
        }
    }
}

fn print_tasks(tasks: &[Task], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(tasks) {
                println!("{json}");
            }
        }
        OutputFormat::Table => {
            if tasks.is_empty() {
                println!("No tasks found.");
                return;
            }
            println!("{:<36}  {:<8}  {}", "ID", "STATE", "TITLE");
            println!("{}", "-".repeat(80));
            for t in tasks {
                let state = match t.state {
                    TaskState::Open => "open",
                    TaskState::Closed => "closed",
                };
                println!("{:<36}  {:<8}  {}", t.id, state, t.title);
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

fn handle_task(cmd: TaskCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        TaskCommands::Create(args) => {
            let scope: Option<Scope> = match args.scope.as_deref() {
                None => None,
                Some(raw) => match serde_json::from_str::<Scope>(raw) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        return output_error(
                            &hivemind::core::error::HivemindError::user(
                                "invalid_scope",
                                format!("Invalid scope definition: {e}"),
                                "cli:task:create",
                            ),
                            format,
                        );
                    }
                },
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
        TaskCommands::List(args) => {
            let state_filter = args.state.as_ref().and_then(|s| parse_task_state(s));
            match registry.list_tasks(&args.project, state_filter) {
                Ok(tasks) => {
                    print_tasks(&tasks, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        TaskCommands::Inspect(args) => match registry.get_task(&args.task_id) {
            Ok(task) => {
                print_task(&task, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        TaskCommands::Update(args) => {
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
        TaskCommands::Close(args) => match registry.close_task(&args.task_id, args.reason.as_deref()) {
            Ok(task) => {
                print_task(&task, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        TaskCommands::Retry(args) => match registry.retry_task(&args.task_id, args.reset_count) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        TaskCommands::Abort(args) => match registry.abort_task(&args.task_id, args.reason.as_deref()) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}

fn handle_events(cmd: EventCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        EventCommands::List(args) => {
            // Basic implementation: only project filter + limit
            let filter_project_id = if let Some(project) = args.project.as_deref() {
                match registry.get_project(project) {
                    Ok(p) => Some(p.id),
                    Err(e) => return output_error(&e, format),
                }
            } else {
                None
            };

            let events = match registry.list_events(filter_project_id, args.limit) {
                Ok(evs) => evs,
                Err(e) => return output_error(&e, format),
            };

            match format {
                OutputFormat::Json => {
                    let response = hivemind::cli::output::CliResponse::success(events);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                }
                OutputFormat::Table => {
                    if events.is_empty() {
                        println!("No events found.");
                        return ExitCode::Success;
                    }

                    println!("{:<6}  {:<20}  {:<22}  {}", "SEQ", "TYPE", "TIMESTAMP", "PROJECT");
                    println!("{}", "-".repeat(90));
                    for ev in events {
                        let seq = ev.metadata.sequence.unwrap_or(0);
                        let typ = match &ev.payload {
                            hivemind::core::events::EventPayload::ProjectCreated { .. } => "project_created",
                            hivemind::core::events::EventPayload::ProjectUpdated { .. } => "project_updated",
                            hivemind::core::events::EventPayload::TaskCreated { .. } => "task_created",
                            hivemind::core::events::EventPayload::TaskUpdated { .. } => "task_updated",
                            hivemind::core::events::EventPayload::TaskClosed { .. } => "task_closed",
                            hivemind::core::events::EventPayload::RepositoryAttached { .. } => "repo_attached",
                            hivemind::core::events::EventPayload::RepositoryDetached { .. } => "repo_detached",
                            hivemind::core::events::EventPayload::TaskGraphCreated { .. } => "graph_created",
                            hivemind::core::events::EventPayload::TaskAddedToGraph { .. } => "graph_task_added",
                            hivemind::core::events::EventPayload::DependencyAdded { .. } => "graph_dependency_added",
                            hivemind::core::events::EventPayload::ScopeAssigned { .. } => "graph_scope_assigned",
                            hivemind::core::events::EventPayload::TaskFlowCreated { .. } => "flow_created",
                            hivemind::core::events::EventPayload::TaskFlowStarted { .. } => "flow_started",
                            hivemind::core::events::EventPayload::TaskFlowPaused { .. } => "flow_paused",
                            hivemind::core::events::EventPayload::TaskFlowResumed { .. } => "flow_resumed",
                            hivemind::core::events::EventPayload::TaskFlowCompleted { .. } => "flow_completed",
                            hivemind::core::events::EventPayload::TaskFlowAborted { .. } => "flow_aborted",
                            hivemind::core::events::EventPayload::TaskReady { .. } => "task_ready",
                            hivemind::core::events::EventPayload::TaskBlocked { .. } => "task_blocked",
                            hivemind::core::events::EventPayload::TaskExecutionStateChanged { .. } => {
                                "task_exec_state_changed"
                            }
                            hivemind::core::events::EventPayload::TaskRetryRequested { .. } => {
                                "task_retry_requested"
                            }
                            hivemind::core::events::EventPayload::TaskAborted { .. } => "task_aborted",
                        };
                        let ts = ev.metadata.timestamp.to_rfc3339();
                        let proj = ev
                            .metadata
                            .correlation
                            .project_id
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        println!("{:<6}  {:<20}  {:<22}  {}", seq, typ, ts, proj);
                    }
                }
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
                    let response = hivemind::cli::output::CliResponse::success(event);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
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
    }
}
