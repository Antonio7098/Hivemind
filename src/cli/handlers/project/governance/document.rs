use super::*;

pub(super) fn handle_document_commands(
    service: &GovernanceService,
    cmd: ProjectGovernanceDocumentCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        ProjectGovernanceDocumentCommands::Create(args) => {
            match service.project_governance_document_create(
                &args.project,
                &args.document_id,
                &args.title,
                &args.owner,
                &args.tags,
                &args.content,
            ) {
                Ok(result) => {
                    print_structured(&result, format, "governance document create result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceDocumentCommands::List(args) => {
            match service.project_governance_document_list(&args.project) {
                Ok(result) => {
                    print_structured(&result, format, "governance document list");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceDocumentCommands::Inspect(args) => {
            match service.project_governance_document_inspect(&args.project, &args.document_id) {
                Ok(result) => {
                    print_structured(&result, format, "governance document inspect result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceDocumentCommands::Update(args) => {
            match service.project_governance_document_update(
                &args.project,
                &args.document_id,
                args.title.as_deref(),
                args.owner.as_deref(),
                args.tags.as_deref(),
                args.content.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "governance document update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceDocumentCommands::Delete(args) => {
            match service.project_governance_document_delete(&args.project, &args.document_id) {
                Ok(result) => {
                    print_structured(&result, format, "governance document delete result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
