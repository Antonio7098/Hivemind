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
