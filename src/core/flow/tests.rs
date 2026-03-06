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
    assert!(flow.start().is_err());
}

#[test]
fn task_execution_transitions() {
    let mut exec = TaskExecution::new(Uuid::new_v4());
    assert_eq!(exec.state, TaskExecState::Pending);
    exec.transition(TaskExecState::Ready).unwrap();
    exec.transition(TaskExecState::Running).unwrap();
    assert_eq!(exec.attempt_count, 1);
    exec.transition(TaskExecState::Verifying).unwrap();
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
    assert!(exec.transition(TaskExecState::Success).is_err());
}

#[test]
fn flow_task_transition() {
    let mut flow = test_flow();
    let task_id = *flow.task_executions.keys().next().unwrap();
    flow.transition_task(task_id, TaskExecState::Ready).unwrap();
    assert_eq!(
        flow.get_task_execution(task_id).unwrap().state,
        TaskExecState::Ready
    );
}

#[test]
fn tasks_in_state() {
    let mut flow = test_flow();
    let task_ids: Vec<_> = flow.task_executions.keys().copied().collect();
    flow.transition_task(task_ids[0], TaskExecState::Ready)
        .unwrap();
    flow.transition_task(task_ids[1], TaskExecState::Ready)
        .unwrap();
    assert_eq!(flow.tasks_in_state(TaskExecState::Ready).len(), 2);
    assert_eq!(flow.tasks_in_state(TaskExecState::Pending).len(), 1);
}

#[test]
fn flow_completion_requires_terminal_tasks() {
    let mut flow = test_flow();
    flow.start().unwrap();
    assert!(flow.complete().is_err());
}

#[test]
fn flow_serialization() {
    let flow = test_flow();
    let json = serde_json::to_string(&flow).unwrap();
    let restored: TaskFlow = serde_json::from_str(&json).unwrap();
    assert_eq!(flow.id, restored.id);
    assert_eq!(flow.state, restored.state);
}
