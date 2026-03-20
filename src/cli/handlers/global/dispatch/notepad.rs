use super::*;

pub(super) fn handle_notepad_commands(
    service: &GovernanceService,
    cmd: GlobalNotepadCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        GlobalNotepadCommands::Create(args) => match service.global_notepad_create(&args.content) {
            Ok(result) => {
                print_structured(&result, format, "global notepad create result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalNotepadCommands::Show => match service.global_notepad_show() {
            Ok(result) => {
                print_structured(&result, format, "global notepad show result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalNotepadCommands::Update(args) => match service.global_notepad_update(&args.content) {
            Ok(result) => {
                print_structured(&result, format, "global notepad update result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalNotepadCommands::Delete => match service.global_notepad_delete() {
            Ok(result) => {
                print_structured(&result, format, "global notepad delete result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}
