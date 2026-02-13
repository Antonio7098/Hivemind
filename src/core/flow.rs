//! `TaskFlow` - Runtime instance of a `TaskGraph`.
//!
//! A `TaskFlow` binds a `TaskGraph` to execution state. Multiple `TaskFlows`
//! may exist for the same `TaskGraph`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// `TaskFlow` lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowState {
    /// Flow created but not started.
    Created,
    /// Flow is actively executing.
    Running,
    /// Flow is paused (can be resumed).
    Paused,
    /// Flow is frozen for merge preparation/execution.
    FrozenForMerge,
    /// Flow completed successfully.
    Completed,
    /// Flow was merged into the target branch.
    Merged,
    /// Flow was aborted.
    Aborted,
}

/// Task execution state within a flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskExecState {
    /// Task not yet eligible for execution.
    Pending,
    /// Task is ready (dependencies satisfied).
    Ready,
    /// Task is currently executing.
    Running,
    /// Task is being verified.
    Verifying,
    /// Task completed successfully.
    Success,
    /// Task is scheduled for retry.
    Retry,
    /// Task failed permanently.
    Failed,
    /// Task escalated to human.
    Escalated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RetryMode {
    #[default]
    Clean,
    Continue,
}

/// Execution state for a single task within a flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    /// Task ID (from `TaskGraph`).
    pub task_id: Uuid,
    /// Current execution state.
    pub state: TaskExecState,
    /// Number of attempts made.
    pub attempt_count: u32,
    #[serde(default)]
    pub retry_mode: RetryMode,
    #[serde(default)]
    pub frozen_commit_sha: Option<String>,
    #[serde(default)]
    pub integrated_commit_sha: Option<String>,
    /// Last state change timestamp.
    pub updated_at: DateTime<Utc>,
    /// Blocking reason (if blocked).
    pub blocked_reason: Option<String>,
}

impl TaskExecution {
    /// Creates a new task execution in Pending state.
    #[must_use]
    pub fn new(task_id: Uuid) -> Self {
        Self {
            task_id,
            state: TaskExecState::Pending,
            attempt_count: 0,
            retry_mode: RetryMode::default(),
            frozen_commit_sha: None,
            integrated_commit_sha: None,
            updated_at: Utc::now(),
            blocked_reason: None,
        }
    }

    /// Transitions to a new state.
    pub fn transition(&mut self, new_state: TaskExecState) -> Result<(), FlowError> {
        if !self.can_transition_to(new_state) {
            return Err(FlowError::InvalidTransition {
                from: self.state,
                to: new_state,
            });
        }

        self.state = new_state;
        self.updated_at = Utc::now();

        if new_state == TaskExecState::Running {
            self.attempt_count += 1;
        }

        Ok(())
    }

    /// Checks if a transition is valid.
    #[must_use]
    pub fn can_transition_to(&self, new_state: TaskExecState) -> bool {
        use TaskExecState::{
            Escalated, Failed, Pending, Ready, Retry, Running, Success, Verifying,
        };
        matches!(
            (self.state, new_state),
            (Pending, Ready | Running)
                | (Ready | Retry, Running)
                | (Running, Verifying)
                | (Verifying, Success | Retry | Failed)
                | (Failed, Escalated)
        )
    }
}

/// A `TaskFlow` - runtime instance of a `TaskGraph`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFlow {
    /// Unique flow ID.
    pub id: Uuid,
    /// Associated `TaskGraph` ID.
    pub graph_id: Uuid,
    /// Associated project ID.
    pub project_id: Uuid,
    #[serde(default)]
    pub base_revision: Option<String>,
    /// Current flow state.
    pub state: FlowState,
    /// Task execution states.
    pub task_executions: HashMap<Uuid, TaskExecution>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Start timestamp.
    pub started_at: Option<DateTime<Utc>>,
    /// Completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl TaskFlow {
    /// Creates a new `TaskFlow` from a graph.
    #[must_use]
    pub fn new(graph_id: Uuid, project_id: Uuid, task_ids: &[Uuid]) -> Self {
        let now = Utc::now();
        let mut task_executions = HashMap::new();

        for &task_id in task_ids {
            task_executions.insert(task_id, TaskExecution::new(task_id));
        }

        Self {
            id: Uuid::new_v4(),
            graph_id,
            project_id,
            base_revision: None,
            state: FlowState::Created,
            task_executions,
            created_at: now,
            started_at: None,
            completed_at: None,
            updated_at: now,
        }
    }

    /// Starts the flow.
    pub fn start(&mut self) -> Result<(), FlowError> {
        if self.state != FlowState::Created {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Running,
            });
        }

        self.state = FlowState::Running;
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Pauses the flow.
    pub fn pause(&mut self) -> Result<(), FlowError> {
        if self.state != FlowState::Running {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Paused,
            });
        }

        self.state = FlowState::Paused;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Resumes the flow.
    pub fn resume(&mut self) -> Result<(), FlowError> {
        if self.state != FlowState::Paused {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Running,
            });
        }

        self.state = FlowState::Running;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Aborts the flow.
    pub fn abort(&mut self) -> Result<(), FlowError> {
        if matches!(
            self.state,
            FlowState::Completed | FlowState::Merged | FlowState::Aborted
        ) {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Aborted,
            });
        }

        self.state = FlowState::Aborted;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Marks the flow as completed.
    pub fn complete(&mut self) -> Result<(), FlowError> {
        if self.state != FlowState::Running {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Completed,
            });
        }

        // Check all tasks are in terminal state
        for exec in self.task_executions.values() {
            if !matches!(
                exec.state,
                TaskExecState::Success | TaskExecState::Failed | TaskExecState::Escalated
            ) {
                return Err(FlowError::TasksNotComplete);
            }
        }

        self.state = FlowState::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Gets the execution state for a task.
    #[must_use]
    pub fn get_task_execution(&self, task_id: Uuid) -> Option<&TaskExecution> {
        self.task_executions.get(&task_id)
    }

    /// Gets mutable execution state for a task.
    pub fn get_task_execution_mut(&mut self, task_id: Uuid) -> Option<&mut TaskExecution> {
        self.task_executions.get_mut(&task_id)
    }

    /// Transitions a task to a new state.
    pub fn transition_task(
        &mut self,
        task_id: Uuid,
        new_state: TaskExecState,
    ) -> Result<(), FlowError> {
        let exec = self
            .task_executions
            .get_mut(&task_id)
            .ok_or(FlowError::TaskNotFound(task_id))?;

        exec.transition(new_state)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Checks if the flow is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            FlowState::Completed | FlowState::Merged | FlowState::Aborted
        )
    }

    /// Gets tasks in a particular state.
    #[must_use]
    pub fn tasks_in_state(&self, state: TaskExecState) -> Vec<Uuid> {
        self.task_executions
            .iter()
            .filter(|(_, exec)| exec.state == state)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Counts tasks by state.
    #[must_use]
    pub fn task_state_counts(&self) -> HashMap<TaskExecState, usize> {
        let mut counts = HashMap::new();
        for exec in self.task_executions.values() {
            *counts.entry(exec.state).or_insert(0) += 1;
        }
        counts
    }
}

/// Errors that can occur during flow operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError {
    /// Invalid task state transition.
    InvalidTransition {
        from: TaskExecState,
        to: TaskExecState,
    },
    /// Invalid flow state transition.
    InvalidFlowTransition { from: FlowState, to: FlowState },
    /// Task not found.
    TaskNotFound(Uuid),
    /// Cannot complete flow with incomplete tasks.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_flow() -> TaskFlow {
        let task_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        TaskFlow::new(Uuid::new_v4(), Uuid::new_v4(), &task_ids)
    }

    #[test]
    fn create_flow() {
        let flow = test_flow();
        assert_eq!(flow.state, FlowState::Created);
        assert_eq!(flow.task_executions.len(), 3);
    }

    #[test]
    fn flow_lifecycle() {
        let mut flow = test_flow();

        assert!(flow.start().is_ok());
        assert_eq!(flow.state, FlowState::Running);
        assert!(flow.started_at.is_some());

        assert!(flow.pause().is_ok());
        assert_eq!(flow.state, FlowState::Paused);

        assert!(flow.resume().is_ok());
        assert_eq!(flow.state, FlowState::Running);

        assert!(flow.abort().is_ok());
        assert_eq!(flow.state, FlowState::Aborted);
        assert!(flow.completed_at.is_some());
    }

    #[test]
    fn cannot_start_twice() {
        let mut flow = test_flow();
        flow.start().unwrap();

        let result = flow.start();
        assert!(result.is_err());
    }

    #[test]
    fn task_execution_transitions() {
        let mut exec = TaskExecution::new(Uuid::new_v4());

        assert_eq!(exec.state, TaskExecState::Pending);

        exec.transition(TaskExecState::Ready).unwrap();
        assert_eq!(exec.state, TaskExecState::Ready);

        exec.transition(TaskExecState::Running).unwrap();
        assert_eq!(exec.state, TaskExecState::Running);
        assert_eq!(exec.attempt_count, 1);

        exec.transition(TaskExecState::Verifying).unwrap();
        assert_eq!(exec.state, TaskExecState::Verifying);

        exec.transition(TaskExecState::Success).unwrap();
        assert_eq!(exec.state, TaskExecState::Success);
    }

    #[test]
    fn task_retry_cycle() {
        let mut exec = TaskExecution::new(Uuid::new_v4());

        exec.transition(TaskExecState::Ready).unwrap();
        exec.transition(TaskExecState::Running).unwrap();
        exec.transition(TaskExecState::Verifying).unwrap();
        exec.transition(TaskExecState::Retry).unwrap();

        assert_eq!(exec.attempt_count, 1);

        exec.transition(TaskExecState::Running).unwrap();
        assert_eq!(exec.attempt_count, 2);
    }

    #[test]
    fn invalid_task_transition() {
        let mut exec = TaskExecution::new(Uuid::new_v4());

        // Cannot go directly from Pending to Success
        let result = exec.transition(TaskExecState::Success);
        assert!(result.is_err());
    }

    #[test]
    fn flow_task_transition() {
        let mut flow = test_flow();
        let task_id = *flow.task_executions.keys().next().unwrap();

        flow.transition_task(task_id, TaskExecState::Ready).unwrap();

        let exec = flow.get_task_execution(task_id).unwrap();
        assert_eq!(exec.state, TaskExecState::Ready);
    }

    #[test]
    fn tasks_in_state() {
        let mut flow = test_flow();
        let task_ids: Vec<_> = flow.task_executions.keys().copied().collect();

        flow.transition_task(task_ids[0], TaskExecState::Ready)
            .unwrap();
        flow.transition_task(task_ids[1], TaskExecState::Ready)
            .unwrap();

        let ready_tasks = flow.tasks_in_state(TaskExecState::Ready);
        assert_eq!(ready_tasks.len(), 2);

        let pending_tasks = flow.tasks_in_state(TaskExecState::Pending);
        assert_eq!(pending_tasks.len(), 1);
    }

    #[test]
    fn flow_completion_requires_terminal_tasks() {
        let mut flow = test_flow();
        flow.start().unwrap();

        // Cannot complete with pending tasks
        let result = flow.complete();
        assert!(result.is_err());
    }

    #[test]
    fn flow_serialization() {
        let flow = test_flow();
        let json = serde_json::to_string(&flow).unwrap();
        let restored: TaskFlow = serde_json::from_str(&json).unwrap();

        assert_eq!(flow.id, restored.id);
        assert_eq!(flow.state, restored.state);
    }
}
