use super::*;

pub(super) fn handle_task_create(
    service: &TaskService,
    args: &TaskCreateArgs,
    format: OutputFormat,
) -> ExitCode {
    let scope = match parse_scope_arg(args.scope.as_deref(), format) {
        Ok(s) => s,
        Err(code) => return code,
    };

    match service.create_task(
        &args.project,
        &args.title,
        args.description.as_deref(),
        args.checkpoints,
        scope,
    ) {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
pub(super) fn handle_task_list(
    service: &TaskService,
    args: &TaskListArgs,
    format: OutputFormat,
) -> ExitCode {
    let state_filter = args.state.as_ref().and_then(|s| parse_task_state(s));
    match service.list_tasks(&args.project, state_filter) {
        Ok(tasks) => {
            print_tasks(&tasks, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
pub(super) fn handle_task_inspect(
    service: &TaskService,
    args: &TaskInspectArgs,
    format: OutputFormat,
) -> ExitCode {
    match service.get_task(&args.task_id) {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
pub(super) fn handle_task_update(
    service: &TaskService,
    args: &TaskUpdateArgs,
    format: OutputFormat,
) -> ExitCode {
    match service.update_task(
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
pub(super) fn handle_task_runtime_set(
    service: &TaskService,
    args: &crate::cli::commands::TaskRuntimeSetArgs,
    format: OutputFormat,
) -> ExitCode {
    let role = parse_runtime_role(args.role);
    let result = if args.clear {
        service.task_runtime_clear_role(&args.task_id, role)
    } else {
        service.task_runtime_set_role(
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
pub(super) fn handle_task_close(
    service: &TaskService,
    args: &TaskCloseArgs,
    format: OutputFormat,
) -> ExitCode {
    match service.close_task(&args.task_id, args.reason.as_deref()) {
        Ok(task) => {
            print_task(&task, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
pub(super) fn handle_task_start(
    service: &TaskService,
    args: &TaskStartArgs,
    format: OutputFormat,
) -> ExitCode {
    let task_id = match service.resolve_task_id_with_legacy_project(
        &args.task_id,
        args.legacy_task_id.as_deref(),
        "cli:task:start",
    ) {
        Ok(task_id) => task_id,
        Err(e) => return output_error(&e, format),
    };

    match service.start_task_execution(&task_id) {
        Ok(attempt_id) => {
            print_attempt_id(attempt_id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
pub(super) fn handle_task_complete(
    service: &TaskService,
    args: &TaskCompleteArgs,
    format: OutputFormat,
) -> ExitCode {
    let task_id = match service.resolve_task_id_with_legacy_project(
        &args.task_id,
        args.legacy_task_id.as_deref(),
        "cli:task:complete",
    ) {
        Ok(task_id) => task_id,
        Err(e) => return output_error(&e, format),
    };

    if matches!(args.success, Some(false)) {
        return match service.close_task(&task_id, args.message.as_deref()) {
            Ok(task) => {
                print_task(&task, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        };
    }

    match service.complete_task_execution(&task_id) {
        Ok(flow) => {
            print_flow_id(flow.id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
pub(super) fn handle_task_retry(
    service: &TaskService,
    args: &TaskRetryArgs,
    format: OutputFormat,
) -> ExitCode {
    let task_id = match service.resolve_task_id_with_legacy_project(
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

    match service.retry_task(&task_id, args.reset_count, mode) {
        Ok(flow) => {
            print_flow_id(flow.id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
pub(super) fn handle_task_abort(
    service: &TaskService,
    args: &TaskAbortArgs,
    format: OutputFormat,
) -> ExitCode {
    match service.abort_task(&args.task_id, args.reason.as_deref()) {
        Ok(flow) => {
            print_flow_id(flow.id, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
pub fn handle_task_show(args: TaskInspectArgs, format: OutputFormat) -> ExitCode {
    handle_task(TaskCommands::Inspect(args), format)
}
