use super::*;

pub fn handle_task(cmd: TaskCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_task_service(format) else {
        return ExitCode::Error;
    };

    match cmd {
        TaskCommands::Create(args) => handle_task_create(&service, &args, format),
        TaskCommands::List(args) => handle_task_list(&service, &args, format),
        TaskCommands::Inspect(args) => handle_task_inspect(&service, &args, format),
        TaskCommands::Update(args) => handle_task_update(&service, &args, format),
        TaskCommands::RuntimeSet(args) => handle_task_runtime_set(&service, &args, format),
        TaskCommands::Close(args) => handle_task_close(&service, &args, format),
        TaskCommands::Start(args) => handle_task_start(&service, &args, format),
        TaskCommands::Complete(args) => handle_task_complete(&service, &args, format),
        TaskCommands::Retry(args) => handle_task_retry(&service, &args, format),
        TaskCommands::Abort(args) => handle_task_abort(&service, &args, format),
        TaskCommands::SetRunMode(args) => {
            match service.task_set_run_mode(&args.task_id, parse_run_mode(args.mode)) {
                Ok(task) => {
                    print_task(&task, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        TaskCommands::Delete(args) => match service.delete_task(&args.task_id) {
            Ok(task_id) => {
                print_task_id(task_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}
