use super::*;

#[derive(Subcommand)]
pub enum GlobalSkillCommands {
    Create(GlobalSkillCreateArgs),
    List,
    Inspect(GlobalSkillInspectArgs),
    Update(GlobalSkillUpdateArgs),
    Delete(GlobalSkillDeleteArgs),
    #[command(subcommand)]
    Registry(GlobalSkillRegistryCommands),
}
#[derive(Subcommand)]
pub enum GlobalSkillRegistryCommands {
    RegistryList,
    Search(GlobalSkillRegistrySearchArgs),
    RegistryInspect(GlobalSkillRegistryInspectArgs),
    Pull(GlobalSkillRegistryPullArgs),
    PullGithub(GlobalSkillRegistryPullGithubArgs),
}
#[derive(Args)]
pub struct GlobalSkillRegistrySearchArgs {
    #[arg(long)]
    pub registry: Option<String>,
    pub query: Option<String>,
}
#[derive(Args)]
pub struct GlobalSkillRegistryInspectArgs {
    pub registry: String,
    pub skill_name: String,
}
#[derive(Args)]
pub struct GlobalSkillRegistryPullArgs {
    pub registry: String,
    pub skill_name: String,
    #[arg(long)]
    pub skill_id: Option<String>,
}
#[derive(Args)]
pub struct GlobalSkillRegistryPullGithubArgs {
    pub repo: String,
    #[arg(long)]
    pub skill: Option<String>,
}
#[derive(Args)]
pub struct GlobalSkillCreateArgs {
    pub skill_id: String,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub content: String,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
}
#[derive(Args)]
pub struct GlobalSkillInspectArgs {
    pub skill_id: String,
}
#[derive(Args)]
pub struct GlobalSkillUpdateArgs {
    pub skill_id: String,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub content: Option<String>,
    #[arg(long = "tag")]
    pub tags: Option<Vec<String>>,
}
#[derive(Args)]
pub struct GlobalSkillDeleteArgs {
    pub skill_id: String,
}
