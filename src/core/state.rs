//! State derived from events.
//!
//! All state in Hivemind is derived by replaying events. This ensures
//! determinism, idempotency, and complete observability.
use super::events::{Event, EventPayload, RuntimeRole};
use super::flow::{FlowState, RetryMode, RunMode, TaskExecState, TaskExecution, TaskFlow};
use super::graph::{GraphState, TaskGraph};
use super::scope::{RepoAccessMode, Scope};
use super::verification::CheckResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

mod app;
mod apply;
mod catalog;
mod runtime;

const fn default_max_parallel_tasks() -> u16 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptCheckpointState {
    Declared,
    Active,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptCheckpoint {
    pub checkpoint_id: String,
    pub order: u32,
    pub total: u32,
    pub state: AttemptCheckpointState,
    #[serde(default)]
    pub commit_hash: Option<String>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptState {
    pub id: Uuid,
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub attempt_number: u32,
    pub started_at: DateTime<Utc>,
    pub baseline_id: Option<Uuid>,
    pub diff_id: Option<Uuid>,
    #[serde(default)]
    pub check_results: Vec<CheckResult>,
    #[serde(default)]
    pub checkpoints: Vec<AttemptCheckpoint>,
    #[serde(default)]
    pub all_checkpoints_completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRuntimeConfig {
    pub adapter_name: String,
    pub binary_path: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub timeout_ms: u64,
    #[serde(default = "default_max_parallel_tasks")]
    pub max_parallel_tasks: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRuntimeConfig {
    pub adapter_name: String,
    pub binary_path: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeRoleDefaults {
    #[serde(default)]
    pub worker: Option<ProjectRuntimeConfig>,
    #[serde(default)]
    pub validator: Option<ProjectRuntimeConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceProjectStorage {
    pub project_id: Uuid,
    pub schema_version: String,
    pub projection_version: u32,
    pub root_path: String,
    pub initialized_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceArtifact {
    #[serde(default)]
    pub project_id: Option<Uuid>,
    pub scope: String,
    pub artifact_kind: String,
    pub artifact_key: String,
    pub path: String,
    #[serde(default)]
    pub revision: u64,
    pub schema_version: String,
    pub projection_version: u32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceAttachment {
    pub project_id: Uuid,
    pub task_id: Uuid,
    pub artifact_kind: String,
    pub artifact_key: String,
    pub attached: bool,
    pub schema_version: String,
    pub projection_version: u32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceMigration {
    #[serde(default)]
    pub project_id: Option<Uuid>,
    pub from_layout: String,
    pub to_layout: String,
    #[serde(default)]
    pub migrated_paths: Vec<String>,
    pub rollback_hint: String,
    pub schema_version: String,
    pub projection_version: u32,
    pub migrated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaskRuntimeRoleOverrides {
    #[serde(default)]
    pub worker: Option<TaskRuntimeConfig>,
    #[serde(default)]
    pub validator: Option<TaskRuntimeConfig>,
}

/// A project in the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub repositories: Vec<Repository>,
    #[serde(default)]
    pub runtime: Option<ProjectRuntimeConfig>,
    #[serde(default)]
    pub runtime_defaults: RuntimeRoleDefaults,
    #[serde(default)]
    pub constitution_digest: Option<String>,
    #[serde(default)]
    pub constitution_schema_version: Option<String>,
    #[serde(default)]
    pub constitution_version: Option<u32>,
    #[serde(default)]
    pub constitution_updated_at: Option<DateTime<Utc>>,
}

/// A repository attached to a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub access_mode: RepoAccessMode,
}

/// Task state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Open,
    Closed,
}

/// A task in the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub scope: Option<Scope>,
    #[serde(default)]
    pub runtime_override: Option<TaskRuntimeConfig>,
    #[serde(default)]
    pub runtime_overrides: TaskRuntimeRoleOverrides,
    #[serde(default)]
    pub run_mode: RunMode,
    pub state: TaskState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Merge workflow status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeStatus {
    Prepared,
    Approved,
    Completed,
}

/// Merge state for a flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeState {
    pub flow_id: Uuid,
    pub status: MergeStatus,
    pub target_branch: Option<String>,
    pub conflicts: Vec<String>,
    pub commits: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

/// The complete application state derived from events.
#[derive(Debug, Default, Clone)]
pub struct AppState {
    pub projects: HashMap<Uuid, Project>,
    pub governance_projects: HashMap<Uuid, GovernanceProjectStorage>,
    pub governance_artifacts: HashMap<String, GovernanceArtifact>,
    pub governance_attachments: HashMap<String, GovernanceAttachment>,
    pub governance_migrations: Vec<GovernanceMigration>,
    pub tasks: HashMap<Uuid, Task>,
    pub graphs: HashMap<Uuid, TaskGraph>,
    pub flows: HashMap<Uuid, TaskFlow>,
    pub global_runtime_defaults: RuntimeRoleDefaults,
    pub flow_runtime_defaults: HashMap<Uuid, RuntimeRoleDefaults>,
    pub merge_states: HashMap<Uuid, MergeState>,
    pub attempts: HashMap<Uuid, AttemptState>,
}

#[cfg(test)]
mod tests;
