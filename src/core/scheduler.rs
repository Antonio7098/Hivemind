//! Scheduler - Dependency resolution and task execution coordination.
//!
//! The scheduler releases tasks for execution when their dependencies
//! are satisfied and coordinates the execution flow.

use super::flow::{FlowError, TaskExecState, TaskFlow};
use super::graph::TaskGraph;
use uuid::Uuid;

mod attempt;
mod scheduler_impl;

#[cfg(test)]
mod tests;

/// Result of a scheduling decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleResult {
    /// Tasks are ready for execution.
    Ready(Vec<Uuid>),
    /// All tasks are complete.
    Complete,
    /// Flow is blocked (waiting on running/verifying tasks).
    Blocked,
    /// Flow has failed tasks.
    HasFailures(Vec<Uuid>),
}

/// Scheduler for `TaskFlow` execution.
pub struct Scheduler<'a> {
    graph: &'a TaskGraph,
    flow: &'a mut TaskFlow,
}

/// Attempt tracking for a task execution.
#[derive(Debug, Clone)]
pub struct Attempt {
    /// Unique attempt ID.
    pub id: Uuid,
    /// Task this attempt is for.
    pub task_id: Uuid,
    /// Attempt number (1-indexed).
    pub attempt_number: u32,
    /// Start timestamp.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// End timestamp.
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Outcome.
    pub outcome: Option<AttemptOutcome>,
}

/// Outcome of an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// Attempt succeeded.
    Success,
    /// Attempt failed (may retry).
    Failed,
    /// Attempt crashed.
    Crashed,
}
