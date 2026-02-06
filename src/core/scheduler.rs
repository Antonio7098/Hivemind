//! Scheduler - Dependency resolution and task execution coordination.
//!
//! The scheduler releases tasks for execution when their dependencies
//! are satisfied and coordinates the execution flow.

use super::flow::{FlowError, TaskExecState, TaskFlow};
use super::graph::TaskGraph;
use uuid::Uuid;

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

impl<'a> Scheduler<'a> {
    /// Creates a new scheduler.
    pub fn new(graph: &'a TaskGraph, flow: &'a mut TaskFlow) -> Self {
        Self { graph, flow }
    }

    /// Updates task readiness based on dependencies.
    ///
    /// Returns the list of tasks that became ready.
    pub fn update_readiness(&mut self) -> Vec<Uuid> {
        let mut newly_ready = Vec::new();

        for task_id in self.graph.tasks.keys() {
            if let Some(exec) = self.flow.get_task_execution(*task_id) {
                if exec.state != TaskExecState::Pending {
                    continue;
                }
            } else {
                continue;
            }

            // Check if all dependencies are satisfied
            if self.dependencies_satisfied(*task_id)
                && self
                    .flow
                    .transition_task(*task_id, TaskExecState::Ready)
                    .is_ok()
            {
                newly_ready.push(*task_id);
            }
        }

        newly_ready
    }

    /// Checks if all dependencies for a task are satisfied (SUCCESS).
    fn dependencies_satisfied(&self, task_id: Uuid) -> bool {
        let Some(deps) = self.graph.dependencies.get(&task_id) else {
            return true;
        };

        for dep_id in deps {
            if let Some(exec) = self.flow.get_task_execution(*dep_id) {
                if exec.state != TaskExecState::Success {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Gets the current scheduling decision.
    pub fn schedule(&mut self) -> ScheduleResult {
        // First, update readiness
        self.update_readiness();

        // Check for failures
        let failed: Vec<_> = self.flow.tasks_in_state(TaskExecState::Failed);
        let escalated: Vec<_> = self.flow.tasks_in_state(TaskExecState::Escalated);
        if !failed.is_empty() || !escalated.is_empty() {
            let mut failures = failed;
            failures.extend(escalated);
            return ScheduleResult::HasFailures(failures);
        }

        // Check for ready tasks
        let ready: Vec<_> = self.flow.tasks_in_state(TaskExecState::Ready);
        if !ready.is_empty() {
            return ScheduleResult::Ready(ready);
        }

        // Check if all tasks are complete
        let pending = self.flow.tasks_in_state(TaskExecState::Pending);
        let running = self.flow.tasks_in_state(TaskExecState::Running);
        let verifying = self.flow.tasks_in_state(TaskExecState::Verifying);
        let retry = self.flow.tasks_in_state(TaskExecState::Retry);

        if pending.is_empty() && running.is_empty() && verifying.is_empty() && retry.is_empty() {
            return ScheduleResult::Complete;
        }

        // Otherwise, we're blocked waiting
        ScheduleResult::Blocked
    }

    /// Marks a task as started (Running).
    pub fn start_task(&mut self, task_id: Uuid) -> Result<(), FlowError> {
        let exec = self
            .flow
            .get_task_execution(task_id)
            .ok_or(FlowError::TaskNotFound(task_id))?;

        // Can start from Ready or Retry state
        if exec.state != TaskExecState::Ready && exec.state != TaskExecState::Retry {
            return Err(FlowError::InvalidTransition {
                from: exec.state,
                to: TaskExecState::Running,
            });
        }

        self.flow.transition_task(task_id, TaskExecState::Running)
    }

    /// Marks a task as completed execution (moves to Verifying).
    pub fn complete_execution(&mut self, task_id: Uuid) -> Result<(), FlowError> {
        self.flow.transition_task(task_id, TaskExecState::Verifying)
    }

    /// Records verification result.
    pub fn record_verification(
        &mut self,
        task_id: Uuid,
        passed: bool,
        can_retry: bool,
    ) -> Result<(), FlowError> {
        let new_state = if passed {
            TaskExecState::Success
        } else if can_retry {
            TaskExecState::Retry
        } else {
            TaskExecState::Failed
        };

        self.flow.transition_task(task_id, new_state)
    }

    /// Escalates a failed task to human intervention.
    pub fn escalate(&mut self, task_id: Uuid) -> Result<(), FlowError> {
        let exec = self
            .flow
            .get_task_execution(task_id)
            .ok_or(FlowError::TaskNotFound(task_id))?;

        if exec.state != TaskExecState::Failed {
            return Err(FlowError::InvalidTransition {
                from: exec.state,
                to: TaskExecState::Escalated,
            });
        }

        self.flow.transition_task(task_id, TaskExecState::Escalated)
    }

    /// Gets tasks that are blocked by a specific task.
    pub fn blocked_by(&self, task_id: Uuid) -> Vec<Uuid> {
        self.graph.dependents(task_id)
    }

    /// Checks if the flow can complete (all tasks in terminal state or can reach terminal).
    pub fn can_complete(&self) -> bool {
        let pending = self.flow.tasks_in_state(TaskExecState::Pending);
        let running = self.flow.tasks_in_state(TaskExecState::Running);
        let verifying = self.flow.tasks_in_state(TaskExecState::Verifying);
        let retry = self.flow.tasks_in_state(TaskExecState::Retry);
        let ready = self.flow.tasks_in_state(TaskExecState::Ready);

        pending.is_empty()
            && running.is_empty()
            && verifying.is_empty()
            && retry.is_empty()
            && ready.is_empty()
    }
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

impl Attempt {
    /// Creates a new attempt.
    pub fn new(task_id: Uuid, attempt_number: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            attempt_number,
            started_at: chrono::Utc::now(),
            ended_at: None,
            outcome: None,
        }
    }

    /// Completes the attempt with an outcome.
    pub fn complete(&mut self, outcome: AttemptOutcome) {
        self.ended_at = Some(chrono::Utc::now());
        self.outcome = Some(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::graph::{GraphTask, SuccessCriteria, TaskGraph};

    fn setup_test() -> (TaskGraph, TaskFlow) {
        let project_id = Uuid::new_v4();
        let mut graph = TaskGraph::new(project_id, "test");

        let t1 = graph
            .add_task(GraphTask::new("Task 1", SuccessCriteria::new("Done")))
            .unwrap();
        let t2 = graph
            .add_task(GraphTask::new("Task 2", SuccessCriteria::new("Done")))
            .unwrap();
        let t3 = graph
            .add_task(GraphTask::new("Task 3", SuccessCriteria::new("Done")))
            .unwrap();

        // t2 depends on t1, t3 depends on t2
        graph.add_dependency(t2, t1).unwrap();
        graph.add_dependency(t3, t2).unwrap();

        let task_ids: Vec<_> = graph.tasks.keys().copied().collect();
        let flow = TaskFlow::new(graph.id, project_id, &task_ids);

        (graph, flow)
    }

    #[test]
    fn initial_readiness() {
        let (graph, mut flow) = setup_test();
        let mut scheduler = Scheduler::new(&graph, &mut flow);

        let ready = scheduler.update_readiness();

        // Only t1 should be ready (no dependencies)
        assert_eq!(ready.len(), 1);

        let root_tasks = graph.root_tasks();
        assert!(ready.iter().all(|id| root_tasks.contains(id)));
    }

    #[test]
    fn dependency_chain_execution() {
        let (graph, mut flow) = setup_test();
        let task_ids: Vec<_> = graph.topological_order();
        let t1 = task_ids[0];
        let t2 = task_ids[1];
        let t3 = task_ids[2];

        {
            let mut scheduler = Scheduler::new(&graph, &mut flow);

            // Initial schedule - t1 should be ready
            let result = scheduler.schedule();
            assert!(matches!(result, ScheduleResult::Ready(ref tasks) if tasks.contains(&t1)));

            // Start t1
            scheduler.start_task(t1).unwrap();
        }

        {
            let mut scheduler = Scheduler::new(&graph, &mut flow);

            // Should be blocked (t1 running)
            let result = scheduler.schedule();
            assert!(matches!(result, ScheduleResult::Blocked));

            // Complete t1 execution
            scheduler.complete_execution(t1).unwrap();

            // Pass verification
            scheduler.record_verification(t1, true, false).unwrap();
        }

        {
            let mut scheduler = Scheduler::new(&graph, &mut flow);

            // Now t2 should be ready
            let result = scheduler.schedule();
            assert!(matches!(result, ScheduleResult::Ready(ref tasks) if tasks.contains(&t2)));

            // Complete t2
            scheduler.start_task(t2).unwrap();
            scheduler.complete_execution(t2).unwrap();
            scheduler.record_verification(t2, true, false).unwrap();
        }

        {
            let mut scheduler = Scheduler::new(&graph, &mut flow);

            // Now t3 should be ready
            let result = scheduler.schedule();
            assert!(matches!(result, ScheduleResult::Ready(ref tasks) if tasks.contains(&t3)));

            // Complete t3
            scheduler.start_task(t3).unwrap();
            scheduler.complete_execution(t3).unwrap();
            scheduler.record_verification(t3, true, false).unwrap();
        }

        {
            let mut scheduler = Scheduler::new(&graph, &mut flow);

            // Should be complete
            let result = scheduler.schedule();
            assert!(matches!(result, ScheduleResult::Complete));
        }
    }

    #[test]
    fn retry_cycle() {
        let (graph, mut flow) = setup_test();
        let t1 = graph.root_tasks()[0];

        {
            let mut scheduler = Scheduler::new(&graph, &mut flow);
            scheduler.update_readiness();
            scheduler.start_task(t1).unwrap();
            scheduler.complete_execution(t1).unwrap();
            // Fail with retry
            scheduler.record_verification(t1, false, true).unwrap();
        }

        let exec = flow.get_task_execution(t1).unwrap();
        assert_eq!(exec.state, TaskExecState::Retry);

        {
            let mut scheduler = Scheduler::new(&graph, &mut flow);
            // Retry
            scheduler.start_task(t1).unwrap();
        }

        let exec = flow.get_task_execution(t1).unwrap();
        assert_eq!(exec.attempt_count, 2);
    }

    #[test]
    fn failure_handling() {
        let (graph, mut flow) = setup_test();
        let t1 = graph.root_tasks()[0];

        let mut scheduler = Scheduler::new(&graph, &mut flow);

        scheduler.update_readiness();
        scheduler.start_task(t1).unwrap();
        scheduler.complete_execution(t1).unwrap();

        // Fail without retry
        scheduler.record_verification(t1, false, false).unwrap();

        let result = scheduler.schedule();
        assert!(matches!(result, ScheduleResult::HasFailures(_)));
    }

    #[test]
    fn escalation() {
        let (graph, mut flow) = setup_test();
        let t1 = graph.root_tasks()[0];

        let mut scheduler = Scheduler::new(&graph, &mut flow);

        scheduler.update_readiness();
        scheduler.start_task(t1).unwrap();
        scheduler.complete_execution(t1).unwrap();
        scheduler.record_verification(t1, false, false).unwrap();

        // Escalate
        scheduler.escalate(t1).unwrap();

        let exec = flow.get_task_execution(t1).unwrap();
        assert_eq!(exec.state, TaskExecState::Escalated);
    }

    #[test]
    fn attempt_tracking() {
        let task_id = Uuid::new_v4();
        let mut attempt = Attempt::new(task_id, 1);

        assert_eq!(attempt.attempt_number, 1);
        assert!(attempt.ended_at.is_none());
        assert!(attempt.outcome.is_none());

        attempt.complete(AttemptOutcome::Success);

        assert!(attempt.ended_at.is_some());
        assert_eq!(attempt.outcome, Some(AttemptOutcome::Success));
    }
}
