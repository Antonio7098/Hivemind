use super::*;

pub(super) fn handle_notepad_commands(
    service: &GovernanceService,
    cmd: ProjectGovernanceNotepadCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        ProjectGovernanceNotepadCommands::Create(args) => {
            match service.project_governance_notepad_create(&args.project, &args.content) {
                Ok(result) => {
                    print_structured(&result, format, "project notepad create result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceNotepadCommands::Show(args) => {
            match service.project_governance_notepad_show(&args.project) {
                Ok(result) => {
                    print_structured(&result, format, "project notepad show result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceNotepadCommands::Update(args) => {
            match service.project_governance_notepad_update(&args.project, &args.content) {
                Ok(result) => {
                    print_structured(&result, format, "project notepad update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceNotepadCommands::Delete(args) => {
            match service.project_governance_notepad_delete(&args.project) {
                Ok(result) => {
                    print_structured(&result, format, "project notepad delete result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
