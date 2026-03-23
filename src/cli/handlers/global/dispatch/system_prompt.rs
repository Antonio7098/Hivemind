use super::*;

pub(super) fn handle_system_prompt_commands(
    service: &GovernanceService,
    cmd: GlobalSystemPromptCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        GlobalSystemPromptCommands::Create(args) => {
            match service.global_system_prompt_create(&args.prompt_id, &args.content) {
                Ok(result) => {
                    print_structured(&result, format, "global system prompt create result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalSystemPromptCommands::List => match service.global_system_prompt_list() {
            Ok(result) => {
                print_structured(&result, format, "global system prompt list");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalSystemPromptCommands::Inspect(args) => {
            match service.global_system_prompt_inspect(&args.prompt_id) {
                Ok(result) => {
                    print_structured(&result, format, "global system prompt inspect result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalSystemPromptCommands::Update(args) => {
            match service.global_system_prompt_update(&args.prompt_id, &args.content) {
                Ok(result) => {
                    print_structured(&result, format, "global system prompt update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalSystemPromptCommands::Delete(args) => {
            match service.global_system_prompt_delete(&args.prompt_id) {
                Ok(result) => {
                    print_structured(&result, format, "global system prompt delete result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
