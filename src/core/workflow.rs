//! `Workflow` - Workflow-native execution definitions and runs.
//!
//! This module introduces the workflow domain alongside legacy `TaskFlow`.
//! Workflows are event-sourced, replayable execution definitions that can
//! evolve into the primary orchestration model over time.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// Supported workflow step kinds for the workflow engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepKind {
    Task,
    Workflow,
    Conditional,
    Wait,
    Join,
}

/// Lifecycle state of a workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowRunState {
    Created,
    Running,
    Paused,
    Completed,
    Aborted,
}

impl WorkflowRunState {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created | Self::Paused, Self::Running)
                | (Self::Created | Self::Running | Self::Paused, Self::Aborted)
                | (Self::Running, Self::Paused | Self::Completed)
        )
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Aborted)
    }
}

/// Lifecycle state of an individual workflow step run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStepState {
    Pending,
    Ready,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Skipped,
    Aborted,
}

impl WorkflowStepState {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending | Self::Waiting | Self::Failed, Self::Ready)
                | (Self::Pending | Self::Ready, Self::Skipped)
                | (
                    Self::Pending | Self::Ready | Self::Running | Self::Waiting | Self::Failed,
                    Self::Aborted,
                )
                | (Self::Ready, Self::Running)
                | (
                    Self::Running,
                    Self::Waiting | Self::Succeeded | Self::Failed
                )
                | (Self::Waiting, Self::Failed)
        )
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Skipped | Self::Aborted
        )
    }
}

/// Static step definition within a workflow definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepDefinition {
    pub id: Uuid,
    pub name: String,
    pub kind: WorkflowStepKind,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<Uuid>,
}

impl WorkflowStepDefinition {
    #[must_use]
    pub fn new(name: impl Into<String>, kind: WorkflowStepKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            description: None,
            depends_on: Vec::new(),
        }
    }
}

/// Immutable workflow definition registered under a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub steps: BTreeMap<Uuid, WorkflowStepDefinition>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowDefinition {
    #[must_use]
    pub fn new(project_id: Uuid, name: impl Into<String>, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            description,
            steps: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn add_step(&mut self, step: WorkflowStepDefinition) {
        self.steps.insert(step.id, step);
        self.updated_at = Utc::now();
    }

    pub fn update_metadata(&mut self, name: Option<String>, description: Option<Option<String>>) {
        if let Some(name) = name {
            self.name = name;
        }
        if let Some(description) = description {
            self.description = description;
        }
        self.updated_at = Utc::now();
    }

    #[must_use]
    pub fn has_step_named(&self, name: &str) -> bool {
        self.steps.values().any(|step| step.name == name)
    }

    #[must_use]
    pub fn root_step_ids(&self) -> Vec<Uuid> {
        self.steps
            .values()
            .filter(|step| step.depends_on.is_empty())
            .map(|step| step.id)
            .collect()
    }
}

/// Runtime state of a single step within a workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepRun {
    pub id: Uuid,
    pub step_id: Uuid,
    pub state: WorkflowStepState,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Runtime execution record for a workflow definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub project_id: Uuid,
    pub root_workflow_run_id: Uuid,
    #[serde(default)]
    pub parent_workflow_run_id: Option<Uuid>,
    #[serde(default)]
    pub parent_step_id: Option<Uuid>,
    pub state: WorkflowRunState,
    #[serde(default)]
    pub step_runs: BTreeMap<Uuid, WorkflowStepRun>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowRun {
    #[must_use]
    pub fn new_root(definition: &WorkflowDefinition) -> Self {
        let now = Utc::now();
        let run_id = Uuid::new_v4();
        let step_runs = definition
            .steps
            .keys()
            .map(|step_id| {
                (
                    *step_id,
                    WorkflowStepRun {
                        id: Uuid::new_v4(),
                        step_id: *step_id,
                        state: WorkflowStepState::Pending,
                        updated_at: now,
                        reason: None,
                    },
                )
            })
            .collect();

        Self {
            id: run_id,
            workflow_id: definition.id,
            project_id: definition.project_id,
            root_workflow_run_id: run_id,
            parent_workflow_run_id: None,
            parent_step_id: None,
            state: WorkflowRunState::Created,
            step_runs,
            created_at: now,
            started_at: None,
            completed_at: None,
            updated_at: now,
        }
    }

    #[must_use]
    pub fn can_complete(&self) -> bool {
        self.step_runs
            .values()
            .all(|step_run| step_run.state.is_terminal())
    }
}

/// Workflow domain error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowError {
    InvalidRunTransition {
        from: WorkflowRunState,
        to: WorkflowRunState,
    },
    InvalidStepTransition {
        from: WorkflowStepState,
        to: WorkflowStepState,
    },
    StepNotFound(Uuid),
}

impl std::fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRunTransition { from, to } => {
                write!(f, "Invalid workflow run transition from {from:?} to {to:?}")
            }
            Self::InvalidStepTransition { from, to } => {
                write!(
                    f,
                    "Invalid workflow step transition from {from:?} to {to:?}"
                )
            }
            Self::StepNotFound(id) => write!(f, "Workflow step not found: {id}"),
        }
    }
}

impl std::error::Error for WorkflowError {}
