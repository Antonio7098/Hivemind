use super::*;

pub(super) fn handle_attachment_commands(
    service: &GovernanceService,
    cmd: ProjectGovernanceAttachmentCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        ProjectGovernanceAttachmentCommands::Include(args) => {
            match service.project_governance_attachment_set_document(
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
            }
        }
        ProjectGovernanceAttachmentCommands::Exclude(args) => {
            match service.project_governance_attachment_set_document(
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
            }
        }
    }
}
