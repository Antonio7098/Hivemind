//! `TaskFlow` - Runtime instance of a `TaskGraph`.
//!
//! A `TaskFlow` binds a `TaskGraph` to execution state. Multiple `TaskFlows`
//! may exist for the same `TaskGraph`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

mod execution;
mod lifecycle;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowState {
    Created,
    Running,
    Paused,
    FrozenForMerge,
    Completed,
    Merged,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskExecState {
    Pending,
    Ready,
    Running,
    Verifying,
    Success,
    Retry,
    Failed,
    Escalated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RetryMode {
    #[default]
    Clean,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub task_id: Uuid,
    pub state: TaskExecState,
    pub attempt_count: u32,
    #[serde(default)]
    pub retry_mode: RetryMode,
    #[serde(default)]
    pub frozen_commit_sha: Option<String>,
    #[serde(default)]
    pub integrated_commit_sha: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFlow {
    pub id: Uuid,
    pub graph_id: Uuid,
    pub project_id: Uuid,
    #[serde(default)]
    pub base_revision: Option<String>,
    #[serde(default)]
    pub run_mode: RunMode,
    #[serde(default)]
    pub depends_on_flows: HashSet<Uuid>,
    pub state: FlowState,
    pub task_executions: HashMap<Uuid, TaskExecution>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError {
    InvalidTransition {
        from: TaskExecState,
        to: TaskExecState,
    },
    InvalidFlowTransition {
        from: FlowState,
        to: FlowState,
    },
    TaskNotFound(Uuid),
    TasksNotComplete,
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(f, "Invalid transition from {from:?} to {to:?}")
            }
            Self::InvalidFlowTransition { from, to } => {
                write!(f, "Invalid flow transition from {from:?} to {to:?}")
            }
            Self::TaskNotFound(id) => write!(f, "Task not found: {id}"),
            Self::TasksNotComplete => write!(f, "Cannot complete flow with incomplete tasks"),
        }
    }
}

impl std::error::Error for FlowError {}
