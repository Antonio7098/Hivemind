//! Hivemind CLI entrypoint.

use clap::Parser;
use hivemind::cli::commands::{
    AttemptCommands, Cli, Commands, EventCommands, FlowCommands, GraphCommands, MergeCommands,
    ProjectCommands, TaskCommands, VerifyCommands, WorktreeCommands,
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
        WorktreeCommands::Cleanup(args) => match registry.worktree_cleanup(&args.flow_id) {
            Ok(()) => {
                match format {
                    OutputFormat::Json => {
                        println!(
                            "{}",
                            serde_json::json!({"success": true, "flow_id": args.flow_id})
                        );
                    }
                    OutputFormat::Yaml => {
                        if let Ok(yaml) = serde_yaml::to_string(&serde_json::json!({
                            "success": true,
                            "flow_id": args.flow_id
                        })) {
                            print!("{yaml}");
                        }
                    }
                    OutputFormat::Table => {
                        println!("ok");
                    }
                }
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
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
        GraphCommands::AddDependency(args) => {
            match registry.add_graph_dependency(&args.graph_id, &args.from_task, &args.to_task) {
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
    }
}

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
        FlowCommands::Abort(args) => {
            match registry.abort_flow(&args.flow_id, args.reason.as_deref(), args.force) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Status(args) => match registry.get_flow(&args.flow_id) {
            Ok(flow) => match format {
                OutputFormat::Json => {
                    if let Ok(json) = serde_json::to_string_pretty(&flow) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Yaml => {
                    if let Ok(yaml) = serde_yaml::to_string(&flow) {
                        print!("{yaml}");
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
        Some(Commands::Verify(cmd)) => handle_verify(cmd, format),
        Some(Commands::Merge(cmd)) => handle_merge(cmd, format),
        Some(Commands::Attempt(cmd)) => handle_attempt(cmd, format),
        Some(Commands::Worktree(cmd)) => handle_worktree(cmd, format),
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
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(project) {
                print!("{yaml}");
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
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(projects) {
                print!("{yaml}");
            }
        }
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
    }
}

fn print_task(task: &Task, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(task) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(task) {
                print!("{yaml}");
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
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(tasks) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            if tasks.is_empty() {
                println!("No tasks found.");
                return;
            }
            println!("{:<36}  {:<8}  TITLE", "ID", "STATE");
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
        TaskCommands::Close(args) => {
            match registry.close_task(&args.task_id, args.reason.as_deref()) {
                Ok(task) => {
                    print_task(&task, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        TaskCommands::Retry(args) => match registry.retry_task(&args.task_id, args.reset_count) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        TaskCommands::Abort(args) => {
            match registry.abort_task(&args.task_id, args.reason.as_deref()) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

fn event_type_label(payload: &hivemind::core::events::EventPayload) -> &'static str {
    use hivemind::core::events::EventPayload;
    match payload {
        EventPayload::ProjectCreated { .. } => "project_created",
        EventPayload::ProjectUpdated { .. } => "project_updated",
        EventPayload::TaskCreated { .. } => "task_created",
        EventPayload::TaskUpdated { .. } => "task_updated",
        EventPayload::TaskClosed { .. } => "task_closed",
        EventPayload::RepositoryAttached { .. } => "repo_attached",
        EventPayload::RepositoryDetached { .. } => "repo_detached",
        EventPayload::TaskGraphCreated { .. } => "graph_created",
        EventPayload::TaskAddedToGraph { .. } => "graph_task_added",
        EventPayload::DependencyAdded { .. } => "graph_dependency_added",
        EventPayload::ScopeAssigned { .. } => "graph_scope_assigned",
        EventPayload::TaskFlowCreated { .. } => "flow_created",
        EventPayload::TaskFlowStarted { .. } => "flow_started",
        EventPayload::TaskFlowPaused { .. } => "flow_paused",
        EventPayload::TaskFlowResumed { .. } => "flow_resumed",
        EventPayload::TaskFlowCompleted { .. } => "flow_completed",
        EventPayload::TaskFlowAborted { .. } => "flow_aborted",
        EventPayload::TaskReady { .. } => "task_ready",
        EventPayload::TaskBlocked { .. } => "task_blocked",
        EventPayload::TaskExecutionStateChanged { .. } => "task_exec_state_changed",
        EventPayload::TaskRetryRequested { .. } => "task_retry_requested",
        EventPayload::TaskAborted { .. } => "task_aborted",
        EventPayload::HumanOverride { .. } => "human_override",
        EventPayload::MergePrepared { .. } => "merge_prepared",
        EventPayload::MergeApproved { .. } => "merge_approved",
        EventPayload::MergeCompleted { .. } => "merge_completed",
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

#[allow(clippy::too_many_lines)]
fn handle_events(cmd: EventCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        EventCommands::List(args) => {
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
            use hivemind::storage::event_store::EventFilter;

            let mut filter = EventFilter::all();
            filter.limit = Some(args.limit);

            if let Some(ref project) = args.project {
                match registry.get_project(project) {
                    Ok(p) => filter.project_id = Some(p.id),
                    Err(e) => return output_error(&e, format),
                }
            }
            if let Some(ref flow) = args.flow {
                match Uuid::parse_str(flow) {
                    Ok(id) => filter.flow_id = Some(id),
                    Err(_) => {
                        return output_error(
                            &hivemind::core::error::HivemindError::user(
                                "invalid_flow_id",
                                format!("'{flow}' is not a valid flow ID"),
                                "cli:events:stream",
                            ),
                            format,
                        );
                    }
                }
            }
            if let Some(ref task) = args.task {
                match Uuid::parse_str(task) {
                    Ok(id) => filter.task_id = Some(id),
                    Err(_) => {
                        return output_error(
                            &hivemind::core::error::HivemindError::user(
                                "invalid_task_id",
                                format!("'{task}' is not a valid task ID"),
                                "cli:events:stream",
                            ),
                            format,
                        );
                    }
                }
            }
            if let Some(ref graph) = args.graph {
                match Uuid::parse_str(graph) {
                    Ok(id) => filter.graph_id = Some(id),
                    Err(_) => {
                        return output_error(
                            &hivemind::core::error::HivemindError::user(
                                "invalid_graph_id",
                                format!("'{graph}' is not a valid graph ID"),
                                "cli:events:stream",
                            ),
                            format,
                        );
                    }
                }
            }

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
        MergeCommands::Execute(args) => match registry.merge_execute(&args.flow_id) {
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
        AttemptCommands::Inspect(args) => {
            let Ok(attempt_id) = Uuid::parse_str(&args.attempt_id) else {
                return output_error(
                    &hivemind::core::error::HivemindError::user(
                        "invalid_attempt_id",
                        format!("'{}' is not a valid attempt ID", args.attempt_id),
                        "cli:attempt:inspect",
                    ),
                    format,
                );
            };

            let state = match registry.state() {
                Ok(s) => s,
                Err(e) => return output_error(&e, format),
            };

            for flow in state.flows.values() {
                for exec in flow.task_executions.values() {
                    if exec.task_id == attempt_id {
                        let info = serde_json::json!({
                            "task_id": exec.task_id,
                            "state": format!("{:?}", exec.state),
                            "attempt_count": exec.attempt_count,
                            "blocked_reason": exec.blocked_reason,
                            "flow_id": flow.id,
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
                            }
                        }
                        return ExitCode::Success;
                    }
                }
            }

            output_error(
                &hivemind::core::error::HivemindError::user(
                    "attempt_not_found",
                    format!("Attempt '{}' not found", args.attempt_id),
                    "cli:attempt:inspect",
                ),
                format,
            )
        }
    }
}
