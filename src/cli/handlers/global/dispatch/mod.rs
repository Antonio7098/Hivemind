use super::*;

pub(super) mod notepad;
pub(super) mod skill;
pub(super) mod system_prompt;
pub(super) mod template;

pub fn handle_global(cmd: GlobalCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_governance_service(format) else {
        return ExitCode::Error;
    };

    match cmd {
        GlobalCommands::Skill(subcmd) => skill::handle_skill_commands(&service, subcmd, format),
        GlobalCommands::SystemPrompt(subcmd) => {
            system_prompt::handle_system_prompt_commands(&service, subcmd, format)
        }
        GlobalCommands::Template(subcmd) => {
            template::handle_template_commands(&service, subcmd, format)
        }
        GlobalCommands::Notepad(subcmd) => {
            notepad::handle_notepad_commands(&service, subcmd, format)
        }
    }
}
