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
    assert!(WorkflowStepState::Running.can_transition_to(WorkflowStepState::Verifying));
    assert!(WorkflowStepState::Verifying.can_transition_to(WorkflowStepState::Retry));
    assert!(WorkflowStepState::Retry.can_transition_to(WorkflowStepState::Running));
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

#[test]
fn workflow_definition_root_step_ids_exclude_dependent_steps() {
    let project_id = Uuid::new_v4();
    let mut definition = WorkflowDefinition::new(project_id, "demo", None);
    let root = WorkflowStepDefinition::new("root", WorkflowStepKind::Task);
    let root_id = root.id;
    let mut dependent = WorkflowStepDefinition::new("dependent", WorkflowStepKind::Task);
    dependent.depends_on = vec![root_id];
    let dependent_id = dependent.id;
    definition.add_step(root);
    definition.add_step(dependent);

    let root_ids = definition.root_step_ids();

    assert_eq!(root_ids, vec![root_id]);
    assert!(!root_ids.contains(&dependent_id));
}

#[test]
fn workflow_context_initialization_and_patch_are_deterministic() {
    let value = WorkflowDataValue::new(
        "text/plain",
        1,
        serde_json::Value::String("alpha".to_string()),
    )
    .unwrap();
    let context = WorkflowContextState::initialize(
        "workflow_context".to_string(),
        1,
        BTreeMap::from([("goal".to_string(), value.clone())]),
        Utc::now(),
        "init",
    );

    assert_eq!(context.initialization_inputs["goal"], value);
    assert_eq!(context.current_snapshot.revision, 1);
    assert_eq!(context.current_snapshot.values["goal"], value);

    let patched = context
        .apply_patches(
            BTreeMap::from([(
                "joined".to_string(),
                WorkflowDataValue::new(
                    "text/plain",
                    1,
                    serde_json::Value::String("done".to_string()),
                )
                .unwrap(),
            )]),
            Utc::now(),
            "join",
            None,
            None,
        )
        .unwrap();
    let patched_again = context
        .apply_patches(
            BTreeMap::from([(
                "joined".to_string(),
                WorkflowDataValue::new(
                    "text/plain",
                    1,
                    serde_json::Value::String("done".to_string()),
                )
                .unwrap(),
            )]),
            Utc::now(),
            "join",
            None,
            None,
        )
        .unwrap();

    assert_eq!(patched.current_snapshot.revision, 2);
    assert_eq!(
        patched.current_snapshot.snapshot_hash,
        patched_again.current_snapshot.snapshot_hash
    );
}

#[test]
fn workflow_value_schema_validation_rejects_invalid_inputs() {
    assert!(WorkflowDataValue::new("", 1, serde_json::Value::Null).is_err());
    assert!(WorkflowDataValue::new("text/plain", 0, serde_json::Value::Null).is_err());
}

#[test]
fn workflow_output_bag_hash_changes_with_append_order() {
    let payload =
        WorkflowDataValue::new("text/plain", 1, serde_json::Value::String("a".to_string()))
            .unwrap();
    let bag = WorkflowOutputBag::default();
    let first = bag.append(WorkflowOutputBagEntry {
        entry_id: Uuid::new_v4(),
        workflow_run_id: Uuid::new_v4(),
        producer_step_id: Uuid::new_v4(),
        producer_step_run_id: Uuid::new_v4(),
        branch_step_id: None,
        join_step_id: None,
        output_name: "result".to_string(),
        tags: vec![],
        payload: payload.clone(),
        event_sequence: 1,
        appended_at: Utc::now(),
    });
    let second = first.append(WorkflowOutputBagEntry {
        entry_id: Uuid::new_v4(),
        workflow_run_id: Uuid::new_v4(),
        producer_step_id: Uuid::new_v4(),
        producer_step_run_id: Uuid::new_v4(),
        branch_step_id: None,
        join_step_id: None,
        output_name: "result".to_string(),
        tags: vec![],
        payload,
        event_sequence: 2,
        appended_at: Utc::now(),
    });

    assert_ne!(bag.bag_hash, first.bag_hash);
    assert_ne!(first.bag_hash, second.bag_hash);
}

#[test]
fn reducers_are_deterministic_for_list_and_keyed_map() {
    let step_a = Uuid::new_v4();
    let step_b = Uuid::new_v4();
    let entries = vec![
        WorkflowOutputBagEntry {
            entry_id: Uuid::new_v4(),
            workflow_run_id: Uuid::new_v4(),
            producer_step_id: step_b,
            producer_step_run_id: Uuid::new_v4(),
            branch_step_id: Some(step_b),
            join_step_id: None,
            output_name: "branch".to_string(),
            tags: vec![],
            payload: WorkflowDataValue::new(
                "text/plain",
                1,
                serde_json::Value::String("b".to_string()),
            )
            .unwrap(),
            event_sequence: 2,
            appended_at: Utc::now(),
        },
        WorkflowOutputBagEntry {
            entry_id: Uuid::new_v4(),
            workflow_run_id: Uuid::new_v4(),
            producer_step_id: step_a,
            producer_step_run_id: Uuid::new_v4(),
            branch_step_id: Some(step_a),
            join_step_id: None,
            output_name: "branch".to_string(),
            tags: vec![],
            payload: WorkflowDataValue::new(
                "text/plain",
                1,
                serde_json::Value::String("a".to_string()),
            )
            .unwrap(),
            event_sequence: 1,
            appended_at: Utc::now(),
        },
    ];
    let selector = WorkflowBagSelector {
        output_name: "branch".to_string(),
        producer_step_ids: vec![],
        tags: vec![],
        expected_schema: Some("text/plain".to_string()),
        expected_schema_version: Some(1),
    };
    let step_names = BTreeMap::from([
        (step_a, "step-a".to_string()),
        (step_b, "step-b".to_string()),
    ]);

    let list = WorkflowBagReducer::OrderedList
        .reduce(&selector, &entries, &step_names)
        .unwrap();
    let map = WorkflowBagReducer::KeyedMap {
        key_field: WorkflowBagKeyField::ProducerStepName,
    }
    .reduce(&selector, &entries, &step_names)
    .unwrap();

    assert_eq!(list.value, serde_json::json!(["b", "a"]));
    assert_eq!(map.value, serde_json::json!({"step-b":"b","step-a":"a"}));
}
