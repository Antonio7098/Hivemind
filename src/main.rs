//! Hivemind CLI entrypoint.

use clap::error::ErrorKind;
use clap::Parser;
use hivemind::cli::commands::{
    AttemptCommands, AttemptInspectArgs, Cli, Commands, EventCommands, FlowCommands, GraphCommands,
    MergeCommands, ProjectCommands, ServeArgs, TaskAbortArgs, TaskCloseArgs, TaskCommands,
    TaskCompleteArgs, TaskCreateArgs, TaskInspectArgs, TaskListArgs, TaskRetryArgs, TaskStartArgs,
    TaskUpdateArgs, VerifyCommands, WorktreeCommands,
};
use hivemind::cli::output::{output, output_error, OutputFormat};
use hivemind::core::error::ExitCode;
use hivemind::core::registry::Registry;
use hivemind::core::scope::RepoAccessMode;
use hivemind::core::scope::Scope;
use hivemind::core::state::{AttemptState, Project, Task, TaskState};
use std::ffi::OsString;
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
        FlowCommands::Tick(args) => match registry.tick_flow(&args.flow_id, args.interactive) {
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
            output_version(format);
            ExitCode::Success
        }
        Some(Commands::Serve(args)) => handle_serve(args, format),
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
        ProjectCommands::RuntimeSet(args) => match registry.project_runtime_set(
            &args.project,
            &args.adapter,
            &args.binary_path,
            args.model,
            &args.args,
            &args.env,
            args.timeout_ms,
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
        _ => {
            if let Err(err) = output(tasks, format) {
                eprintln!("Failed to render tasks: {err}");
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

fn handle_task_close(registry: &Registry, args: &TaskCloseArgs, format: OutputFormat) -> ExitCode {
    match registry.close_task(&args.task_id, args.reason.as_deref()) {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_start(registry: &Registry, args: &TaskStartArgs, format: OutputFormat) -> ExitCode {
    match registry.start_task_execution(&args.task_id) {
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
    match registry.complete_task_execution(&args.task_id) {
        Ok(flow) => {
            print_flow_id(flow.id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

fn handle_task_retry(registry: &Registry, args: &TaskRetryArgs, format: OutputFormat) -> ExitCode {
    match registry.retry_task(&args.task_id, args.reset_count) {
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
        TaskCommands::Close(args) => handle_task_close(&registry, &args, format),
        TaskCommands::Start(args) => handle_task_start(&registry, &args, format),
        TaskCommands::Complete(args) => handle_task_complete(&registry, &args, format),
        TaskCommands::Retry(args) => handle_task_retry(&registry, &args, format),
        TaskCommands::Abort(args) => handle_task_abort(&registry, &args, format),
    }
}

fn event_type_label(payload: &hivemind::core::events::EventPayload) -> &'static str {
    use hivemind::core::events::EventPayload;
    match payload {
        EventPayload::ErrorOccurred { .. } => "error_occurred",
        EventPayload::ProjectCreated { .. } => "project_created",
        EventPayload::ProjectUpdated { .. } => "project_updated",
        EventPayload::ProjectRuntimeConfigured { .. } => "project_runtime_configured",
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
        EventPayload::AttemptStarted { .. } => "attempt_started",
        EventPayload::BaselineCaptured { .. } => "baseline_captured",
        EventPayload::FileModified { .. } => "file_modified",
        EventPayload::DiffComputed { .. } => "diff_computed",
        EventPayload::CheckpointCommitCreated { .. } => "checkpoint_commit_created",
        EventPayload::ScopeValidated { .. } => "scope_validated",
        EventPayload::ScopeViolationDetected { .. } => "scope_violation_detected",
        EventPayload::TaskRetryRequested { .. } => "task_retry_requested",
        EventPayload::TaskAborted { .. } => "task_aborted",
        EventPayload::HumanOverride { .. } => "human_override",
        EventPayload::MergePrepared { .. } => "merge_prepared",
        EventPayload::MergeApproved { .. } => "merge_approved",
        EventPayload::MergeCompleted { .. } => "merge_completed",
        EventPayload::RuntimeStarted { .. } => "runtime_started",
        EventPayload::RuntimeOutputChunk { .. } => "runtime_output_chunk",
        EventPayload::RuntimeInputProvided { .. } => "runtime_input_provided",
        EventPayload::RuntimeInterrupted { .. } => "runtime_interrupted",
        EventPayload::RuntimeExited { .. } => "runtime_exited",
        EventPayload::RuntimeTerminated { .. } => "runtime_terminated",
        EventPayload::RuntimeFilesystemObserved { .. } => "runtime_filesystem_observed",
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
        AttemptCommands::Inspect(args) => handle_attempt_inspect(&registry, &args, format),
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
        Ok(attempt) => print_attempt_inspect_attempt(registry, &attempt, args.diff, format),
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

    let info = serde_json::json!({
        "attempt_id": attempt.id,
        "task_id": attempt.task_id,
        "flow_id": attempt.flow_id,
        "attempt_number": attempt.attempt_number,
        "started_at": attempt.started_at,
        "baseline_id": attempt.baseline_id,
        "diff_id": attempt.diff_id,
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
    };

    for ev in events {
        match &ev.payload {
            EventPayload::RuntimeStarted {
                adapter_name,
                task_id,
                attempt_id: _,
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
