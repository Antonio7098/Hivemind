//! `TaskGraph` - Static, immutable DAG representing planned intent.
//!
//! A `TaskGraph` is created by the Planner and is immutable once execution begins.
//! It represents what should happen, not what has happened.

use super::scope::Scope;
use super::verification::CheckConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

mod error;
mod model;
mod operations;
mod traversal;

#[cfg(test)]
mod tests;

/// Retry policy for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Whether to escalate to human on final failure.
    pub escalate_on_failure: bool,
}

/// Success criteria for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuccessCriteria {
    /// Human-readable description of success.
    pub description: String,
    /// Automated checks to run (command patterns).
    pub checks: Vec<CheckConfig>,
}

/// A task node within a `TaskGraph`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphTask {
    /// Unique task ID.
    pub id: Uuid,
    /// Task title.
    pub title: String,
    /// Task description/objective.
    pub description: Option<String>,
    /// Success criteria.
    pub criteria: SuccessCriteria,
    /// Retry policy.
    pub retry_policy: RetryPolicy,
    /// Ordered checkpoint IDs for attempt execution.
    #[serde(default = "GraphTask::default_checkpoints")]
    pub checkpoints: Vec<String>,
    /// Required scope (optional at graph creation).
    pub scope: Option<Scope>,
}

/// State of a `TaskGraph`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphState {
    /// Graph is being built, can be modified.
    Draft,
    /// Graph is validated and ready for execution.
    Validated,
    /// Graph is locked and immutable (execution started).
    Locked,
}

/// A `TaskGraph` - static DAG of planned intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGraph {
    /// Unique graph ID.
    pub id: Uuid,
    /// Associated project ID.
    pub project_id: Uuid,
    /// Graph name.
    pub name: String,
    /// Graph description.
    pub description: Option<String>,
    /// Current state.
    pub state: GraphState,
    /// Task nodes.
    pub tasks: HashMap<Uuid, GraphTask>,
    /// Dependencies (`task_id` -> set of dependency `task_ids`).
    pub dependencies: HashMap<Uuid, HashSet<Uuid>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Errors that can occur during graph operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    /// Graph is locked and cannot be modified.
    GraphLocked,
    /// Task not found.
    TaskNotFound(Uuid),
    /// Cycle detected in dependencies.
    CycleDetected,
    /// Invalid state transition.
    InvalidStateTransition,
    /// Graph is empty.
    EmptyGraph,
}
