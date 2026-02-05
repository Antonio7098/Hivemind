//! TaskGraph - Static, immutable DAG representing planned intent.
//!
//! A TaskGraph is created by the Planner and is immutable once execution begins.
//! It represents what should happen, not what has happened.

use super::scope::Scope;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Retry policy for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Whether to escalate to human on final failure.
    pub escalate_on_failure: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            escalate_on_failure: true,
        }
    }
}

/// Success criteria for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuccessCriteria {
    /// Human-readable description of success.
    pub description: String,
    /// Automated checks to run (command patterns).
    pub checks: Vec<String>,
}

impl SuccessCriteria {
    /// Creates new success criteria.
    #[must_use]
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            checks: Vec::new(),
        }
    }

    /// Adds an automated check.
    #[must_use]
    pub fn with_check(mut self, check: impl Into<String>) -> Self {
        self.checks.push(check.into());
        self
    }
}

/// A task node within a TaskGraph.
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
    /// Required scope (optional at graph creation).
    pub scope: Option<Scope>,
}

impl GraphTask {
    /// Creates a new graph task.
    #[must_use]
    pub fn new(title: impl Into<String>, criteria: SuccessCriteria) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: None,
            criteria,
            retry_policy: RetryPolicy::default(),
            scope: None,
        }
    }

    /// Sets the task description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Sets the retry policy.
    #[must_use]
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Sets the scope.
    #[must_use]
    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = Some(scope);
        self
    }
}

/// State of a TaskGraph.
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

/// A TaskGraph - static DAG of planned intent.
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
    /// Dependencies (task_id -> set of dependency task_ids).
    pub dependencies: HashMap<Uuid, HashSet<Uuid>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl TaskGraph {
    /// Creates a new draft TaskGraph.
    #[must_use]
    pub fn new(project_id: Uuid, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            description: None,
            state: GraphState::Draft,
            tasks: HashMap::new(),
            dependencies: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Checks if the graph is modifiable.
    #[must_use]
    pub fn is_modifiable(&self) -> bool {
        self.state == GraphState::Draft
    }

    /// Adds a task to the graph.
    ///
    /// # Errors
    /// Returns an error if the graph is locked.
    pub fn add_task(&mut self, task: GraphTask) -> Result<Uuid, GraphError> {
        if !self.is_modifiable() {
            return Err(GraphError::GraphLocked);
        }

        let id = task.id;
        self.tasks.insert(id, task);
        self.dependencies.insert(id, HashSet::new());
        self.updated_at = Utc::now();
        Ok(id)
    }

    /// Adds a dependency between tasks.
    ///
    /// # Errors
    /// Returns an error if the graph is locked, tasks don't exist, or would create a cycle.
    pub fn add_dependency(&mut self, from: Uuid, to: Uuid) -> Result<(), GraphError> {
        if !self.is_modifiable() {
            return Err(GraphError::GraphLocked);
        }

        if !self.tasks.contains_key(&from) {
            return Err(GraphError::TaskNotFound(from));
        }
        if !self.tasks.contains_key(&to) {
            return Err(GraphError::TaskNotFound(to));
        }

        // Check for self-dependency
        if from == to {
            return Err(GraphError::CycleDetected);
        }

        // Add dependency tentatively
        self.dependencies.entry(from).or_default().insert(to);

        // Check for cycles
        if self.has_cycle() {
            // Rollback
            self.dependencies.entry(from).or_default().remove(&to);
            return Err(GraphError::CycleDetected);
        }

        self.updated_at = Utc::now();
        Ok(())
    }

    /// Sets scope for a task.
    ///
    /// # Errors
    /// Returns an error if the graph is locked or task doesn't exist.
    pub fn set_scope(&mut self, task_id: Uuid, scope: Scope) -> Result<(), GraphError> {
        if !self.is_modifiable() {
            return Err(GraphError::GraphLocked);
        }

        let task = self
            .tasks
            .get_mut(&task_id)
            .ok_or(GraphError::TaskNotFound(task_id))?;

        task.scope = Some(scope);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Validates the graph and transitions to Validated state.
    ///
    /// # Errors
    /// Returns an error if validation fails.
    pub fn validate(&mut self) -> Result<(), GraphError> {
        if self.state != GraphState::Draft {
            return Err(GraphError::InvalidStateTransition);
        }

        // Must have at least one task
        if self.tasks.is_empty() {
            return Err(GraphError::EmptyGraph);
        }

        // Check for cycles (should already be prevented, but double-check)
        if self.has_cycle() {
            return Err(GraphError::CycleDetected);
        }

        // All dependencies must reference existing tasks
        for (task_id, deps) in &self.dependencies {
            if !self.tasks.contains_key(task_id) {
                return Err(GraphError::TaskNotFound(*task_id));
            }
            for dep in deps {
                if !self.tasks.contains_key(dep) {
                    return Err(GraphError::TaskNotFound(*dep));
                }
            }
        }

        self.state = GraphState::Validated;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Locks the graph (immutable after this).
    ///
    /// # Errors
    /// Returns an error if the graph is not validated.
    pub fn lock(&mut self) -> Result<(), GraphError> {
        if self.state != GraphState::Validated {
            return Err(GraphError::InvalidStateTransition);
        }

        self.state = GraphState::Locked;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Checks if the graph contains a cycle using DFS.
    fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for task_id in self.tasks.keys() {
            if self.has_cycle_util(*task_id, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    fn has_cycle_util(
        &self,
        node: Uuid,
        visited: &mut HashSet<Uuid>,
        rec_stack: &mut HashSet<Uuid>,
    ) -> bool {
        if rec_stack.contains(&node) {
            return true;
        }
        if visited.contains(&node) {
            return false;
        }

        visited.insert(node);
        rec_stack.insert(node);

        if let Some(deps) = self.dependencies.get(&node) {
            for dep in deps {
                if self.has_cycle_util(*dep, visited, rec_stack) {
                    return true;
                }
            }
        }

        rec_stack.remove(&node);
        false
    }

    /// Returns tasks in topological order (dependencies first).
    #[must_use]
    pub fn topological_order(&self) -> Vec<Uuid> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();

        for task_id in self.tasks.keys() {
            self.topological_visit(*task_id, &mut visited, &mut result);
        }

        result
    }

    fn topological_visit(&self, node: Uuid, visited: &mut HashSet<Uuid>, result: &mut Vec<Uuid>) {
        if visited.contains(&node) {
            return;
        }

        visited.insert(node);

        if let Some(deps) = self.dependencies.get(&node) {
            for dep in deps {
                self.topological_visit(*dep, visited, result);
            }
        }

        result.push(node);
    }

    /// Gets tasks that have no dependencies (entry points).
    #[must_use]
    pub fn root_tasks(&self) -> Vec<Uuid> {
        self.tasks
            .keys()
            .filter(|id| {
                self.dependencies
                    .get(*id)
                    .map_or(true, |deps| deps.is_empty())
            })
            .copied()
            .collect()
    }

    /// Gets tasks that depend on a given task.
    #[must_use]
    pub fn dependents(&self, task_id: Uuid) -> Vec<Uuid> {
        self.dependencies
            .iter()
            .filter(|(_, deps)| deps.contains(&task_id))
            .map(|(id, _)| *id)
            .collect()
    }
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

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GraphLocked => write!(f, "Graph is locked and cannot be modified"),
            Self::TaskNotFound(id) => write!(f, "Task not found: {}", id),
            Self::CycleDetected => write!(f, "Cycle detected in task dependencies"),
            Self::InvalidStateTransition => write!(f, "Invalid state transition"),
            Self::EmptyGraph => write!(f, "Graph must contain at least one task"),
        }
    }
}

impl std::error::Error for GraphError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_graph() -> TaskGraph {
        TaskGraph::new(Uuid::new_v4(), "test-graph")
    }

    fn test_task(title: &str) -> GraphTask {
        GraphTask::new(title, SuccessCriteria::new("Task completed"))
    }

    #[test]
    fn create_graph() {
        let graph = test_graph();
        assert_eq!(graph.state, GraphState::Draft);
        assert!(graph.tasks.is_empty());
    }

    #[test]
    fn add_tasks() {
        let mut graph = test_graph();

        let t1 = graph.add_task(test_task("Task 1")).unwrap();
        let t2 = graph.add_task(test_task("Task 2")).unwrap();

        assert_eq!(graph.tasks.len(), 2);
        assert!(graph.tasks.contains_key(&t1));
        assert!(graph.tasks.contains_key(&t2));
    }

    #[test]
    fn add_dependencies() {
        let mut graph = test_graph();

        let t1 = graph.add_task(test_task("Task 1")).unwrap();
        let t2 = graph.add_task(test_task("Task 2")).unwrap();

        // t2 depends on t1
        graph.add_dependency(t2, t1).unwrap();

        assert!(graph.dependencies[&t2].contains(&t1));
    }

    #[test]
    fn prevent_cycles() {
        let mut graph = test_graph();

        let t1 = graph.add_task(test_task("Task 1")).unwrap();
        let t2 = graph.add_task(test_task("Task 2")).unwrap();
        let t3 = graph.add_task(test_task("Task 3")).unwrap();

        graph.add_dependency(t2, t1).unwrap();
        graph.add_dependency(t3, t2).unwrap();

        // This would create a cycle: t1 -> t2 -> t3 -> t1
        let result = graph.add_dependency(t1, t3);
        assert_eq!(result, Err(GraphError::CycleDetected));
    }

    #[test]
    fn prevent_self_dependency() {
        let mut graph = test_graph();
        let t1 = graph.add_task(test_task("Task 1")).unwrap();

        let result = graph.add_dependency(t1, t1);
        assert_eq!(result, Err(GraphError::CycleDetected));
    }

    #[test]
    fn topological_order() {
        let mut graph = test_graph();

        let t1 = graph.add_task(test_task("Task 1")).unwrap();
        let t2 = graph.add_task(test_task("Task 2")).unwrap();
        let t3 = graph.add_task(test_task("Task 3")).unwrap();

        // t2 depends on t1, t3 depends on t2
        graph.add_dependency(t2, t1).unwrap();
        graph.add_dependency(t3, t2).unwrap();

        let order = graph.topological_order();

        // t1 must come before t2, t2 must come before t3
        let pos1 = order.iter().position(|&x| x == t1).unwrap();
        let pos2 = order.iter().position(|&x| x == t2).unwrap();
        let pos3 = order.iter().position(|&x| x == t3).unwrap();

        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
    }

    #[test]
    fn root_tasks() {
        let mut graph = test_graph();

        let t1 = graph.add_task(test_task("Task 1")).unwrap();
        let t2 = graph.add_task(test_task("Task 2")).unwrap();
        let t3 = graph.add_task(test_task("Task 3")).unwrap();

        graph.add_dependency(t2, t1).unwrap();
        graph.add_dependency(t3, t1).unwrap();

        let roots = graph.root_tasks();
        assert_eq!(roots.len(), 1);
        assert!(roots.contains(&t1));
    }

    #[test]
    fn validate_and_lock() {
        let mut graph = test_graph();
        graph.add_task(test_task("Task 1")).unwrap();

        assert!(graph.validate().is_ok());
        assert_eq!(graph.state, GraphState::Validated);

        assert!(graph.lock().is_ok());
        assert_eq!(graph.state, GraphState::Locked);
    }

    #[test]
    fn cannot_modify_locked_graph() {
        let mut graph = test_graph();
        graph.add_task(test_task("Task 1")).unwrap();
        graph.validate().unwrap();
        graph.lock().unwrap();

        let result = graph.add_task(test_task("Task 2"));
        assert_eq!(result, Err(GraphError::GraphLocked));
    }

    #[test]
    fn cannot_validate_empty_graph() {
        let mut graph = test_graph();
        let result = graph.validate();
        assert_eq!(result, Err(GraphError::EmptyGraph));
    }

    #[test]
    fn graph_serialization() {
        let mut graph = test_graph();
        let t1 = graph.add_task(test_task("Task 1")).unwrap();
        let t2 = graph.add_task(test_task("Task 2")).unwrap();
        graph.add_dependency(t2, t1).unwrap();

        let json = serde_json::to_string(&graph).unwrap();
        let restored: TaskGraph = serde_json::from_str(&json).unwrap();

        assert_eq!(graph.id, restored.id);
        assert_eq!(graph.tasks.len(), restored.tasks.len());
    }

    #[test]
    fn dependents() {
        let mut graph = test_graph();

        let t1 = graph.add_task(test_task("Task 1")).unwrap();
        let t2 = graph.add_task(test_task("Task 2")).unwrap();
        let t3 = graph.add_task(test_task("Task 3")).unwrap();

        graph.add_dependency(t2, t1).unwrap();
        graph.add_dependency(t3, t1).unwrap();

        let dependents = graph.dependents(t1);
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&t2));
        assert!(dependents.contains(&t3));
    }
}
