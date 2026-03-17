use super::*;

#[test]
#[cfg(unix)]
fn agent_loop_repairs_done_until_checkpoint_complete_succeeds() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "DONE:ready to finish".to_string(),
            "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"done\"}"
                .to_string(),
            "DONE:all good".to_string(),
        ],
        prompts.clone(),
    );

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-checkpoint",
            vec![user_input_item(
                "inv-checkpoint",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-checkpoint:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should complete after checkpoint repair");

    assert_eq!(result.turns.len(), 2);
    assert!(matches!(
        result.turns[0].directive,
        ModelDirective::Act { .. }
    ));
    assert!(result.history_items.iter().any(|item| item
        .render_for_prompt()
        .contains("returned DONE before the active execution checkpoint was completed")));
    assert_eq!(result.final_summary.as_deref(), Some("all good"));
}

#[test]
#[cfg(unix)]
fn agent_loop_auto_completes_checkpoint_after_repeated_done_outputs() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "DONE:first summary".to_string(),
            "DONE:second summary".to_string(),
            "DONE:third summary".to_string(),
            "DONE:final summary".to_string(),
        ],
        prompts.clone(),
    );

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-checkpoint-auto",
            vec![user_input_item(
                "inv-checkpoint-auto",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-checkpoint-auto:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should auto-complete checkpoint after repeated DONE outputs");

    assert_eq!(result.turns.len(), 1);
    assert!(matches!(
        result.turns[0].directive,
        ModelDirective::Done { .. }
    ));
    assert_eq!(result.final_summary.as_deref(), Some("final summary"));
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert_eq!(
        result.turns[0].tool_calls[0].tool_name,
        "checkpoint_complete"
    );
    assert!(result.turns[0].tool_calls[0].failure.is_none());
}

#[test]
#[cfg(unix)]
fn agent_loop_accepts_done_after_checkpoint_was_already_completed() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let model = MockModelClient::from_outputs(vec![
        "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"finished work\"}"
            .to_string(),
        "DONE:all good".to_string(),
    ]);

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-checkpoint-done",
            vec![user_input_item(
                "inv-checkpoint-done",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-checkpoint-done:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should accept DONE after prior checkpoint completion");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("all good"));
    assert_eq!(result.turns.len(), 2);
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert_eq!(result.turns[1].tool_calls.len(), 0);
}

#[test]
#[cfg(unix)]
fn agent_loop_auto_finishes_after_redundant_checkpoint_completion() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let model = MockModelClient::from_outputs(vec![
        "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"finished work\"}"
            .to_string(),
        "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"finished work\"}"
            .to_string(),
    ]);

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-redundant-checkpoint",
            vec![user_input_item(
                "inv-redundant-checkpoint",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-redundant-checkpoint:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should auto-finish after redundant checkpoint completion");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("finished work"));
    assert_eq!(result.turns.len(), 2);
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert!(matches!(
        result.turns[1].directive,
        ModelDirective::Done { .. }
    ));
    assert_eq!(result.turns[1].tool_calls.len(), 0);
}

#[test]
#[cfg(unix)]
fn agent_loop_auto_finishes_after_post_checkpoint_non_done_loop() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let model = MockModelClient::from_outputs(vec![
        "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"finished work\"}"
            .to_string(),
        "THINK:double-checking final state".to_string(),
        "ACT:tool:list_files:{}".to_string(),
    ]);

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-post-checkpoint-done",
            vec![user_input_item(
                "inv-post-checkpoint-done",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-post-checkpoint-done:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should auto-finish after post-checkpoint non-DONE outputs");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("finished work"));
    assert_eq!(result.turns.len(), 2);
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert!(matches!(
        result.turns[1].directive,
        ModelDirective::Done { .. }
    ));
    assert_eq!(result.turns[1].tool_calls.len(), 0);
}

#[test]
fn agent_loop_treats_checkpoint_already_completed_auto_completion_as_benign() {
    let model = MockModelClient::from_outputs(vec![
        "DONE:first summary".to_string(),
        "DONE:second summary".to_string(),
        "DONE:third summary".to_string(),
        "DONE:final summary".to_string(),
    ]);
    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-checkpoint-benign",
            vec![user_input_item(
                "inv-checkpoint-benign",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, _action| {
                vec![NativeToolCallTrace {
                    call_id: format!("inv-checkpoint-benign:turn:{turn_index}:tool:0"),
                    tool_name: "checkpoint_complete".to_string(),
                    request: "{}".to_string(),
                    duration_ms: None,
                    response: None,
                    failure: Some(NativeToolCallFailure {
                        code: "native_tool_execution_failed".to_string(),
                        message: "checkpoint completion failed with checkpoint_already_completed"
                            .to_string(),
                        recoverable: false,
                        policy_source: None,
                        denial_reason: None,
                        recovery_hint: None,
                    }),
                    policy_tags: Vec::new(),
                }]
            },
        )
        .expect("loop should treat already-completed checkpoint auto-completion as benign");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("final summary"));
}
