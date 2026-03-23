use super::*;

pub(super) fn handle_template_commands(
    service: &GovernanceService,
    cmd: GlobalTemplateCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        GlobalTemplateCommands::Create(args) => match service.global_template_create(
            &args.template_id,
            &args.system_prompt_id,
            &args.skill_ids,
            &args.document_ids,
            args.description.as_deref(),
        ) {
            Ok(result) => {
                print_structured(&result, format, "global template create result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalTemplateCommands::List => match service.global_template_list() {
            Ok(result) => {
                print_structured(&result, format, "global template list");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalTemplateCommands::Inspect(args) => {
            match service.global_template_inspect(&args.template_id) {
                Ok(result) => {
                    print_structured(&result, format, "global template inspect result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalTemplateCommands::Update(args) => match service.global_template_update(
            &args.template_id,
            args.system_prompt_id.as_deref(),
            args.skill_ids.as_deref(),
            args.document_ids.as_deref(),
            args.description.as_deref(),
        ) {
            Ok(result) => {
                print_structured(&result, format, "global template update result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalTemplateCommands::Delete(args) => {
            match service.global_template_delete(&args.template_id) {
                Ok(result) => {
                    print_structured(&result, format, "global template delete result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalTemplateCommands::Instantiate(args) => {
            match service.global_template_instantiate(&args.project, &args.template_id) {
                Ok(result) => {
                    print_structured(&result, format, "global template instantiate result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
