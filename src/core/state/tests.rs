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
