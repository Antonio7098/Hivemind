use super::*;
use tempfile::tempdir;

fn basic_input() -> ExecutionInput {
    ExecutionInput {
        task_description: "list files".to_string(),
        success_criteria: "show files".to_string(),
        context: None,
        prior_attempts: Vec::new(),
        verifier_feedback: None,
        native_prompt_metadata: None,
    }
}

#[test]
fn execute_requires_prepare() {
    let mut adapter = NativeRuntimeAdapter::new(NativeAdapterConfig::default());
    let error = adapter
        .execute(basic_input())
        .expect_err("should require prepare");
    assert_eq!(error.code, "not_prepared");
}

#[test]
fn invalid_scope_json_is_reported() {
    let mut config = NativeAdapterConfig::default();
    config
        .base
        .env
        .insert("HIVEMIND_TASK_SCOPE_JSON".to_string(), "{".to_string());
    let mut adapter = NativeRuntimeAdapter::new(config);
    let dir = tempdir().expect("tempdir");
    adapter.prepare(Uuid::new_v4(), dir.path()).unwrap();
    let error = adapter
        .execute(basic_input())
        .expect_err("invalid scope json should fail");
    assert_eq!(error.code, "native_scope_decode_failed");
}

#[test]
fn execution_trace_includes_mode_and_prompt_metadata() {
    let state_dir = tempdir().expect("state tempdir");
    let mut config = NativeAdapterConfig::default();
    config.native.agent_mode = crate::native::AgentMode::Planner;
    let expected_max_turns = config.native.max_turns;
    let expected_token_budget = config.native.token_budget;
    let expected_prompt_headroom = config.native.prompt_headroom;
    config.base.env.insert(
        "HIVEMIND_NATIVE_STATE_DIR".to_string(),
        state_dir.path().display().to_string(),
    );
    let mut adapter = NativeRuntimeAdapter::new(config);
    let dir = tempdir().expect("tempdir");
    adapter.prepare(Uuid::new_v4(), dir.path()).unwrap();

    let report = adapter
        .execute(basic_input())
        .expect("native execution succeeds");
    let trace = report.native_invocation.expect("native trace present");

    assert_eq!(trace.agent_mode.as_deref(), Some("planner"));
    assert_eq!(trace.configured_max_turns, Some(expected_max_turns));
    assert_eq!(trace.configured_token_budget, Some(expected_token_budget));
    assert_eq!(
        trace.configured_prompt_headroom,
        Some(expected_prompt_headroom)
    );
    assert!(trace.allowed_tools.iter().all(|tool| tool != "write_file"));
    assert!(!trace.turns.is_empty());
    assert_eq!(trace.turns[0].agent_mode.as_deref(), Some("planner"));
    assert!(trace.turns[0].prompt_hash.is_some());
    assert!(trace.turns[0].prompt_headroom.is_some());
    assert!(trace.turns[0].mode_contract_hash.is_some());
    assert!(trace.turns[0].available_budget > 0);
    assert!(trace.turns[0].rendered_prompt_bytes > 0);
    assert!(trace.turns[0].request_tokens > 0);
    assert!(trace.turns[0].response_tokens > 0);
}

#[test]
fn execute_interactive_emits_native_progress_markers() {
    let state_dir = tempdir().expect("state tempdir");
    let mut config = NativeAdapterConfig::default();
    config.base.env.insert(
        "HIVEMIND_NATIVE_STATE_DIR".to_string(),
        state_dir.path().display().to_string(),
    );
    let mut adapter = NativeRuntimeAdapter::new(config);
    let dir = tempdir().expect("tempdir");
    adapter.prepare(Uuid::new_v4(), dir.path()).unwrap();

    let mut outputs = Vec::new();
    adapter
        .execute_interactive(&basic_input(), |event| {
            if let InteractiveAdapterEvent::Output { content } = event {
                outputs.push(content);
            }
            Ok(())
        })
        .expect("interactive execution succeeds");

    let transcript = outputs.join("");
    assert!(transcript.contains("stage=invocation_starting"));
    assert!(transcript.contains("stage=model_client_ready"));
    assert!(transcript.contains("stage=turn_request_prepared"));
    assert!(transcript.contains("elapsed_ms="));
    assert!(transcript.contains("assembly_latency_ms="));
    assert!(transcript.contains("stage=model_request_started"));
    assert!(transcript.contains("stage=model_response_received"));
    assert!(transcript.contains("stage=invocation_finished success=true"));
}

#[test]
fn execute_interactive_reports_history_compaction_under_budget_pressure() {
    let state_dir = tempdir().expect("state tempdir");
    let mut config = NativeAdapterConfig::default();
    config.native.token_budget = 15_000;
    config.native.prompt_headroom = 128;
    config.scripted_directives = vec![
        "ACT:tool:read_file:{\"path\":\"note.txt\"}".to_string(),
        "ACT:tool:read_file:{\"path\":\"note.txt\"}".to_string(),
        "ACT:tool:read_file:{\"path\":\"note.txt\"}".to_string(),
        "ACT:tool:read_file:{\"path\":\"note.txt\"}".to_string(),
        "DONE:finished under pressure".to_string(),
    ];
    config.base.env.insert(
        "HIVEMIND_NATIVE_STATE_DIR".to_string(),
        state_dir.path().display().to_string(),
    );
    let mut adapter = NativeRuntimeAdapter::new(config);
    let dir = tempdir().expect("tempdir");
    std::fs::write(dir.path().join("note.txt"), "x".repeat(900)).expect("write note");
    adapter.prepare(Uuid::new_v4(), dir.path()).unwrap();

    let mut outputs = Vec::new();
    let mut input = basic_input();
    input.task_description = "Inspect the note repeatedly".to_string();
    let result = adapter
        .execute_interactive(&input, |event| {
            if let InteractiveAdapterEvent::Output { content } = event {
                outputs.push(content);
            }
            Ok(())
        })
        .expect("interactive execution should succeed");

    assert!(
        result.report.exit_code == 0
            || result
                .report
                .stderr
                .contains("Native loop exceeded token budget")
    );
    let transcript = outputs.join("");
    assert!(
        transcript.contains("stage=history_compacted"),
        "{transcript}"
    );
    assert!(
        transcript.contains("reason=soft_budget_pressure"),
        "{transcript}"
    );
}

#[test]
fn recovered_tool_failures_do_not_fail_completed_native_invocation() {
    let state_dir = tempdir().expect("state tempdir");
    let mut config = NativeAdapterConfig {
        provider_name: "mock".to_string(),
        scripted_directives: vec![
            "ACT:tool:graph_query:{\"query\":\"MATCH (n) RETURN n\"}".to_string(),
            "DONE:recovered successfully".to_string(),
        ],
        ..NativeAdapterConfig::default()
    };
    config.base.env.insert(
        "HIVEMIND_NATIVE_STATE_DIR".to_string(),
        state_dir.path().display().to_string(),
    );
    let mut adapter = NativeRuntimeAdapter::new(config);
    let dir = tempdir().expect("tempdir");
    adapter.prepare(Uuid::new_v4(), dir.path()).unwrap();

    let report = adapter.execute(basic_input()).expect("execution succeeds");
    let trace = report.native_invocation.expect("native trace");

    assert_eq!(report.exit_code, 0);
    assert!(trace.failure.is_none());
    assert_eq!(
        trace.final_summary.as_deref(),
        Some("recovered successfully")
    );
    assert_eq!(trace.turns.len(), 2);
    assert!(trace.turns[0].tool_calls[0].failure.is_some());
}
