use super::*;

#[derive(Subcommand)]
pub enum GlobalSystemPromptCommands {
    Create(GlobalSystemPromptCreateArgs),
    List,
    Inspect(GlobalSystemPromptInspectArgs),
    Update(GlobalSystemPromptUpdateArgs),
    Delete(GlobalSystemPromptDeleteArgs),
}
#[derive(Args)]
pub struct GlobalSystemPromptCreateArgs {
    pub prompt_id: String,
    #[arg(long)]
    pub content: String,
}
#[derive(Args)]
pub struct GlobalSystemPromptInspectArgs {
    pub prompt_id: String,
}
#[derive(Args)]
pub struct GlobalSystemPromptUpdateArgs {
    pub prompt_id: String,
    #[arg(long)]
    pub content: String,
}
#[derive(Args)]
pub struct GlobalSystemPromptDeleteArgs {
    pub prompt_id: String,
}
#[derive(Subcommand)]
pub enum GlobalTemplateCommands {
    Create(GlobalTemplateCreateArgs),
    List,
    Inspect(GlobalTemplateInspectArgs),
    Update(GlobalTemplateUpdateArgs),
    Delete(GlobalTemplateDeleteArgs),
    Instantiate(GlobalTemplateInstantiateArgs),
}
#[derive(Args)]
pub struct GlobalTemplateCreateArgs {
    pub template_id: String,
    #[arg(long)]
    pub system_prompt_id: String,
    #[arg(long = "skill-id")]
    pub skill_ids: Vec<String>,
    #[arg(long = "document-id")]
    pub document_ids: Vec<String>,
    #[arg(long)]
    pub description: Option<String>,
}
#[derive(Args)]
pub struct GlobalTemplateInspectArgs {
    pub template_id: String,
}
#[derive(Args)]
pub struct GlobalTemplateUpdateArgs {
    pub template_id: String,
    #[arg(long)]
    pub system_prompt_id: Option<String>,
    #[arg(long = "skill-id")]
    pub skill_ids: Option<Vec<String>>,
    #[arg(long = "document-id")]
    pub document_ids: Option<Vec<String>>,
    #[arg(long)]
    pub description: Option<String>,
}
#[derive(Args)]
pub struct GlobalTemplateDeleteArgs {
    pub template_id: String,
}
#[derive(Args)]
pub struct GlobalTemplateInstantiateArgs {
    pub project: String,
    pub template_id: String,
}
#[derive(Subcommand)]
pub enum GlobalNotepadCommands {
    Create(GlobalNotepadCreateArgs),
    Show,
    Update(GlobalNotepadUpdateArgs),
    Delete,
}
#[derive(Args)]
pub struct GlobalNotepadCreateArgs {
    #[arg(long)]
    pub content: String,
}
#[derive(Args)]
pub struct GlobalNotepadUpdateArgs {
    #[arg(long)]
    pub content: String,
}
