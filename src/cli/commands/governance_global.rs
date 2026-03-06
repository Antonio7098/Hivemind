use super::*;

#[derive(Subcommand)]
pub enum ConstitutionCommands {
    Init(ConstitutionInitArgs),
    Show(ConstitutionShowArgs),
    Validate(ConstitutionValidateArgs),
    Check(ConstitutionCheckArgs),
    Update(ConstitutionUpdateArgs),
}

#[derive(Args)]
pub struct ConstitutionInitArgs {
    pub project: String,
    #[arg(long, conflicts_with = "from_file")]
    pub content: Option<String>,
    #[arg(long = "from-file", conflicts_with = "content")]
    pub from_file: Option<String>,
    #[arg(long, default_value_t = false)]
    pub confirm: bool,
    #[arg(long)]
    pub actor: Option<String>,
    #[arg(long)]
    pub intent: Option<String>,
}

#[derive(Args)]
pub struct ConstitutionShowArgs {
    pub project: String,
}

#[derive(Args)]
pub struct ConstitutionValidateArgs {
    pub project: String,
}

#[derive(Args)]
pub struct ConstitutionCheckArgs {
    #[arg(long)]
    pub project: String,
}

#[derive(Args)]
pub struct ConstitutionUpdateArgs {
    pub project: String,
    #[arg(long, conflicts_with = "from_file")]
    pub content: Option<String>,
    #[arg(long = "from-file", conflicts_with = "content")]
    pub from_file: Option<String>,
    #[arg(long, default_value_t = false)]
    pub confirm: bool,
    #[arg(long)]
    pub actor: Option<String>,
    #[arg(long)]
    pub intent: Option<String>,
}

#[derive(Subcommand)]
pub enum ProjectGovernanceCommands {
    Init(ProjectGovernanceInitArgs),
    Migrate(ProjectGovernanceMigrateArgs),
    Inspect(ProjectGovernanceInspectArgs),
    Diagnose(ProjectGovernanceDiagnoseArgs),
    Replay(ProjectGovernanceReplayArgs),
    #[command(subcommand)]
    Snapshot(ProjectGovernanceSnapshotCommands),
    #[command(subcommand)]
    Repair(ProjectGovernanceRepairCommands),
    #[command(subcommand)]
    Document(ProjectGovernanceDocumentCommands),
    #[command(subcommand)]
    Attachment(ProjectGovernanceAttachmentCommands),
    #[command(subcommand)]
    Notepad(ProjectGovernanceNotepadCommands),
}

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

#[derive(Subcommand)]
pub enum ProjectGovernanceDocumentCommands {
    Create(ProjectGovernanceDocumentCreateArgs),
    List(ProjectGovernanceDocumentListArgs),
    Inspect(ProjectGovernanceDocumentInspectArgs),
    Update(ProjectGovernanceDocumentUpdateArgs),
    Delete(ProjectGovernanceDocumentDeleteArgs),
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentCreateArgs {
    pub project: String,
    pub document_id: String,
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub owner: String,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentListArgs {
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentInspectArgs {
    pub project: String,
    pub document_id: String,
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentUpdateArgs {
    pub project: String,
    pub document_id: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long = "tag")]
    pub tags: Option<Vec<String>>,
    #[arg(long)]
    pub content: Option<String>,
}

#[derive(Args)]
pub struct ProjectGovernanceDocumentDeleteArgs {
    pub project: String,
    pub document_id: String,
}

#[derive(Subcommand)]
pub enum ProjectGovernanceAttachmentCommands {
    Include(ProjectGovernanceAttachmentSetArgs),
    Exclude(ProjectGovernanceAttachmentSetArgs),
}

#[derive(Args)]
pub struct ProjectGovernanceAttachmentSetArgs {
    pub project: String,
    pub task_id: String,
    pub document_id: String,
}

#[derive(Subcommand)]
pub enum ProjectGovernanceNotepadCommands {
    Create(ProjectGovernanceNotepadCreateArgs),
    Show(ProjectGovernanceNotepadShowArgs),
    Update(ProjectGovernanceNotepadUpdateArgs),
    Delete(ProjectGovernanceNotepadDeleteArgs),
}

#[derive(Args)]
pub struct ProjectGovernanceNotepadCreateArgs {
    pub project: String,
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct ProjectGovernanceNotepadShowArgs {
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceNotepadUpdateArgs {
    pub project: String,
    #[arg(long)]
    pub content: String,
}

#[derive(Args)]
pub struct ProjectGovernanceNotepadDeleteArgs {
    pub project: String,
}

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

#[derive(Args)]
pub struct ProjectGovernanceInitArgs {
    pub project: Option<String>,
    #[arg(long = "project")]
    pub project_flag: Option<String>,
}

#[derive(Args)]
pub struct ProjectGovernanceMigrateArgs {
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceInspectArgs {
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceDiagnoseArgs {
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceReplayArgs {
    pub project: String,
    #[arg(long)]
    pub verify: bool,
}

#[derive(Subcommand)]
pub enum ProjectGovernanceSnapshotCommands {
    Create(ProjectGovernanceSnapshotCreateArgs),
    List(ProjectGovernanceSnapshotListArgs),
    Restore(ProjectGovernanceSnapshotRestoreArgs),
}

#[derive(Args)]
pub struct ProjectGovernanceSnapshotCreateArgs {
    pub project: String,
    #[arg(long = "interval-minutes")]
    pub interval_minutes: Option<u64>,
}

#[derive(Args)]
pub struct ProjectGovernanceSnapshotListArgs {
    pub project: String,
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

#[derive(Args)]
pub struct ProjectGovernanceSnapshotRestoreArgs {
    pub project: String,
    pub snapshot_id: String,
    #[arg(long)]
    pub confirm: bool,
}

#[derive(Subcommand)]
pub enum ProjectGovernanceRepairCommands {
    Detect(ProjectGovernanceRepairDetectArgs),
    Preview(ProjectGovernanceRepairPreviewArgs),
    Apply(ProjectGovernanceRepairApplyArgs),
}

#[derive(Args)]
pub struct ProjectGovernanceRepairDetectArgs {
    pub project: String,
}

#[derive(Args)]
pub struct ProjectGovernanceRepairPreviewArgs {
    pub project: String,
    #[arg(long = "snapshot-id")]
    pub snapshot_id: Option<String>,
}

#[derive(Args)]
pub struct ProjectGovernanceRepairApplyArgs {
    pub project: String,
    #[arg(long = "snapshot-id")]
    pub snapshot_id: Option<String>,
    #[arg(long)]
    pub confirm: bool,
}
