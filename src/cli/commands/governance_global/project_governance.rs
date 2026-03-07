use super::*;

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
