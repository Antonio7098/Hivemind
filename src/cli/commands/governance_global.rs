use super::*;
mod constitution;
pub use constitution::*;
mod project_governance;
pub use project_governance::*;
mod global_skill;
pub use global_skill::*;
mod global_artifacts;
pub use global_artifacts::*;

#[derive(Subcommand)]
pub enum GlobalCommands {
    #[command(subcommand)]
    Skill(GlobalSkillCommands),
    #[command(subcommand)]
    SystemPrompt(GlobalSystemPromptCommands),
    #[command(subcommand)]
    Template(GlobalTemplateCommands),
    #[command(subcommand)]
    Notepad(GlobalNotepadCommands),
}
