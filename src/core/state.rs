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

impl RuntimeRoleDefaults {
    pub fn set(&mut self, role: RuntimeRole, config: Option<ProjectRuntimeConfig>) {
        match role {
            RuntimeRole::Worker => self.worker = config,
            RuntimeRole::Validator => self.validator = config,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaskRuntimeRoleOverrides {
    #[serde(default)]
    pub worker: Option<TaskRuntimeConfig>,
    #[serde(default)]
    pub validator: Option<TaskRuntimeConfig>,
}

impl TaskRuntimeRoleOverrides {
    pub fn set(&mut self, role: RuntimeRole, config: Option<TaskRuntimeConfig>) {
        match role {
            RuntimeRole::Worker => self.worker = config,
            RuntimeRole::Validator => self.validator = config,
        }
    }
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
    pub tasks: HashMap<Uuid, Task>,
    pub graphs: HashMap<Uuid, TaskGraph>,
    pub flows: HashMap<Uuid, TaskFlow>,
    pub global_runtime_defaults: RuntimeRoleDefaults,
    pub flow_runtime_defaults: HashMap<Uuid, RuntimeRoleDefaults>,
    pub merge_states: HashMap<Uuid, MergeState>,
    pub attempts: HashMap<Uuid, AttemptState>,
}

impl AppState {
    /// Creates a new empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies an event to the state, returning a new state.
    #[must_use]
    pub fn apply(mut self, event: &Event) -> Self {
        self.apply_mut(event);
        self
    }

    /// Applies an event to the state in place.
    #[allow(clippy::too_many_lines)]
    pub fn apply_mut(&mut self, event: &Event) {
        let timestamp = event.timestamp();

        match &event.payload {
            EventPayload::ProjectCreated {
                id,
                name,
                description,
            } => {
                self.projects.insert(
                    *id,
                    Project {
                        id: *id,
                        name: name.clone(),
                        description: description.clone(),
                        created_at: timestamp,
                        updated_at: timestamp,
                        repositories: Vec::new(),
                        runtime: None,
                        runtime_defaults: RuntimeRoleDefaults::default(),
                    },
                );
            }
            EventPayload::ProjectUpdated {
                id,
                name,
                description,
            } => {
                if let Some(project) = self.projects.get_mut(id) {
                    if let Some(n) = name {
                        n.clone_into(&mut project.name);
                    }
                    if let Some(d) = description {
                        project.description = Some(d.clone());
                    }
                    project.updated_at = timestamp;
                }
            }

            EventPayload::TaskExecutionFrozen {
                flow_id,
                task_id,
                commit_sha,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.frozen_commit_sha.clone_from(commit_sha);
                        exec.updated_at = timestamp;
                        flow.updated_at = timestamp;
                    }
                }
            }

            EventPayload::TaskIntegratedIntoFlow {
                flow_id,
                task_id,
                commit_sha,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.integrated_commit_sha.clone_from(commit_sha);
                        exec.updated_at = timestamp;
                        flow.updated_at = timestamp;
                    }
                }
            }
            EventPayload::ProjectRuntimeConfigured {
                project_id,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    let configured = ProjectRuntimeConfig {
                        adapter_name: adapter_name.clone(),
                        binary_path: binary_path.clone(),
                        model: model.clone(),
                        args: args.clone(),
                        env: env.clone(),
                        timeout_ms: *timeout_ms,
                        max_parallel_tasks: *max_parallel_tasks,
                    };
                    project.runtime = Some(configured.clone());
                    project.runtime_defaults.worker = Some(configured);
                    project.updated_at = timestamp;
                }
            }
            EventPayload::ProjectRuntimeRoleConfigured {
                project_id,
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    let configured = ProjectRuntimeConfig {
                        adapter_name: adapter_name.clone(),
                        binary_path: binary_path.clone(),
                        model: model.clone(),
                        args: args.clone(),
                        env: env.clone(),
                        timeout_ms: *timeout_ms,
                        max_parallel_tasks: *max_parallel_tasks,
                    };
                    project
                        .runtime_defaults
                        .set(*role, Some(configured.clone()));
                    if *role == RuntimeRole::Worker {
                        project.runtime = Some(configured);
                    }
                    project.updated_at = timestamp;
                }
            }
            EventPayload::GlobalRuntimeConfigured {
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                let configured = ProjectRuntimeConfig {
                    adapter_name: adapter_name.clone(),
                    binary_path: binary_path.clone(),
                    model: model.clone(),
                    args: args.clone(),
                    env: env.clone(),
                    timeout_ms: *timeout_ms,
                    max_parallel_tasks: *max_parallel_tasks,
                };
                self.global_runtime_defaults.set(*role, Some(configured));
            }
            EventPayload::TaskCreated {
                id,
                project_id,
                title,
                description,
                scope,
            } => {
                self.tasks.insert(
                    *id,
                    Task {
                        id: *id,
                        project_id: *project_id,
                        title: title.clone(),
                        description: description.clone(),
                        scope: scope.clone(),
                        runtime_override: None,
                        runtime_overrides: TaskRuntimeRoleOverrides::default(),
                        run_mode: RunMode::Auto,
                        state: TaskState::Open,
                        created_at: timestamp,
                        updated_at: timestamp,
                    },
                );
            }
            EventPayload::TaskUpdated {
                id,
                title,
                description,
            } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    if let Some(t) = title {
                        t.clone_into(&mut task.title);
                    }
                    if let Some(d) = description {
                        task.description = Some(d.clone());
                    }
                    task.updated_at = timestamp;
                }
            }
            EventPayload::TaskRuntimeConfigured {
                task_id,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    let configured = TaskRuntimeConfig {
                        adapter_name: adapter_name.clone(),
                        binary_path: binary_path.clone(),
                        model: model.clone(),
                        args: args.clone(),
                        env: env.clone(),
                        timeout_ms: *timeout_ms,
                    };
                    task.runtime_override = Some(configured.clone());
                    task.runtime_overrides.worker = Some(configured);
                    task.updated_at = timestamp;
                }
            }
            EventPayload::TaskRuntimeRoleConfigured {
                task_id,
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    let configured = TaskRuntimeConfig {
                        adapter_name: adapter_name.clone(),
                        binary_path: binary_path.clone(),
                        model: model.clone(),
                        args: args.clone(),
                        env: env.clone(),
                        timeout_ms: *timeout_ms,
                    };
                    task.runtime_overrides.set(*role, Some(configured.clone()));
                    if *role == RuntimeRole::Worker {
                        task.runtime_override = Some(configured);
                    }
                    task.updated_at = timestamp;
                }
            }
            EventPayload::TaskRuntimeCleared { task_id } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.runtime_override = None;
                    task.runtime_overrides.worker = None;
                    task.updated_at = timestamp;
                }
            }
            EventPayload::TaskRuntimeRoleCleared { task_id, role } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.runtime_overrides.set(*role, None);
                    if *role == RuntimeRole::Worker {
                        task.runtime_override = None;
                    }
                    task.updated_at = timestamp;
                }
            }
            EventPayload::TaskRunModeSet { task_id, mode } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.run_mode = *mode;
                    task.updated_at = timestamp;
                }
            }
            EventPayload::TaskClosed { id, reason: _ } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    task.state = TaskState::Closed;
                    task.updated_at = timestamp;
                }
            }
            EventPayload::RepositoryAttached {
                project_id,
                path,
                name,
                access_mode,
            } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    project.repositories.push(Repository {
                        name: name.clone(),
                        path: path.clone(),
                        access_mode: *access_mode,
                    });
                    project.updated_at = timestamp;
                }
            }
            EventPayload::RepositoryDetached { project_id, name } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    project.repositories.retain(|r| r.name != *name);
                    project.updated_at = timestamp;
                }
            }

            EventPayload::TaskGraphCreated {
                graph_id,
                project_id,
                name,
                description,
            } => {
                self.graphs.insert(
                    *graph_id,
                    TaskGraph {
                        id: *graph_id,
                        project_id: *project_id,
                        name: name.clone(),
                        description: description.clone(),
                        state: GraphState::Draft,
                        tasks: HashMap::new(),
                        dependencies: HashMap::<Uuid, HashSet<Uuid>>::new(),
                        created_at: timestamp,
                        updated_at: timestamp,
                    },
                );
            }
            EventPayload::TaskAddedToGraph { graph_id, task } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.tasks.insert(task.id, task.clone());
                    graph.dependencies.entry(task.id).or_default();
                    graph.updated_at = timestamp;
                }
            }
            EventPayload::DependencyAdded {
                graph_id,
                from_task,
                to_task,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph
                        .dependencies
                        .entry(*to_task)
                        .or_default()
                        .insert(*from_task);
                    graph.updated_at = timestamp;
                }
            }
            EventPayload::GraphTaskCheckAdded {
                graph_id,
                task_id,
                check,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    if let Some(task) = graph.tasks.get_mut(task_id) {
                        task.criteria.checks.push(check.clone());
                        graph.updated_at = timestamp;
                    }
                }
            }
            EventPayload::ScopeAssigned {
                graph_id,
                task_id,
                scope,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    if let Some(task) = graph.tasks.get_mut(task_id) {
                        task.scope = Some(scope.clone());
                        graph.updated_at = timestamp;
                    }
                }
            }

            EventPayload::TaskGraphValidated {
                graph_id,
                project_id: _,
                valid,
                issues: _,
            } => {
                if *valid {
                    if let Some(graph) = self.graphs.get_mut(graph_id) {
                        graph.state = GraphState::Validated;
                        graph.updated_at = timestamp;
                    }
                }
            }

            EventPayload::TaskGraphLocked {
                graph_id,
                project_id: _,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.state = GraphState::Locked;
                    graph.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowCreated {
                flow_id,
                graph_id,
                project_id,
                name: _,
                task_ids,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.state = GraphState::Locked;
                    graph.updated_at = timestamp;
                }

                let mut task_executions = HashMap::new();
                for task_id in task_ids {
                    task_executions.insert(
                        *task_id,
                        TaskExecution {
                            task_id: *task_id,
                            state: TaskExecState::Pending,
                            attempt_count: 0,
                            retry_mode: RetryMode::default(),
                            frozen_commit_sha: None,
                            integrated_commit_sha: None,
                            updated_at: timestamp,
                            blocked_reason: None,
                        },
                    );
                }

                self.flows.insert(
                    *flow_id,
                    TaskFlow {
                        id: *flow_id,
                        graph_id: *graph_id,
                        project_id: *project_id,
                        base_revision: None,
                        run_mode: RunMode::Manual,
                        depends_on_flows: HashSet::new(),
                        state: FlowState::Created,
                        task_executions,
                        created_at: timestamp,
                        started_at: None,
                        completed_at: None,
                        updated_at: timestamp,
                    },
                );
                self.flow_runtime_defaults.entry(*flow_id).or_default();
            }
            EventPayload::TaskFlowDependencyAdded {
                flow_id,
                depends_on_flow_id,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.depends_on_flows.insert(*depends_on_flow_id);
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowRunModeSet { flow_id, mode } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.run_mode = *mode;
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowRuntimeConfigured {
                flow_id,
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                let configured = ProjectRuntimeConfig {
                    adapter_name: adapter_name.clone(),
                    binary_path: binary_path.clone(),
                    model: model.clone(),
                    args: args.clone(),
                    env: env.clone(),
                    timeout_ms: *timeout_ms,
                    max_parallel_tasks: *max_parallel_tasks,
                };
                self.flow_runtime_defaults
                    .entry(*flow_id)
                    .or_default()
                    .set(*role, Some(configured));
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowRuntimeCleared { flow_id, role } => {
                self.flow_runtime_defaults
                    .entry(*flow_id)
                    .or_default()
                    .set(*role, None);
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowStarted {
                flow_id,
                base_revision,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Running;
                    flow.started_at = Some(timestamp);
                    flow.base_revision.clone_from(base_revision);
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowPaused {
                flow_id,
                running_tasks: _,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Paused;
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowResumed { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Running;
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowCompleted { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Completed;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
            }

            EventPayload::FlowFrozenForMerge { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::FrozenForMerge;
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowAborted {
                flow_id,
                reason: _,
                forced: _,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Aborted;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskReady { flow_id, task_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = TaskExecState::Ready;
                        exec.blocked_reason = None;
                        exec.updated_at = timestamp;
                    }
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskBlocked {
                flow_id,
                task_id,
                reason,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = TaskExecState::Pending;
                        exec.blocked_reason.clone_from(reason);
                        exec.updated_at = timestamp;
                    }
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskExecutionStateChanged {
                flow_id,
                task_id,
                attempt_id: _,
                from: _,
                to,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = *to;
                        exec.updated_at = timestamp;
                        exec.blocked_reason = None;
                        if *to == TaskExecState::Running {
                            exec.attempt_count += 1;
                        }
                    }
                    flow.updated_at = timestamp;
                }
            }

            EventPayload::AttemptStarted {
                flow_id,
                task_id,
                attempt_id,
                attempt_number,
            } => {
                self.attempts.insert(
                    *attempt_id,
                    AttemptState {
                        id: *attempt_id,
                        flow_id: *flow_id,
                        task_id: *task_id,
                        attempt_number: *attempt_number,
                        started_at: timestamp,
                        baseline_id: None,
                        diff_id: None,
                        check_results: Vec::new(),
                        checkpoints: Vec::new(),
                        all_checkpoints_completed: false,
                    },
                );
            }

            EventPayload::CheckpointDeclared {
                attempt_id,
                checkpoint_id,
                order,
                total,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    let exists = attempt
                        .checkpoints
                        .iter()
                        .any(|cp| cp.checkpoint_id == *checkpoint_id);
                    if !exists {
                        attempt.checkpoints.push(AttemptCheckpoint {
                            checkpoint_id: checkpoint_id.clone(),
                            order: *order,
                            total: *total,
                            state: AttemptCheckpointState::Declared,
                            commit_hash: None,
                            completed_at: None,
                            summary: None,
                        });
                        attempt.checkpoints.sort_by_key(|cp| cp.order);
                    }
                }
            }

            EventPayload::CheckpointActivated {
                attempt_id,
                checkpoint_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    for cp in &mut attempt.checkpoints {
                        if cp.checkpoint_id == *checkpoint_id {
                            cp.state = AttemptCheckpointState::Active;
                        } else if cp.state != AttemptCheckpointState::Completed {
                            cp.state = AttemptCheckpointState::Declared;
                        }
                    }
                }
            }

            EventPayload::CheckpointCompleted {
                attempt_id,
                checkpoint_id,
                commit_hash,
                timestamp,
                summary,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    if let Some(cp) = attempt
                        .checkpoints
                        .iter_mut()
                        .find(|cp| cp.checkpoint_id == *checkpoint_id)
                    {
                        cp.state = AttemptCheckpointState::Completed;
                        cp.commit_hash = Some(commit_hash.clone());
                        cp.completed_at = Some(*timestamp);
                        cp.summary.clone_from(summary);
                    }
                }
            }

            EventPayload::AllCheckpointsCompleted { attempt_id, .. } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.all_checkpoints_completed = true;
                }
            }

            EventPayload::BaselineCaptured {
                attempt_id,
                baseline_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.baseline_id = Some(*baseline_id);
                }
            }

            EventPayload::DiffComputed {
                attempt_id,
                diff_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.diff_id = Some(*diff_id);
                }
            }

            EventPayload::CheckCompleted {
                attempt_id,
                check_name,
                passed,
                exit_code,
                output,
                duration_ms,
                required,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.check_results.push(CheckResult {
                        name: check_name.clone(),
                        passed: *passed,
                        exit_code: *exit_code,
                        output: output.clone(),
                        duration_ms: *duration_ms,
                        required: *required,
                    });
                }
            }

            EventPayload::CheckStarted { .. }
            | EventPayload::ErrorOccurred { .. }
            | EventPayload::TaskExecutionStarted { .. }
            | EventPayload::TaskExecutionSucceeded { .. }
            | EventPayload::TaskExecutionFailed { .. }
            | EventPayload::MergeConflictDetected { .. }
            | EventPayload::MergeCheckStarted { .. }
            | EventPayload::MergeCheckCompleted { .. }
            | EventPayload::RuntimeStarted { .. }
            | EventPayload::RuntimeOutputChunk { .. }
            | EventPayload::RuntimeInputProvided { .. }
            | EventPayload::RuntimeInterrupted { .. }
            | EventPayload::RuntimeExited { .. }
            | EventPayload::RuntimeTerminated { .. }
            | EventPayload::RuntimeFilesystemObserved { .. }
            | EventPayload::RuntimeCommandObserved { .. }
            | EventPayload::RuntimeToolCallObserved { .. }
            | EventPayload::RuntimeTodoSnapshotUpdated { .. }
            | EventPayload::RuntimeNarrativeOutputObserved { .. }
            | EventPayload::FileModified { .. }
            | EventPayload::CheckpointCommitCreated { .. }
            | EventPayload::ScopeValidated { .. }
            | EventPayload::ScopeViolationDetected { .. }
            | EventPayload::ScopeConflictDetected { .. }
            | EventPayload::TaskSchedulingDeferred { .. }
            | EventPayload::RetryContextAssembled { .. }
            | EventPayload::FlowIntegrationLockAcquired { .. }
            | EventPayload::Unknown => {}

            EventPayload::TaskRetryRequested {
                task_id,
                reset_count,
                retry_mode,
            } => {
                let flow_id = event.metadata.correlation.flow_id;
                let mut candidate_flow_ids = Vec::new();

                if let Some(fid) = flow_id {
                    candidate_flow_ids.push(fid);
                } else {
                    for (fid, flow) in &self.flows {
                        if flow.task_executions.contains_key(task_id) {
                            candidate_flow_ids.push(*fid);
                        }
                    }
                }

                for fid in candidate_flow_ids {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = TaskExecState::Pending;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            exec.retry_mode = *retry_mode;
                            if *reset_count {
                                exec.attempt_count = 0;
                            }
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
            }

            EventPayload::TaskAborted { task_id, reason: _ } => {
                let flow_id = event.metadata.correlation.flow_id;
                let mut candidate_flow_ids = Vec::new();

                if let Some(fid) = flow_id {
                    candidate_flow_ids.push(fid);
                } else {
                    for (fid, flow) in &self.flows {
                        if flow.task_executions.contains_key(task_id) {
                            candidate_flow_ids.push(*fid);
                        }
                    }
                }

                for fid in candidate_flow_ids {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = TaskExecState::Failed;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
            }

            EventPayload::HumanOverride {
                task_id,
                override_type: _,
                decision,
                reason: _,
                user: _,
            } => {
                let flow_id = event.metadata.correlation.flow_id;
                let mut candidate_flow_ids = Vec::new();

                if let Some(fid) = flow_id {
                    candidate_flow_ids.push(fid);
                } else {
                    for (fid, flow) in &self.flows {
                        if flow.task_executions.contains_key(task_id) {
                            candidate_flow_ids.push(*fid);
                        }
                    }
                }

                let new_state = if decision == "pass" {
                    TaskExecState::Success
                } else {
                    TaskExecState::Failed
                };

                for fid in candidate_flow_ids {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = new_state;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
            }

            EventPayload::MergePrepared {
                flow_id,
                target_branch,
                conflicts,
            } => {
                self.merge_states.insert(
                    *flow_id,
                    MergeState {
                        flow_id: *flow_id,
                        status: MergeStatus::Prepared,
                        target_branch: target_branch.clone(),
                        conflicts: conflicts.clone(),
                        commits: Vec::new(),
                        updated_at: timestamp,
                    },
                );
            }
            EventPayload::MergeApproved { flow_id, user: _ } => {
                if let Some(ms) = self.merge_states.get_mut(flow_id) {
                    ms.status = MergeStatus::Approved;
                    ms.updated_at = timestamp;
                }
            }
            EventPayload::MergeCompleted { flow_id, commits } => {
                if let Some(ms) = self.merge_states.get_mut(flow_id) {
                    ms.status = MergeStatus::Completed;
                    commits.clone_into(&mut ms.commits);
                    ms.updated_at = timestamp;
                }

                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Merged;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::WorktreeCleanupPerformed { .. } => {}
        }
    }

    /// Replays a sequence of events to produce state.
    /// Deterministic: same events → same state.
    #[must_use]
    pub fn replay(events: &[Event]) -> Self {
        let mut state = Self::new();
        for event in events {
            state.apply_mut(event);
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::CorrelationIds;

    #[test]
    fn replay_is_deterministic() {
        let project_id = Uuid::new_v4();
        let events = vec![
            Event::new(
                EventPayload::ProjectCreated {
                    id: project_id,
                    name: "test".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::ProjectUpdated {
                    id: project_id,
                    name: Some("updated".to_string()),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ),
        ];

        let state1 = AppState::replay(&events);
        let state2 = AppState::replay(&events);

        assert_eq!(state1.projects.len(), state2.projects.len());
        assert_eq!(
            state1.projects.get(&project_id).unwrap().name,
            state2.projects.get(&project_id).unwrap().name
        );
    }

    #[test]
    fn replay_is_idempotent() {
        let project_id = Uuid::new_v4();
        let events = vec![Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "test".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        )];

        let state1 = AppState::replay(&events);
        let state2 = AppState::replay(&events);

        assert_eq!(state1.projects.len(), 1);
        assert_eq!(state2.projects.len(), 1);
    }

    #[test]
    fn task_lifecycle() {
        let project_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();

        let events = vec![
            Event::new(
                EventPayload::ProjectCreated {
                    id: project_id,
                    name: "proj".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::TaskCreated {
                    id: task_id,
                    project_id,
                    title: "task1".to_string(),
                    description: None,
                    scope: None,
                },
                CorrelationIds::for_task(project_id, task_id),
            ),
            Event::new(
                EventPayload::TaskClosed {
                    id: task_id,
                    reason: None,
                },
                CorrelationIds::for_task(project_id, task_id),
            ),
        ];

        let state = AppState::replay(&events);
        let task = state.tasks.get(&task_id).unwrap();

        assert_eq!(task.state, TaskState::Closed);
    }
}
