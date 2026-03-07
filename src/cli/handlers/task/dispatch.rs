use super::*;

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
