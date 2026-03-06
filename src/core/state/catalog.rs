use super::*;

impl AppState {
    pub(super) fn apply_catalog_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
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
                        constitution_digest: None,
                        constitution_schema_version: None,
                        constitution_version: None,
                        constitution_updated_at: None,
                    },
                );
                true
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
                true
            }
            EventPayload::ProjectDeleted { project_id } => {
                self.apply_project_deleted(*project_id);
                true
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
                    let configured = project_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                        *max_parallel_tasks,
                    );
                    project.runtime = Some(configured.clone());
                    project.runtime_defaults.worker = Some(configured);
                    project.updated_at = timestamp;
                }
                true
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
                    let configured = project_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                        *max_parallel_tasks,
                    );
                    project
                        .runtime_defaults
                        .set(*role, Some(configured.clone()));
                    if *role == RuntimeRole::Worker {
                        project.runtime = Some(configured);
                    }
                    project.updated_at = timestamp;
                }
                true
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
                self.global_runtime_defaults.set(
                    *role,
                    Some(project_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                        *max_parallel_tasks,
                    )),
                );
                true
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
                true
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
                true
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
                    let configured = task_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                    );
                    task.runtime_override = Some(configured.clone());
                    task.runtime_overrides.worker = Some(configured);
                    task.updated_at = timestamp;
                }
                true
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
                    let configured = task_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                    );
                    task.runtime_overrides.set(*role, Some(configured.clone()));
                    if *role == RuntimeRole::Worker {
                        task.runtime_override = Some(configured);
                    }
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskRuntimeCleared { task_id } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.runtime_override = None;
                    task.runtime_overrides.worker = None;
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskRuntimeRoleCleared { task_id, role } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.runtime_overrides.set(*role, None);
                    if *role == RuntimeRole::Worker {
                        task.runtime_override = None;
                    }
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskRunModeSet { task_id, mode } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.run_mode = *mode;
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskClosed { id, reason: _ } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    task.state = TaskState::Closed;
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskDeleted {
                task_id,
                project_id: _,
            } => {
                self.tasks.remove(task_id);
                true
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
                true
            }
            EventPayload::RepositoryDetached { project_id, name } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    project.repositories.retain(|r| r.name != *name);
                    project.updated_at = timestamp;
                }
                true
            }
            EventPayload::GovernanceProjectStorageInitialized {
                project_id,
                schema_version,
                projection_version,
                root_path,
            } => {
                self.governance_projects.insert(
                    *project_id,
                    GovernanceProjectStorage {
                        project_id: *project_id,
                        schema_version: schema_version.clone(),
                        projection_version: *projection_version,
                        root_path: root_path.clone(),
                        initialized_at: timestamp,
                    },
                );
                true
            }
            EventPayload::GovernanceArtifactUpserted {
                project_id,
                scope,
                artifact_kind,
                artifact_key,
                path,
                revision,
                schema_version,
                projection_version,
            } => {
                self.governance_artifacts.insert(
                    governance_artifact_key(*project_id, scope, artifact_kind, artifact_key),
                    GovernanceArtifact {
                        project_id: *project_id,
                        scope: scope.clone(),
                        artifact_kind: artifact_kind.clone(),
                        artifact_key: artifact_key.clone(),
                        path: path.clone(),
                        revision: *revision,
                        schema_version: schema_version.clone(),
                        projection_version: *projection_version,
                        updated_at: timestamp,
                    },
                );
                true
            }
            EventPayload::GovernanceArtifactDeleted {
                project_id,
                scope,
                artifact_kind,
                artifact_key,
                path: _,
                schema_version: _,
                projection_version: _,
            } => {
                self.governance_artifacts.remove(&governance_artifact_key(
                    *project_id,
                    scope,
                    artifact_kind,
                    artifact_key,
                ));
                true
            }
            EventPayload::GovernanceAttachmentLifecycleUpdated {
                project_id,
                task_id,
                artifact_kind,
                artifact_key,
                attached,
                schema_version,
                projection_version,
            } => {
                let key = format!("{project_id}::{task_id}::{artifact_kind}::{artifact_key}");
                self.governance_attachments.insert(
                    key,
                    GovernanceAttachment {
                        project_id: *project_id,
                        task_id: *task_id,
                        artifact_kind: artifact_kind.clone(),
                        artifact_key: artifact_key.clone(),
                        attached: *attached,
                        schema_version: schema_version.clone(),
                        projection_version: *projection_version,
                        updated_at: timestamp,
                    },
                );
                true
            }
            EventPayload::GovernanceStorageMigrated {
                project_id,
                from_layout,
                to_layout,
                migrated_paths,
                rollback_hint,
                schema_version,
                projection_version,
            } => {
                self.governance_migrations.push(GovernanceMigration {
                    project_id: *project_id,
                    from_layout: from_layout.clone(),
                    to_layout: to_layout.clone(),
                    migrated_paths: migrated_paths.clone(),
                    rollback_hint: rollback_hint.clone(),
                    schema_version: schema_version.clone(),
                    projection_version: *projection_version,
                    migrated_at: timestamp,
                });
                true
            }
            EventPayload::ConstitutionInitialized {
                project_id,
                schema_version,
                constitution_version,
                digest,
                ..
            }
            | EventPayload::ConstitutionUpdated {
                project_id,
                schema_version,
                constitution_version,
                digest,
                ..
            } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    project.constitution_digest = Some(digest.clone());
                    project.constitution_schema_version = Some(schema_version.clone());
                    project.constitution_version = Some(*constitution_version);
                    project.constitution_updated_at = Some(timestamp);
                    project.updated_at = timestamp;
                }
                true
            }
            _ => false,
        }
    }

    fn apply_project_deleted(&mut self, project_id: Uuid) {
        self.projects.remove(&project_id);

        let flow_ids: std::collections::HashSet<Uuid> = self
            .flows
            .values()
            .filter(|flow| flow.project_id == project_id)
            .map(|flow| flow.id)
            .collect();

        self.tasks.retain(|_, task| task.project_id != project_id);
        self.graphs
            .retain(|_, graph| graph.project_id != project_id);
        self.flows.retain(|_, flow| flow.project_id != project_id);
        self.flow_runtime_defaults
            .retain(|flow_id, _| !flow_ids.contains(flow_id));
        self.merge_states
            .retain(|flow_id, _| !flow_ids.contains(flow_id));
        self.attempts
            .retain(|_, attempt| !flow_ids.contains(&attempt.flow_id));
    }
}

fn governance_artifact_key(
    project_id: Option<Uuid>,
    scope: &str,
    artifact_kind: &str,
    artifact_key: &str,
) -> String {
    let project_key = project_id.map_or_else(|| "global".to_string(), |id| id.to_string());
    format!("{project_key}::{scope}::{artifact_kind}::{artifact_key}")
}

fn project_runtime_config(
    adapter_name: &str,
    binary_path: &str,
    model: &Option<String>,
    args: &[String],
    env: &HashMap<String, String>,
    timeout_ms: u64,
    max_parallel_tasks: u16,
) -> ProjectRuntimeConfig {
    ProjectRuntimeConfig {
        adapter_name: adapter_name.to_string(),
        binary_path: binary_path.to_string(),
        model: model.clone(),
        args: args.to_vec(),
        env: env.clone(),
        timeout_ms,
        max_parallel_tasks,
    }
}

fn task_runtime_config(
    adapter_name: &str,
    binary_path: &str,
    model: &Option<String>,
    args: &[String],
    env: &HashMap<String, String>,
    timeout_ms: u64,
) -> TaskRuntimeConfig {
    TaskRuntimeConfig {
        adapter_name: adapter_name.to_string(),
        binary_path: binary_path.to_string(),
        model: model.clone(),
        args: args.to_vec(),
        env: env.clone(),
        timeout_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::CorrelationIds;

    #[test]
    fn project_deleted_cascades_related_runtime_and_flow_state() {
        let project_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let graph_id = Uuid::new_v4();
        let flow_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();
        let state = AppState::replay(&vec![
            Event::new(
                EventPayload::ProjectCreated {
                    id: project_id,
                    name: "proj".into(),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::TaskCreated {
                    id: task_id,
                    project_id,
                    title: "task".into(),
                    description: None,
                    scope: None,
                },
                CorrelationIds::for_task(project_id, task_id),
            ),
            Event::new(
                EventPayload::TaskGraphCreated {
                    graph_id,
                    project_id,
                    name: "graph".into(),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::TaskFlowCreated {
                    flow_id,
                    graph_id,
                    project_id,
                    name: None,
                    task_ids: vec![task_id],
                },
                CorrelationIds::for_flow(project_id, flow_id),
            ),
            Event::new(
                EventPayload::TaskFlowRuntimeConfigured {
                    flow_id,
                    role: RuntimeRole::Worker,
                    adapter_name: "native".into(),
                    binary_path: "hm".into(),
                    model: None,
                    args: Vec::new(),
                    env: HashMap::new(),
                    timeout_ms: 1000,
                    max_parallel_tasks: 1,
                },
                CorrelationIds::for_flow(project_id, flow_id),
            ),
            Event::new(
                EventPayload::AttemptStarted {
                    flow_id,
                    task_id,
                    attempt_id,
                    attempt_number: 1,
                },
                CorrelationIds::for_flow(project_id, flow_id),
            ),
            Event::new(
                EventPayload::MergePrepared {
                    flow_id,
                    target_branch: Some("main".into()),
                    conflicts: Vec::new(),
                },
                CorrelationIds::for_flow(project_id, flow_id),
            ),
            Event::new(
                EventPayload::ProjectDeleted { project_id },
                CorrelationIds::for_project(project_id),
            ),
        ]);

        assert!(!state.projects.contains_key(&project_id));
        assert!(!state.tasks.contains_key(&task_id));
        assert!(!state.graphs.contains_key(&graph_id));
        assert!(!state.flows.contains_key(&flow_id));
        assert!(!state.flow_runtime_defaults.contains_key(&flow_id));
        assert!(!state.merge_states.contains_key(&flow_id));
        assert!(!state.attempts.contains_key(&attempt_id));
    }

    #[test]
    fn task_runtime_role_and_repository_events_are_projected() {
        let project_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let state = AppState::replay(&vec![
            Event::new(
                EventPayload::ProjectCreated {
                    id: project_id,
                    name: "proj".into(),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::RepositoryAttached {
                    project_id,
                    path: "/tmp/repo".into(),
                    name: "repo".into(),
                    access_mode: RepoAccessMode::ReadWrite,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::TaskCreated {
                    id: task_id,
                    project_id,
                    title: "task".into(),
                    description: Some("desc".into()),
                    scope: None,
                },
                CorrelationIds::for_task(project_id, task_id),
            ),
            Event::new(
                EventPayload::TaskRuntimeRoleConfigured {
                    task_id,
                    role: RuntimeRole::Validator,
                    adapter_name: "native".into(),
                    binary_path: "hm".into(),
                    model: Some("gpt".into()),
                    args: vec!["--fast".into()],
                    env: HashMap::new(),
                    timeout_ms: 2500,
                },
                CorrelationIds::for_task(project_id, task_id),
            ),
            Event::new(
                EventPayload::TaskRunModeSet {
                    task_id,
                    mode: RunMode::Manual,
                },
                CorrelationIds::for_task(project_id, task_id),
            ),
            Event::new(
                EventPayload::RepositoryDetached {
                    project_id,
                    name: "repo".into(),
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::TaskClosed {
                    id: task_id,
                    reason: None,
                },
                CorrelationIds::for_task(project_id, task_id),
            ),
        ]);

        let project = state.projects.get(&project_id).expect("project");
        let task = state.tasks.get(&task_id).expect("task");
        assert!(project.repositories.is_empty());
        assert_eq!(task.run_mode, RunMode::Manual);
        assert_eq!(task.state, TaskState::Closed);
        assert!(task.runtime_override.is_none());
        assert_eq!(
            task.runtime_overrides
                .validator
                .as_ref()
                .map(|cfg| cfg.adapter_name.as_str()),
            Some("native")
        );
    }

    #[test]
    fn governance_and_constitution_events_update_projections() {
        let project_id = Uuid::new_v4();
        let artifact_key =
            governance_artifact_key(Some(project_id), "project", "constitution", "main");
        let state = AppState::replay(&vec![
            Event::new(
                EventPayload::ProjectCreated {
                    id: project_id,
                    name: "proj".into(),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::GovernanceProjectStorageInitialized {
                    project_id,
                    schema_version: "gov.v1".into(),
                    projection_version: 3,
                    root_path: "/gov".into(),
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::GovernanceArtifactUpserted {
                    project_id: Some(project_id),
                    scope: "project".into(),
                    artifact_kind: "constitution".into(),
                    artifact_key: "main".into(),
                    path: "/gov/constitution.md".into(),
                    revision: 7,
                    schema_version: "gov.v1".into(),
                    projection_version: 3,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::ConstitutionInitialized {
                    project_id,
                    path: "/gov/constitution.md".into(),
                    schema_version: "constitution.v1".into(),
                    constitution_version: 11,
                    digest: "abc123".into(),
                    revision: 7,
                    actor: "agent".into(),
                    mutation_intent: "initialize".into(),
                    confirmed: true,
                },
                CorrelationIds::for_project(project_id),
            ),
            Event::new(
                EventPayload::GovernanceArtifactDeleted {
                    project_id: Some(project_id),
                    scope: "project".into(),
                    artifact_kind: "constitution".into(),
                    artifact_key: "main".into(),
                    path: "/gov/constitution.md".into(),
                    schema_version: "gov.v1".into(),
                    projection_version: 3,
                },
                CorrelationIds::for_project(project_id),
            ),
        ]);

        let project = state.projects.get(&project_id).expect("project");
        let storage = state.governance_projects.get(&project_id).expect("storage");
        assert_eq!(storage.root_path, "/gov");
        assert_eq!(project.constitution_digest.as_deref(), Some("abc123"));
        assert_eq!(
            project.constitution_schema_version.as_deref(),
            Some("constitution.v1")
        );
        assert_eq!(project.constitution_version, Some(11));
        assert!(!state.governance_artifacts.contains_key(&artifact_key));
    }
}
