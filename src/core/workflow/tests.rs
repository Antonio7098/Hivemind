use super::*;

#[test]
fn workflow_run_state_transitions_are_constrained() {
    assert!(WorkflowRunState::Created.can_transition_to(WorkflowRunState::Running));
    assert!(WorkflowRunState::Running.can_transition_to(WorkflowRunState::Paused));
    assert!(!WorkflowRunState::Completed.can_transition_to(WorkflowRunState::Running));
}

#[test]
fn workflow_step_state_transitions_are_constrained() {
    assert!(WorkflowStepState::Pending.can_transition_to(WorkflowStepState::Ready));
    assert!(WorkflowStepState::Running.can_transition_to(WorkflowStepState::Succeeded));
    assert!(!WorkflowStepState::Succeeded.can_transition_to(WorkflowStepState::Running));
}

#[test]
fn workflow_run_initializes_step_runs_from_definition() {
    let project_id = Uuid::new_v4();
    let mut definition = WorkflowDefinition::new(project_id, "demo", None);
    let mut step = WorkflowStepDefinition::new("leaf", WorkflowStepKind::Task);
    step.depends_on = Vec::new();
    let step_id = step.id;
    definition.add_step(step);

    let run = WorkflowRun::new_root(&definition);

    assert_eq!(run.workflow_id, definition.id);
    assert_eq!(run.root_workflow_run_id, run.id);
    assert_eq!(run.step_runs.len(), 1);
    assert_eq!(
        run.step_runs.get(&step_id).unwrap().state,
        WorkflowStepState::Pending
    );
}

#[test]
fn workflow_run_can_complete_only_after_all_steps_are_terminal() {
    let project_id = Uuid::new_v4();
    let mut definition = WorkflowDefinition::new(project_id, "demo", None);
    let step = WorkflowStepDefinition::new("leaf", WorkflowStepKind::Task);
    let step_id = step.id;
    definition.add_step(step);

    let mut run = WorkflowRun::new_root(&definition);
    assert!(!run.can_complete());

    run.step_runs.get_mut(&step_id).unwrap().state = WorkflowStepState::Succeeded;
    assert!(run.can_complete());
}

#[test]
fn workflow_definition_metadata_updates_in_place() {
    let mut definition = WorkflowDefinition::new(Uuid::new_v4(), "demo", None);
    definition.update_metadata(Some("renamed".to_string()), Some(Some("desc".to_string())));

    assert_eq!(definition.name, "renamed");
    assert_eq!(definition.description.as_deref(), Some("desc"));
}
