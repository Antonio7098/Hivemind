use super::*;
use crate::core::events::CorrelationIds;
use crate::core::workflow::{
    WorkflowDefinition, WorkflowRun, WorkflowRunState, WorkflowStepDefinition, WorkflowStepKind,
    WorkflowStepState,
};

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

#[test]
fn workflow_replay_tracks_definition_and_run_state() {
    let project_id = Uuid::new_v4();
    let mut workflow = WorkflowDefinition::new(project_id, "demo-workflow", None);
    let step = WorkflowStepDefinition::new("root", WorkflowStepKind::Task);
    let step_id = step.id;
    workflow.add_step(step);
    let run = WorkflowRun::new_root(&workflow);

    let events = vec![
        Event::new(
            EventPayload::WorkflowDefinitionCreated {
                definition: workflow.clone(),
            },
            CorrelationIds::for_workflow(project_id, workflow.id),
        ),
        Event::new(
            EventPayload::WorkflowRunCreated { run: run.clone() },
            CorrelationIds::for_workflow_run(project_id, workflow.id, run.id),
        ),
        Event::new(
            EventPayload::WorkflowRunStarted {
                workflow_run_id: run.id,
            },
            CorrelationIds::for_workflow_run(project_id, workflow.id, run.id),
        ),
    ];

    let state = AppState::replay(&events);
    let replayed_run = state.workflow_runs.get(&run.id).unwrap();

    assert!(state.workflows.contains_key(&workflow.id));
    assert_eq!(replayed_run.state, WorkflowRunState::Running);
    assert_eq!(
        replayed_run.step_runs.get(&step_id).unwrap().state,
        WorkflowStepState::Ready
    );
}
