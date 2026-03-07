use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionCompatibility {
    pub minimum_hivemind_version: String,
    pub governance_schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionPartition {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstitutionSeverity {
    Hard,
    Advisory,
    Informational,
}

impl ConstitutionSeverity {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Hard => "hard",
            Self::Advisory => "advisory",
            Self::Informational => "informational",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstitutionRule {
    ForbiddenDependency {
        id: String,
        from: String,
        to: String,
        severity: ConstitutionSeverity,
    },
    AllowedDependency {
        id: String,
        from: String,
        to: String,
        severity: ConstitutionSeverity,
    },
    CoverageRequirement {
        id: String,
        target: String,
        threshold: u8,
        severity: ConstitutionSeverity,
    },
}

impl ConstitutionRule {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::ForbiddenDependency { id, .. }
            | Self::AllowedDependency { id, .. }
            | Self::CoverageRequirement { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionArtifact {
    pub version: u32,
    pub schema_version: String,
    pub compatibility: ConstitutionCompatibility,
    #[serde(default)]
    pub partitions: Vec<ConstitutionPartition>,
    #[serde(default)]
    pub rules: Vec<ConstitutionRule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConstitutionValidationIssue {
    pub code: String,
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectConstitutionShowResult {
    pub project_id: Uuid,
    pub path: String,
    pub revision: u64,
    pub digest: String,
    pub schema_version: String,
    pub constitution_version: u32,
    pub compatibility: ConstitutionCompatibility,
    pub partitions: Vec<ConstitutionPartition>,
    pub rules: Vec<ConstitutionRule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectConstitutionMutationResult {
    pub project_id: Uuid,
    pub path: String,
    pub revision: u64,
    pub digest: String,
    #[serde(default)]
    pub previous_digest: Option<String>,
    pub schema_version: String,
    pub constitution_version: u32,
    pub actor: String,
    pub mutation_intent: String,
    pub confirmed: bool,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectConstitutionValidationResult {
    pub project_id: Uuid,
    pub path: String,
    pub digest: String,
    pub schema_version: String,
    pub constitution_version: u32,
    pub valid: bool,
    pub issues: Vec<ConstitutionValidationIssue>,
    pub validated_by: String,
    pub validated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConstitutionRuleViolation {
    pub rule_id: String,
    pub rule_type: String,
    pub severity: ConstitutionSeverity,
    pub message: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub remediation_hint: Option<String>,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectConstitutionCheckResult {
    pub project_id: Uuid,
    pub gate: String,
    pub path: String,
    pub digest: String,
    pub schema_version: String,
    pub constitution_version: u32,
    #[serde(default)]
    pub flow_id: Option<Uuid>,
    #[serde(default)]
    pub task_id: Option<Uuid>,
    #[serde(default)]
    pub attempt_id: Option<Uuid>,
    #[serde(default)]
    pub skipped: bool,
    #[serde(default)]
    pub skip_reason: Option<String>,
    #[serde(default)]
    pub violations: Vec<ConstitutionRuleViolation>,
    pub hard_violations: usize,
    pub advisory_violations: usize,
    pub informational_violations: usize,
    pub blocked: bool,
    pub checked_at: chrono::DateTime<Utc>,
}
