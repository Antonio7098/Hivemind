use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSkillSummary {
    pub skill_id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub updated_at: chrono::DateTime<Utc>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSkillInspectResult {
    pub summary: GlobalSkillSummary,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSystemPromptSummary {
    pub prompt_id: String,
    pub updated_at: chrono::DateTime<Utc>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSystemPromptInspectResult {
    pub summary: GlobalSystemPromptSummary,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalTemplateSummary {
    pub template_id: String,
    pub system_prompt_id: String,
    pub skill_ids: Vec<String>,
    pub document_ids: Vec<String>,
    pub updated_at: chrono::DateTime<Utc>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalTemplateInspectResult {
    pub summary: GlobalTemplateSummary,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateInstantiationResult {
    pub project_id: Uuid,
    pub template_id: String,
    pub system_prompt_id: String,
    pub skill_ids: Vec<String>,
    pub document_ids: Vec<String>,
    pub schema_version: String,
    pub projection_version: u32,
    pub instantiated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GlobalSkillArtifact {
    pub skill_id: String,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub content: String,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GlobalSystemPromptArtifact {
    pub prompt_id: String,
    pub content: String,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GlobalTemplateArtifact {
    pub template_id: String,
    pub system_prompt_id: String,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub document_ids: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub updated_at: chrono::DateTime<Utc>,
}
