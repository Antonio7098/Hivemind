//! Task command handlers.

use crate::cli::commands::{
    TaskAbortArgs, TaskCloseArgs, TaskCommands, TaskCompleteArgs, TaskCreateArgs, TaskInspectArgs,
    TaskListArgs, TaskRetryArgs, TaskStartArgs, TaskUpdateArgs,
};
use crate::cli::handlers::common::{
    get_registry, parse_run_mode, parse_runtime_role, print_flow_id,
};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::flow::RetryMode;
use crate::core::registry::Registry;
use crate::core::scope::Scope;
use crate::core::state::{Task, TaskState};
use uuid::Uuid;

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

fn parse_scope_arg(scope: Option<&str>, format: OutputFormat) -> Result<Option<Scope>, ExitCode> {
    let Some(raw) = scope else {
        return Ok(None);
    };

    match serde_json::from_str::<Scope>(raw) {
        Ok(s) => Ok(Some(s)),
        Err(e) => Err(output_error(
            &crate::core::error::HivemindError::user(
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
    args: &crate::cli::commands::TaskRuntimeSetArgs,
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
) -> Result<String, crate::core::error::HivemindError> {
    if let Some(task_id) = legacy_task_id {
        let project = registry.get_project(project_or_task)?;
        let task = registry.get_task(task_id)?;
        if task.project_id != project.id {
            return Err(crate::core::error::HivemindError::user(
                "task_project_mismatch",
                format!("Task '{task_id}' does not belong to project '{project_or_task}'"),
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
        crate::cli::commands::TaskRetryMode::Clean => RetryMode::Clean,
        crate::cli::commands::TaskRetryMode::Continue => RetryMode::Continue,
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

pub fn handle_task(cmd: TaskCommands, format: OutputFormat) -> ExitCode {
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

pub fn handle_task_show(args: TaskInspectArgs, format: OutputFormat) -> ExitCode {
    handle_task(TaskCommands::Inspect(args), format)
}
