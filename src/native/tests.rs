use super::*;

#[test]
fn agent_loop_transitions_think_act_done() {
    let model = MockModelClient::from_outputs(vec![
        "ACT:run deterministic step".to_string(),
        "DONE:all good".to_string(),
    ]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let result = loop_harness
        .run("test prompt", Some("context"))
        .expect("loop should complete");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.turns.len(), 2);
    assert_eq!(result.turns[0].from_state, AgentLoopState::Think);
    assert_eq!(result.turns[0].to_state, AgentLoopState::Act);
    assert_eq!(result.turns[1].from_state, AgentLoopState::Act);
    assert_eq!(result.turns[1].to_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("all good"));
}

#[test]
fn agent_loop_fails_loud_on_invalid_transition() {
    let model = MockModelClient::from_outputs(vec!["THINK:still thinking".to_string()]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let err = loop_harness
        .run("test prompt", None)
        .expect_err("expected invalid transition");

    assert!(matches!(
        err,
        NativeRuntimeError::InvalidTransition {
            from: AgentLoopState::Think,
            to: AgentLoopState::Think
        }
    ));
    assert_eq!(err.code(), "native_invalid_transition");
}

#[test]
fn agent_loop_fails_loud_on_malformed_model_output() {
    let model = MockModelClient::from_outputs(vec!["oops not structured".to_string()]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let err = loop_harness
        .run("test prompt", None)
        .expect_err("expected malformed output");

    let NativeRuntimeError::MalformedModelOutput {
        raw_output,
        recovery_hint,
        ..
    } = err
    else {
        panic!("expected malformed output error");
    };
    assert_eq!(raw_output, "oops not structured");
    assert!(
        recovery_hint.contains("THINK/ACT/DONE"),
        "recovery hint should be explicit"
    );
}
