use super::*;

pub fn handle_global_system_prompt(
    cmd: GlobalSystemPromptCommands,
    format: OutputFormat,
) -> ExitCode {
    handle_global(GlobalCommands::SystemPrompt(cmd), format)
}
pub fn handle_global_template(cmd: GlobalTemplateCommands, format: OutputFormat) -> ExitCode {
    handle_global(GlobalCommands::Template(cmd), format)
}
pub fn handle_global_notepad(cmd: GlobalNotepadCommands, format: OutputFormat) -> ExitCode {
    handle_global(GlobalCommands::Notepad(cmd), format)
}
