use super::*;

pub(super) fn handle_skill_commands(
    service: &GovernanceService,
    cmd: GlobalSkillCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        GlobalSkillCommands::Create(args) => {
            match service.global_skill_create(&args.skill_id, &args.name, &args.tags, &args.content)
            {
                Ok(result) => {
                    print_structured(&result, format, "global skill create result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalSkillCommands::List => match service.global_skill_list() {
            Ok(result) => {
                print_structured(&result, format, "global skill list");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalSkillCommands::Inspect(args) => match service.global_skill_inspect(&args.skill_id) {
            Ok(result) => {
                print_structured(&result, format, "global skill inspect result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalSkillCommands::Update(args) => match service.global_skill_update(
            &args.skill_id,
            args.name.as_deref(),
            args.tags.as_deref(),
            args.content.as_deref(),
        ) {
            Ok(result) => {
                print_structured(&result, format, "global skill update result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalSkillCommands::Delete(args) => match service.global_skill_delete(&args.skill_id) {
            Ok(result) => {
                print_structured(&result, format, "global skill delete result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GlobalSkillCommands::Registry(cmd) => handle_skill_registry(cmd, service, format),
    }
}
