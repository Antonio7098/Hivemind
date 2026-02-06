//! Project registry for managing projects via events.
//!
//! The registry derives project state from events and provides
//! operations that emit new events.

use crate::core::error::{HivemindError, Result};
use crate::core::events::{CorrelationIds, Event, EventPayload};
use crate::core::flow::{FlowState, TaskExecState, TaskFlow};
use crate::core::graph::{GraphState, GraphTask, SuccessCriteria, TaskGraph};
use crate::core::scope::{RepoAccessMode, Scope};
use crate::core::state::{AppState, Project, Task, TaskState};
use crate::storage::event_store::{EventFilter, EventStore, FileEventStore};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Configuration for the registry.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Base directory for hivemind data.
    pub data_dir: PathBuf,
}

impl RegistryConfig {
    /// Creates a new config with default data directory.
    #[must_use]
    pub fn default_dir() -> Self {
        let data_dir = dirs::home_dir()
            .map(|h| h.join(".hivemind"))
            .unwrap_or_else(|| PathBuf::from(".hivemind"));
        Self { data_dir }
    }

    /// Creates a config with custom data directory.
    #[must_use]
    pub fn with_dir(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Returns the path to the global events file.
    #[must_use]
    pub fn events_path(&self) -> PathBuf {
        self.data_dir.join("events.jsonl")
    }
}

/// The project registry manages projects via event sourcing.
pub struct Registry {
    store: Arc<dyn EventStore>,
    config: RegistryConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphValidationResult {
    pub graph_id: Uuid,
    pub valid: bool,
    pub issues: Vec<String>,
}

impl Registry {
    /// Opens or creates a registry at the default location.
    ///
    /// # Errors
    /// Returns an error if the event store cannot be opened.
    pub fn open() -> Result<Self> {
        Self::open_with_config(RegistryConfig::default_dir())
    }

    /// Opens or creates a registry with custom config.
    ///
    /// # Errors
    /// Returns an error if the event store cannot be opened.
    pub fn open_with_config(config: RegistryConfig) -> Result<Self> {
        let store = FileEventStore::open(config.events_path()).map_err(|e| {
            HivemindError::system("store_open_failed", e.to_string(), "registry:open")
        })?;

        Ok(Self {
            store: Arc::new(store),
            config,
        })
    }

    /// Creates a registry with a custom event store (for testing).
    #[must_use]
    pub fn with_store(store: Arc<dyn EventStore>, config: RegistryConfig) -> Self {
        Self { store, config }
    }

    /// Gets the current state by replaying all events.
    ///
    /// # Errors
    /// Returns an error if events cannot be read.
    pub fn state(&self) -> Result<AppState> {
        let events = self.store.read_all().map_err(|e| {
            HivemindError::system("state_read_failed", e.to_string(), "registry:state")
        })?;
        Ok(AppState::replay(&events))
    }

    /// Lists events in the store.
    ///
    /// # Errors
    /// Returns an error if events cannot be read.
    pub fn list_events(&self, project_id: Option<Uuid>, limit: usize) -> Result<Vec<Event>> {
        let mut filter = EventFilter::all();
        filter.project_id = project_id;
        filter.limit = Some(limit);

        self.store.read(&filter).map_err(|e| {
            HivemindError::system(
                "event_read_failed",
                e.to_string(),
                "registry:list_events",
            )
        })
    }

    pub fn read_events(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        self.store.read(filter).map_err(|e| {
            HivemindError::system(
                "event_read_failed",
                e.to_string(),
                "registry:read_events",
            )
        })
    }

    /// Gets a specific event by ID.
    ///
    /// # Errors
    /// Returns an error if the event cannot be read or is not found.
    pub fn get_event(&self, event_id: &str) -> Result<Event> {
        let id = Uuid::parse_str(event_id).map_err(|_| {
            HivemindError::user(
                "invalid_event_id",
                format!("'{}' is not a valid event ID", event_id),
                "registry:get_event",
            )
        })?;

        let events = self.store.read_all().map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:get_event")
        })?;

        events
            .into_iter()
            .find(|e| e.metadata.id.as_uuid() == id)
            .ok_or_else(|| {
                HivemindError::user(
                    "event_not_found",
                    format!("Event '{}' not found", event_id),
                    "registry:get_event",
                )
            })
    }

    /// Creates a new project.
    ///
    /// # Errors
    /// Returns an error if a project with that name already exists.
    pub fn create_project(&self, name: &str, description: Option<&str>) -> Result<Project> {
        let state = self.state()?;

        // Check for duplicate name
        if state.projects.values().any(|p| p.name == name) {
            return Err(HivemindError::user(
                "project_exists",
                format!("Project '{}' already exists", name),
                "registry:create_project",
            )
            .with_hint("Choose a different project name"));
        }

        let id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::ProjectCreated {
                id,
                name: name.to_string(),
                description: description.map(String::from),
            },
            CorrelationIds::for_project(id),
        );

        self.store.append(event.clone()).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:create_project",
            )
        })?;

        // Return the created project by replaying
        let new_state = self.state()?;
        new_state.projects.get(&id).cloned().ok_or_else(|| {
            HivemindError::system(
                "project_not_found_after_create",
                "Project was not found after creation",
                "registry:create_project",
            )
        })
    }

    /// Lists all projects.
    ///
    /// # Errors
    /// Returns an error if state cannot be read.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let state = self.state()?;
        let mut projects: Vec<_> = state.projects.into_values().collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(projects)
    }

    /// Gets a project by ID or name.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn get_project(&self, id_or_name: &str) -> Result<Project> {
        let state = self.state()?;

        // Try parsing as UUID first
        if let Ok(id) = Uuid::parse_str(id_or_name) {
            if let Some(project) = state.projects.get(&id) {
                return Ok(project.clone());
            }
        }

        // Search by name
        state
            .projects
            .values()
            .find(|p| p.name == id_or_name)
            .cloned()
            .ok_or_else(|| {
                HivemindError::user(
                    "project_not_found",
                    format!("Project '{}' not found", id_or_name),
                    "registry:get_project",
                )
                .with_hint("Use 'hivemind project list' to see available projects")
            })
    }

    /// Updates a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn update_project(
        &self,
        id_or_name: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Project> {
        let project = self.get_project(id_or_name)?;

        // Check for name conflict if changing name
        if let Some(new_name) = name {
            let state = self.state()?;
            if state
                .projects
                .values()
                .any(|p| p.name == new_name && p.id != project.id)
            {
                return Err(HivemindError::user(
                    "project_name_conflict",
                    format!("Project name '{}' is already taken", new_name),
                    "registry:update_project",
                ));
            }
        }

        let event = Event::new(
            EventPayload::ProjectUpdated {
                id: project.id,
                name: name.map(String::from),
                description: description.map(String::from),
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:update_project",
            )
        })?;

        self.get_project(&project.id.to_string())
    }

    /// Returns the registry configuration.
    #[must_use]
    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

    /// Attaches a repository to a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found or the path is not a valid git repository.
    pub fn attach_repo(
        &self,
        id_or_name: &str,
        path: &str,
        name: Option<&str>,
        access_mode: RepoAccessMode,
    ) -> Result<Project> {
        let project = self.get_project(id_or_name)?;
        let path_buf = std::path::PathBuf::from(path);

        // Validate it's a git repository
        let git_dir = path_buf.join(".git");
        if !git_dir.exists() {
            return Err(HivemindError::git(
                "not_a_git_repo",
                format!("'{}' is not a git repository", path),
                "registry:attach_repo",
            )
            .with_hint("Provide a path to a directory containing a .git folder"));
        }

        // Check if already attached
        let canonical_path = path_buf
            .canonicalize()
            .map_err(|e| {
                HivemindError::system(
                    "path_canonicalize_failed",
                    e.to_string(),
                    "registry:attach_repo",
                )
            })?
            .to_string_lossy()
            .to_string();

        if project
            .repositories
            .iter()
            .any(|r| r.path == canonical_path)
        {
            return Err(HivemindError::user(
                "repo_already_attached",
                format!("Repository '{}' is already attached to this project", path),
                "registry:attach_repo",
            ));
        }

        // Derive repo name from arg or path
        let repo_name = name
            .map(ToString::to_string)
            .or_else(|| path_buf.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "repo".to_string());

        if project.repositories.iter().any(|r| r.name == repo_name) {
            return Err(HivemindError::user(
                "repo_name_already_attached",
                format!(
                    "Repository name '{}' is already attached to project '{}'",
                    repo_name, project.name
                ),
                "registry:attach_repo",
            )
            .with_hint("Use --name to provide a different repository name"));
        }

        let event = Event::new(
            EventPayload::RepositoryAttached {
                project_id: project.id,
                path: canonical_path,
                name: repo_name,
                access_mode,
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:attach_repo")
        })?;

        self.get_project(&project.id.to_string())
    }

    /// Detaches a repository from a project.
    ///
    /// # Errors
    /// Returns an error if the project or repository is not found.
    pub fn detach_repo(&self, id_or_name: &str, repo_name: &str) -> Result<Project> {
        let project = self.get_project(id_or_name)?;

        // Check if repo exists
        if !project.repositories.iter().any(|r| r.name == repo_name) {
            return Err(HivemindError::user(
                "repo_not_found",
                format!(
                    "Repository '{}' is not attached to project '{}'",
                    repo_name, project.name
                ),
                "registry:detach_repo",
            )
            .with_hint("Use 'hivemind project inspect' to see attached repositories"));
        }

        let event = Event::new(
            EventPayload::RepositoryDetached {
                project_id: project.id,
                name: repo_name.to_string(),
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:detach_repo")
        })?;

        self.get_project(&project.id.to_string())
    }

    // ========== Task Management ==========

    /// Creates a new task in a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn create_task(
        &self,
        project_id_or_name: &str,
        title: &str,
        description: Option<&str>,
        scope: Option<Scope>,
    ) -> Result<Task> {
        let project = self.get_project(project_id_or_name)?;

        let task_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::TaskCreated {
                id: task_id,
                project_id: project.id,
                title: title.to_string(),
                description: description.map(String::from),
                scope,
            },
            CorrelationIds::for_task(project.id, task_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:create_task")
        })?;

        self.get_task(&task_id.to_string())
    }

    /// Lists tasks in a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn list_tasks(
        &self,
        project_id_or_name: &str,
        state_filter: Option<TaskState>,
    ) -> Result<Vec<Task>> {
        let project = self.get_project(project_id_or_name)?;
        let state = self.state()?;

        let mut tasks: Vec<_> = state
            .tasks
            .into_values()
            .filter(|t| t.project_id == project.id)
            .filter(|t| state_filter.map_or(true, |s| t.state == s))
            .collect();

        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(tasks)
    }

    /// Gets a task by ID.
    ///
    /// # Errors
    /// Returns an error if the task is not found.
    pub fn get_task(&self, task_id: &str) -> Result<Task> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{}' is not a valid task ID", task_id),
                "registry:get_task",
            )
        })?;

        let state = self.state()?;
        state.tasks.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "task_not_found",
                format!("Task '{}' not found", task_id),
                "registry:get_task",
            )
            .with_hint("Use 'hivemind task list <project>' to see available tasks")
        })
    }

    /// Updates a task.
    ///
    /// # Errors
    /// Returns an error if the task is not found.
    pub fn update_task(
        &self,
        task_id: &str,
        title: Option<&str>,
        description: Option<&str>,
    ) -> Result<Task> {
        let task = self.get_task(task_id)?;

        let event = Event::new(
            EventPayload::TaskUpdated {
                id: task.id,
                title: title.map(String::from),
                description: description.map(String::from),
            },
            CorrelationIds::for_task(task.project_id, task.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:update_task")
        })?;

        self.get_task(task_id)
    }

    /// Closes a task.
    ///
    /// # Errors
    /// Returns an error if the task is not found or already closed.
    pub fn close_task(&self, task_id: &str, reason: Option<&str>) -> Result<Task> {
        let task = self.get_task(task_id)?;

        let state = self.state()?;
        let in_active_flow = state.flows.values().any(|f| {
            f.task_executions.contains_key(&task.id)
                && !matches!(f.state, FlowState::Completed | FlowState::Aborted)
        });
        if in_active_flow {
            return Err(HivemindError::user(
                "task_in_active_flow",
                "Task is part of an active flow",
                "registry:close_task",
            ));
        }

        if task.state == TaskState::Closed {
            // Idempotent: closing an already closed task is a no-op.
            return Ok(task);
        }

        let event = Event::new(
            EventPayload::TaskClosed {
                id: task.id,
                reason: reason.map(String::from),
            },
            CorrelationIds::for_task(task.project_id, task.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:close_task")
        })?;

        self.get_task(task_id)
    }

    pub fn get_graph(&self, graph_id: &str) -> Result<TaskGraph> {
        let id = Uuid::parse_str(graph_id).map_err(|_| {
            HivemindError::user(
                "invalid_graph_id",
                format!("'{}' is not a valid graph ID", graph_id),
                "registry:get_graph",
            )
        })?;

        let state = self.state()?;
        state.graphs.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "graph_not_found",
                format!("Graph '{}' not found", graph_id),
                "registry:get_graph",
            )
        })
    }

    pub fn create_graph(
        &self,
        project_id_or_name: &str,
        name: &str,
        from_tasks: &[Uuid],
    ) -> Result<TaskGraph> {
        let project = self.get_project(project_id_or_name)?;
        let state = self.state()?;

        let mut tasks_to_add = Vec::new();
        for tid in from_tasks {
            let task = state.tasks.get(tid).cloned().ok_or_else(|| {
                HivemindError::user(
                    "task_not_found",
                    format!("Task '{}' not found", tid),
                    "registry:create_graph",
                )
            })?;
            if task.state != TaskState::Open {
                return Err(HivemindError::user(
                    "task_not_open",
                    format!("Task '{}' is not open", tid),
                    "registry:create_graph",
                ));
            }
            tasks_to_add.push(task);
        }

        let graph_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::TaskGraphCreated {
                graph_id,
                project_id: project.id,
                name: name.to_string(),
                description: None,
            },
            CorrelationIds::for_graph(project.id, graph_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:create_graph")
        })?;

        for task in tasks_to_add {
            let graph_task = GraphTask {
                id: task.id,
                title: task.title,
                description: task.description,
                criteria: SuccessCriteria::new("Done"),
                retry_policy: Default::default(),
                scope: task.scope,
            };
            let event = Event::new(
                EventPayload::TaskAddedToGraph {
                    graph_id,
                    task: graph_task,
                },
                CorrelationIds::for_graph(project.id, graph_id),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system(
                    "event_append_failed",
                    e.to_string(),
                    "registry:create_graph",
                )
            })?;
        }

        self.get_graph(&graph_id.to_string())
    }

    pub fn add_graph_dependency(
        &self,
        graph_id: &str,
        from_task: &str,
        to_task: &str,
    ) -> Result<TaskGraph> {
        let gid = Uuid::parse_str(graph_id).map_err(|_| {
            HivemindError::user(
                "invalid_graph_id",
                format!("'{}' is not a valid graph ID", graph_id),
                "registry:add_graph_dependency",
            )
        })?;
        let from = Uuid::parse_str(from_task).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{}' is not a valid task ID", from_task),
                "registry:add_graph_dependency",
            )
        })?;
        let to = Uuid::parse_str(to_task).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{}' is not a valid task ID", to_task),
                "registry:add_graph_dependency",
            )
        })?;

        let state = self.state()?;
        let graph = state.graphs.get(&gid).cloned().ok_or_else(|| {
            HivemindError::user(
                "graph_not_found",
                format!("Graph '{}' not found", graph_id),
                "registry:add_graph_dependency",
            )
        })?;

        if graph.state != GraphState::Draft {
            return Err(HivemindError::user(
                "graph_immutable",
                format!("Graph '{}' is immutable", graph_id),
                "registry:add_graph_dependency",
            ));
        }

        if !graph.tasks.contains_key(&from) || !graph.tasks.contains_key(&to) {
            return Err(HivemindError::user(
                "task_not_in_graph",
                "One or more tasks are not in the graph",
                "registry:add_graph_dependency",
            ));
        }

        if graph
            .dependencies
            .get(&to)
            .map_or(false, |deps| deps.contains(&from))
        {
            return Ok(graph);
        }

        let mut graph_for_check = graph.clone();
        graph_for_check.add_dependency(to, from).map_err(|e| {
            HivemindError::user(
                "cycle_detected",
                e.to_string(),
                "registry:add_graph_dependency",
            )
        })?;

        let event = Event::new(
            EventPayload::DependencyAdded {
                graph_id: gid,
                from_task: from,
                to_task: to,
            },
            CorrelationIds::for_graph(graph.project_id, gid),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:add_graph_dependency",
            )
        })?;

        self.get_graph(graph_id)
    }

    fn validate_graph_issues(graph: &TaskGraph) -> Vec<String> {
        if graph.tasks.is_empty() {
            return vec!["Graph must contain at least one task".to_string()];
        }

        for (task_id, deps) in &graph.dependencies {
            if !graph.tasks.contains_key(task_id) {
                return vec![format!("Task not found: {}", task_id)];
            }
            for dep in deps {
                if !graph.tasks.contains_key(dep) {
                    return vec![format!("Task not found: {}", dep)];
                }
            }
        }

        fn has_cycle(graph: &TaskGraph) -> bool {
            use std::collections::HashSet;

            fn visit(
                graph: &TaskGraph,
                node: Uuid,
                visited: &mut HashSet<Uuid>,
                stack: &mut HashSet<Uuid>,
            ) -> bool {
                if stack.contains(&node) {
                    return true;
                }
                if visited.contains(&node) {
                    return false;
                }

                visited.insert(node);
                stack.insert(node);

                if let Some(deps) = graph.dependencies.get(&node) {
                    for dep in deps {
                        if visit(graph, *dep, visited, stack) {
                            return true;
                        }
                    }
                }

                stack.remove(&node);
                false
            }

            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            for node in graph.tasks.keys() {
                if visit(graph, *node, &mut visited, &mut stack) {
                    return true;
                }
            }
            false
        }

        if has_cycle(graph) {
            return vec!["Cycle detected in task dependencies".to_string()];
        }

        Vec::new()
    }

    pub fn validate_graph(&self, graph_id: &str) -> Result<GraphValidationResult> {
        let graph = self.get_graph(graph_id)?;
        let issues = Self::validate_graph_issues(&graph);
        Ok(GraphValidationResult {
            graph_id: graph.id,
            valid: issues.is_empty(),
            issues,
        })
    }

    pub fn get_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let id = Uuid::parse_str(flow_id).map_err(|_| {
            HivemindError::user(
                "invalid_flow_id",
                format!("'{}' is not a valid flow ID", flow_id),
                "registry:get_flow",
            )
        })?;

        let state = self.state()?;
        state.flows.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "flow_not_found",
                format!("Flow '{}' not found", flow_id),
                "registry:get_flow",
            )
        })
    }

    pub fn create_flow(&self, graph_id: &str, name: Option<&str>) -> Result<TaskFlow> {
        let graph = self.get_graph(graph_id)?;
        let issues = Self::validate_graph_issues(&graph);
        if !issues.is_empty() {
            return Err(HivemindError::user(
                "graph_invalid",
                "Graph validation failed",
                "registry:create_flow",
            )
            .with_context("graph_id", graph.id.to_string()));
        }

        let state = self.state()?;
        let has_active = state.flows.values().any(|f| {
            f.graph_id == graph.id && !matches!(f.state, FlowState::Completed | FlowState::Aborted)
        });
        if has_active {
            return Err(HivemindError::user(
                "graph_in_use",
                "Graph already used by an active flow",
                "registry:create_flow",
            )
            .with_context("graph_id", graph.id.to_string()));
        }

        let flow_id = Uuid::new_v4();
        let task_ids: Vec<Uuid> = graph.tasks.keys().copied().collect();
        let event = Event::new(
            EventPayload::TaskFlowCreated {
                flow_id,
                graph_id: graph.id,
                project_id: graph.project_id,
                name: name.map(String::from),
                task_ids,
            },
            CorrelationIds::for_graph_flow(graph.project_id, graph.id, flow_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:create_flow")
        })?;

        self.get_flow(&flow_id.to_string())
    }

    pub fn start_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        match flow.state {
            FlowState::Created => {}
            FlowState::Paused => {
                let event = Event::new(
                    EventPayload::TaskFlowResumed { flow_id: flow.id },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:start_flow",
                    )
                })?;
                return self.get_flow(flow_id);
            }
            FlowState::Running => {
                return Err(HivemindError::user(
                    "flow_already_running",
                    "Flow is already running",
                    "registry:start_flow",
                ));
            }
            FlowState::Completed => {
                return Err(HivemindError::user(
                    "flow_completed",
                    "Flow has already completed",
                    "registry:start_flow",
                ));
            }
            FlowState::Aborted => {
                return Err(HivemindError::user(
                    "flow_aborted",
                    "Flow was aborted",
                    "registry:start_flow",
                ));
            }
        }

        let event = Event::new(
            EventPayload::TaskFlowStarted { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:start_flow")
        })?;

        let state = self.state()?;
        if let Some(graph) = state.graphs.get(&flow.graph_id) {
            let ready = graph.root_tasks();
            for task_id in ready {
                let event = Event::new(
                    EventPayload::TaskReady {
                        flow_id: flow.id,
                        task_id,
                    },
                    CorrelationIds::for_graph_flow_task(
                        flow.project_id,
                        flow.graph_id,
                        flow.id,
                        task_id,
                    ),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:start_flow",
                    )
                })?;
            }
        }

        self.get_flow(flow_id)
    }

    pub fn pause_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;

        match flow.state {
            FlowState::Paused => return Ok(flow),
            FlowState::Running => {}
            _ => {
                return Err(HivemindError::user(
                    "flow_not_running",
                    "Flow is not in running state",
                    "registry:pause_flow",
                ));
            }
        }

        let running_tasks: Vec<Uuid> = flow.tasks_in_state(TaskExecState::Running);
        let event = Event::new(
            EventPayload::TaskFlowPaused {
                flow_id: flow.id,
                running_tasks,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:pause_flow")
        })?;

        self.get_flow(flow_id)
    }

    pub fn resume_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Paused {
            return Err(HivemindError::user(
                "flow_not_paused",
                "Flow is not paused",
                "registry:resume_flow",
            ));
        }

        let event = Event::new(
            EventPayload::TaskFlowResumed { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:resume_flow")
        })?;

        self.get_flow(flow_id)
    }

    pub fn abort_flow(
        &self,
        flow_id: &str,
        reason: Option<&str>,
        forced: bool,
    ) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state == FlowState::Aborted {
            return Ok(flow);
        }
        if flow.state == FlowState::Completed {
            return Err(HivemindError::user(
                "flow_already_terminal",
                "Flow is completed",
                "registry:abort_flow",
            ));
        }

        let event = Event::new(
            EventPayload::TaskFlowAborted {
                flow_id: flow.id,
                reason: reason.map(String::from),
                forced,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:abort_flow")
        })?;

        self.get_flow(flow_id)
    }

    pub fn retry_task(&self, task_id: &str, reset_count: bool) -> Result<TaskFlow> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{}' is not a valid task ID", task_id),
                "registry:retry_task",
            )
        })?;

        let state = self.state()?;
        let flow = state
            .flows
            .values()
            .find(|f| f.task_executions.contains_key(&id))
            .cloned()
            .ok_or_else(|| {
                HivemindError::user(
                    "task_not_in_flow",
                    "Task is not part of any flow",
                    "registry:retry_task",
                )
            })?;

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:retry_task",
            )
        })?;

        let max_retries = state
            .graphs
            .get(&flow.graph_id)
            .and_then(|g| g.tasks.get(&id))
            .map(|t| t.retry_policy.max_retries)
            .unwrap_or(3);
        let max_attempts = max_retries.saturating_add(1);
        if !reset_count && (exec.attempt_count as u32) >= max_attempts {
            return Err(HivemindError::user(
                "retry_limit_exceeded",
                "Retry limit exceeded",
                "registry:retry_task",
            ));
        }

        if exec.state != TaskExecState::Failed && exec.state != TaskExecState::Retry {
            return Err(HivemindError::user(
                "task_not_retriable",
                "Task is not in a retriable state",
                "registry:retry_task",
            ));
        }

        let event = Event::new(
            EventPayload::TaskRetryRequested {
                task_id: id,
                reset_count,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:retry_task")
        })?;

        self.get_flow(&flow.id.to_string())
    }

    pub fn abort_task(&self, task_id: &str, reason: Option<&str>) -> Result<TaskFlow> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{}' is not a valid task ID", task_id),
                "registry:abort_task",
            )
        })?;

        let state = self.state()?;
        let flow = state
            .flows
            .values()
            .find(|f| f.task_executions.contains_key(&id))
            .cloned()
            .ok_or_else(|| {
                HivemindError::user(
                    "task_not_in_flow",
                    "Task is not part of any flow",
                    "registry:abort_task",
                )
            })?;

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:abort_task",
            )
        })?;

        if exec.state == TaskExecState::Success {
            return Err(HivemindError::user(
                "task_already_terminal",
                "Task is already successful",
                "registry:abort_task",
            ));
        }

        if exec.state == TaskExecState::Failed {
            return Ok(flow);
        }

        let event = Event::new(
            EventPayload::TaskAborted {
                task_id: id,
                reason: reason.map(String::from),
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:abort_task")
        })?;

        self.get_flow(&flow.id.to_string())
    }

    pub fn verify_override(
        &self,
        task_id: &str,
        decision: &str,
        reason: &str,
    ) -> Result<TaskFlow> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{}' is not a valid task ID", task_id),
                "registry:verify_override",
            )
        })?;

        if decision != "pass" && decision != "fail" {
            return Err(HivemindError::user(
                "invalid_decision",
                "Decision must be 'pass' or 'fail'",
                "registry:verify_override",
            ));
        }

        let state = self.state()?;
        let flow = state
            .flows
            .values()
            .find(|f| f.task_executions.contains_key(&id))
            .cloned()
            .ok_or_else(|| {
                HivemindError::user(
                    "task_not_in_flow",
                    "Task is not part of any flow",
                    "registry:verify_override",
                )
            })?;

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:verify_override",
            )
        })?;

        if exec.state != TaskExecState::Verifying {
            return Err(HivemindError::user(
                "task_not_verifying",
                "Task is not in verification state",
                "registry:verify_override",
            ));
        }

        let event = Event::new(
            EventPayload::HumanOverride {
                task_id: id,
                override_type: "VERIFICATION_OVERRIDE".to_string(),
                decision: decision.to_string(),
                reason: reason.to_string(),
                user: None,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:verify_override",
            )
        })?;

        self.get_flow(&flow.id.to_string())
    }

    pub fn merge_prepare(
        &self,
        flow_id: &str,
        target_branch: Option<&str>,
    ) -> Result<crate::core::state::MergeState> {
        let flow = self.get_flow(flow_id)?;

        if flow.state != FlowState::Completed {
            return Err(HivemindError::user(
                "flow_not_completed",
                "Flow has not completed successfully",
                "registry:merge_prepare",
            ));
        }

        let state = self.state()?;
        if let Some(ms) = state.merge_states.get(&flow.id) {
            if ms.status == crate::core::state::MergeStatus::Prepared {
                return Ok(ms.clone());
            }
        }

        let event = Event::new(
            EventPayload::MergePrepared {
                flow_id: flow.id,
                target_branch: target_branch.map(String::from),
                conflicts: Vec::new(),
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:merge_prepare",
            )
        })?;

        let state = self.state()?;
        state
            .merge_states
            .get(&flow.id)
            .cloned()
            .ok_or_else(|| {
                HivemindError::system(
                    "merge_state_not_found",
                    "Merge state not found after prepare",
                    "registry:merge_prepare",
                )
            })
    }

    pub fn merge_approve(
        &self,
        flow_id: &str,
    ) -> Result<crate::core::state::MergeState> {
        let flow = self.get_flow(flow_id)?;

        let state = self.state()?;
        let ms = state.merge_states.get(&flow.id).ok_or_else(|| {
            HivemindError::user(
                "merge_not_prepared",
                "No merge preparation exists for this flow",
                "registry:merge_approve",
            )
        })?;

        if ms.status == crate::core::state::MergeStatus::Approved {
            return Ok(ms.clone());
        }

        if !ms.conflicts.is_empty() {
            return Err(HivemindError::user(
                "unresolved_conflicts",
                "Merge has unresolved conflicts",
                "registry:merge_approve",
            ));
        }

        let event = Event::new(
            EventPayload::MergeApproved {
                flow_id: flow.id,
                user: None,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:merge_approve")
        })?;

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after approve",
                "registry:merge_approve",
            )
        })
    }

    pub fn merge_execute(
        &self,
        flow_id: &str,
    ) -> Result<crate::core::state::MergeState> {
        let flow = self.get_flow(flow_id)?;

        let state = self.state()?;
        let ms = state.merge_states.get(&flow.id).ok_or_else(|| {
            HivemindError::user(
                "merge_not_prepared",
                "No merge preparation exists for this flow",
                "registry:merge_execute",
            )
        })?;

        if ms.status != crate::core::state::MergeStatus::Approved {
            return Err(HivemindError::user(
                "merge_not_approved",
                "Merge has not been approved",
                "registry:merge_execute",
            ));
        }

        let event = Event::new(
            EventPayload::MergeCompleted {
                flow_id: flow.id,
                commits: Vec::new(),
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:merge_execute",
            )
        })?;

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after execute",
                "registry:merge_execute",
            )
        })
    }

    pub fn replay_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let fid = Uuid::parse_str(flow_id).map_err(|_| {
            HivemindError::user(
                "invalid_flow_id",
                format!("'{}' is not a valid flow ID", flow_id),
                "registry:replay_flow",
            )
        })?;

        let filter = EventFilter {
            flow_id: Some(fid),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;
        if events.is_empty() {
            return Err(HivemindError::user(
                "flow_not_found",
                format!("No events found for flow '{}'", flow_id),
                "registry:replay_flow",
            ));
        }

        let all_events = self.store.read_all().map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:replay_flow")
        })?;
        let flow_related: Vec<Event> = all_events
            .into_iter()
            .filter(|e| {
                e.metadata.correlation.flow_id == Some(fid)
                    || match &e.payload {
                        EventPayload::TaskFlowCreated { flow_id: f, .. } => *f == fid,
                        _ => false,
                    }
            })
            .collect();

        let replayed = crate::core::state::AppState::replay(&flow_related);
        replayed.flows.get(&fid).cloned().ok_or_else(|| {
            HivemindError::user(
                "flow_not_found",
                format!("Flow '{}' not found in replayed state", flow_id),
                "registry:replay_flow",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::event_store::InMemoryEventStore;

    fn test_registry() -> Registry {
        let store = Arc::new(InMemoryEventStore::new());
        let config = RegistryConfig::with_dir(PathBuf::from("/tmp/test"));
        Registry::with_store(store, config)
    }

    #[test]
    fn create_and_list_projects() {
        let registry = test_registry();

        registry.create_project("project-a", None).unwrap();
        registry
            .create_project("project-b", Some("Description"))
            .unwrap();

        let projects = registry.list_projects().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "project-a");
        assert_eq!(projects[1].name, "project-b");
    }

    #[test]
    fn duplicate_project_name_rejected() {
        let registry = test_registry();

        registry.create_project("test", None).unwrap();
        let result = registry.create_project("test", None);

        assert!(result.is_err());
    }

    #[test]
    fn get_project_by_name() {
        let registry = test_registry();

        let created = registry.create_project("my-project", None).unwrap();
        let found = registry.get_project("my-project").unwrap();

        assert_eq!(created.id, found.id);
    }

    #[test]
    fn get_project_by_id() {
        let registry = test_registry();

        let created = registry.create_project("my-project", None).unwrap();
        let found = registry.get_project(&created.id.to_string()).unwrap();

        assert_eq!(created.id, found.id);
    }

    #[test]
    fn update_project() {
        let registry = test_registry();

        registry.create_project("old-name", None).unwrap();
        let updated = registry
            .update_project("old-name", Some("new-name"), Some("New desc"))
            .unwrap();

        assert_eq!(updated.name, "new-name");
        assert_eq!(updated.description, Some("New desc".to_string()));
    }

    #[test]
    fn create_and_list_tasks() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        registry.create_task("proj", "Task 1", None, None).unwrap();
        registry.create_task("proj", "Task 2", Some("Description"), None).unwrap();

        let tasks = registry.list_tasks("proj", None).unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn task_lifecycle() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let task = registry.create_task("proj", "My Task", None, None).unwrap();
        assert_eq!(task.state, TaskState::Open);

        let closed = registry.close_task(&task.id.to_string(), None).unwrap();
        assert_eq!(closed.state, TaskState::Closed);
    }

    #[test]
    fn filter_tasks_by_state() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Open Task", None, None).unwrap();
        let t2 = registry.create_task("proj", "Closed Task", None, None).unwrap();
        registry.close_task(&t2.id.to_string(), None).unwrap();

        let open_tasks = registry.list_tasks("proj", Some(TaskState::Open)).unwrap();
        assert_eq!(open_tasks.len(), 1);
        assert_eq!(open_tasks[0].id, t1.id);

        let closed_tasks = registry.list_tasks("proj", Some(TaskState::Closed)).unwrap();
        assert_eq!(closed_tasks.len(), 1);
    }

    #[test]
    fn update_task() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let task = registry
            .create_task("proj", "Original", None, None)
            .unwrap();
        let updated = registry
            .update_task(&task.id.to_string(), Some("Updated"), Some("Desc"))
            .unwrap();

        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.description, Some("Desc".to_string()));
    }

    #[test]
    fn graph_create_from_tasks_and_dependency() {
        let registry = test_registry();
        let proj = registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let t2 = registry.create_task("proj", "Task 2", None, None).unwrap();

        let graph = registry.create_graph("proj", "g1", &[t1.id, t2.id]).unwrap();
        assert_eq!(graph.project_id, proj.id);
        assert_eq!(graph.tasks.len(), 2);
        assert!(graph.tasks.contains_key(&t1.id));
        assert!(graph.tasks.contains_key(&t2.id));

        let updated = registry.add_graph_dependency(&graph.id.to_string(), &t1.id.to_string(), &t2.id.to_string()).unwrap();
        assert!(updated.dependencies.get(&t2.id).map_or(false, |deps| deps.contains(&t1.id)));

        let again = registry.add_graph_dependency(&graph.id.to_string(), &t1.id.to_string(), &t2.id.to_string()).unwrap();
        assert_eq!(again.dependencies.get(&t2.id), updated.dependencies.get(&t2.id));
    }

    #[test]
    fn flow_create_locks_graph_and_start_sets_ready() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let t2 = registry.create_task("proj", "Task 2", None, None).unwrap();

        let graph = registry.create_graph("proj", "g1", &[t1.id, t2.id]).unwrap();
        registry.add_graph_dependency(&graph.id.to_string(), &t1.id.to_string(), &t2.id.to_string()).unwrap();

        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let locked = registry.get_graph(&graph.id.to_string()).unwrap();
        assert_eq!(locked.state, GraphState::Locked);

        let started = registry.start_flow(&flow.id.to_string()).unwrap();
        assert_eq!(started.state, FlowState::Running);

        let started = registry.get_flow(&flow.id.to_string()).unwrap();
        assert_eq!(started.task_executions.get(&t1.id).map(|e| e.state), Some(TaskExecState::Ready));
        assert_eq!(started.task_executions.get(&t2.id).map(|e| e.state), Some(TaskExecState::Pending));
    }

    #[test]
    fn flow_pause_resume_abort_semantics() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();

        let flow = registry.start_flow(&flow.id.to_string()).unwrap();
        let flow = registry.pause_flow(&flow.id.to_string()).unwrap();
        assert_eq!(flow.state, FlowState::Paused);

        let flow2 = registry.pause_flow(&flow.id.to_string()).unwrap();
        assert_eq!(flow2.state, FlowState::Paused);

        let flow = registry.resume_flow(&flow.id.to_string()).unwrap();
        assert_eq!(flow.state, FlowState::Running);

        let flow = registry.abort_flow(&flow.id.to_string(), Some("stop"), true).unwrap();
        assert_eq!(flow.state, FlowState::Aborted);
        let flow2 = registry.abort_flow(&flow.id.to_string(), None, false).unwrap();
        assert_eq!(flow2.state, FlowState::Aborted);
    }

    #[test]
    fn task_abort_and_retry_affect_flow_task_state() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();

        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let flow = registry.abort_task(&t1.id.to_string(), Some("no"));
        assert!(flow.is_ok());
        let flow = registry.get_flow(&flow.unwrap().id.to_string()).unwrap();
        assert_eq!(
            flow.task_executions.get(&t1.id).map(|e| e.state),
            Some(TaskExecState::Failed)
        );

        let flow = registry.retry_task(&t1.id.to_string(), true).unwrap();
        assert_eq!(
            flow.task_executions.get(&t1.id).map(|e| e.state),
            Some(TaskExecState::Pending)
        );
        assert_eq!(
            flow.task_executions.get(&t1.id).map(|e| e.attempt_count),
            Some(0)
        );
    }

    #[test]
    fn close_task_disallowed_in_active_flow() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.close_task(&t1.id.to_string(), None);
        assert!(res.is_err());
    }

    #[test]
    fn retry_limit_exceeded_requires_reset_count() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        for _ in 0..4 {
            let event = Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id: t1.id,
                    from: TaskExecState::Ready,
                    to: TaskExecState::Running,
                },
                CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
            );
            registry.store.append(event).unwrap();
        }

        let event = Event::new(
            EventPayload::TaskExecutionStateChanged {
                flow_id: flow.id,
                task_id: t1.id,
                from: TaskExecState::Running,
                to: TaskExecState::Failed,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
        );
        registry.store.append(event).unwrap();

        assert!(registry.retry_task(&t1.id.to_string(), false).is_err());
        assert!(registry.retry_task(&t1.id.to_string(), true).is_ok());
    }

    fn setup_flow_with_verifying_task(registry: &Registry) -> (TaskFlow, Uuid) {
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let event = Event::new(
            EventPayload::TaskExecutionStateChanged {
                flow_id: flow.id,
                task_id: t1.id,
                from: TaskExecState::Ready,
                to: TaskExecState::Running,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
        );
        registry.store.append(event).unwrap();

        let event = Event::new(
            EventPayload::TaskExecutionStateChanged {
                flow_id: flow.id,
                task_id: t1.id,
                from: TaskExecState::Running,
                to: TaskExecState::Verifying,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
        );
        registry.store.append(event).unwrap();

        let flow = registry.get_flow(&flow.id.to_string()).unwrap();
        (flow, t1.id)
    }

    #[test]
    fn verify_override_pass_transitions_to_success() {
        let registry = test_registry();
        let (flow, t1_id) = setup_flow_with_verifying_task(&registry);

        let updated = registry.verify_override(&t1_id.to_string(), "pass", "looks good").unwrap();
        assert_eq!(
            updated.task_executions.get(&t1_id).map(|e| e.state),
            Some(TaskExecState::Success)
        );
    }

    #[test]
    fn verify_override_fail_transitions_to_failed() {
        let registry = test_registry();
        let (_, t1_id) = setup_flow_with_verifying_task(&registry);

        let updated = registry.verify_override(&t1_id.to_string(), "fail", "bad output").unwrap();
        assert_eq!(
            updated.task_executions.get(&t1_id).map(|e| e.state),
            Some(TaskExecState::Failed)
        );
    }

    #[test]
    fn verify_override_rejects_non_verifying_task() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.verify_override(&t1.id.to_string(), "pass", "reason");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "task_not_verifying");
    }

    #[test]
    fn verify_override_rejects_invalid_decision() {
        let registry = test_registry();
        let (_, t1_id) = setup_flow_with_verifying_task(&registry);

        let res = registry.verify_override(&t1_id.to_string(), "maybe", "reason");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "invalid_decision");
    }

    fn setup_completed_flow(registry: &Registry) -> TaskFlow {
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry
            .create_flow(&graph.id.to_string(), None)
            .unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        for (from, to) in [
            (TaskExecState::Ready, TaskExecState::Running),
            (TaskExecState::Running, TaskExecState::Verifying),
            (TaskExecState::Verifying, TaskExecState::Success),
        ] {
            let event = Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id: t1.id,
                    from,
                    to,
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    t1.id,
                ),
            );
            registry.store.append(event).unwrap();
        }

        let event = Event::new(
            EventPayload::TaskFlowCompleted { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        registry.store.append(event).unwrap();

        registry.get_flow(&flow.id.to_string()).unwrap()
    }

    #[test]
    fn merge_lifecycle_prepare_approve_execute() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

        let ms = registry
            .merge_prepare(&flow.id.to_string(), Some("main"))
            .unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Prepared);
        assert_eq!(ms.target_branch, Some("main".to_string()));

        let ms = registry.merge_approve(&flow.id.to_string()).unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Approved);

        let ms = registry.merge_execute(&flow.id.to_string()).unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Completed);
    }

    #[test]
    fn merge_prepare_idempotent() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

        let ms1 = registry.merge_prepare(&flow.id.to_string(), Some("main")).unwrap();
        let ms2 = registry.merge_prepare(&flow.id.to_string(), Some("main")).unwrap();
        assert_eq!(ms1.status, ms2.status);
    }

    #[test]
    fn merge_approve_idempotent() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

        registry
            .merge_prepare(&flow.id.to_string(), None)
            .unwrap();
        let ms1 = registry.merge_approve(&flow.id.to_string()).unwrap();
        let ms2 = registry.merge_approve(&flow.id.to_string()).unwrap();
        assert_eq!(ms1.status, ms2.status);
    }

    #[test]
    fn merge_prepare_rejects_non_completed_flow() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry
            .create_flow(&graph.id.to_string(), None)
            .unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.merge_prepare(&flow.id.to_string(), None);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "flow_not_completed");
    }

    #[test]
    fn merge_execute_rejects_unapproved() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

        registry
            .merge_prepare(&flow.id.to_string(), None)
            .unwrap();
        let res = registry.merge_execute(&flow.id.to_string());
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "merge_not_approved");
    }

    #[test]
    fn merge_execute_rejects_unprepared() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

        let res = registry.merge_execute(&flow.id.to_string());
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "merge_not_prepared");
    }

    #[test]
    fn replay_flow_reconstructs_state() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry
            .create_flow(&graph.id.to_string(), None)
            .unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let replayed = registry.replay_flow(&flow.id.to_string()).unwrap();
        assert_eq!(replayed.id, flow.id);
        assert_eq!(replayed.state, FlowState::Running);
        assert!(replayed.task_executions.contains_key(&t1.id));
    }

    #[test]
    fn replay_flow_not_found() {
        let registry = test_registry();
        let res = registry.replay_flow(&Uuid::new_v4().to_string());
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "flow_not_found");
    }

    #[test]
    fn read_events_with_flow_filter() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry
            .create_flow(&graph.id.to_string(), None)
            .unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let filter = EventFilter {
            flow_id: Some(flow.id),
            ..EventFilter::default()
        };
        let events = registry.read_events(&filter).unwrap();
        assert!(!events.is_empty());
        for ev in &events {
            assert_eq!(ev.metadata.correlation.flow_id, Some(flow.id));
        }
    }

    #[test]
    fn read_events_with_task_filter() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();

        let filter = EventFilter {
            task_id: Some(t1.id),
            ..EventFilter::default()
        };
        let events = registry.read_events(&filter).unwrap();
        assert_eq!(events.len(), 1);
    }
}
