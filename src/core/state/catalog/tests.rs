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
    let artifact_key = governance_artifact_key(Some(project_id), "project", "constitution", "main");
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
