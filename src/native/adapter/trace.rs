use super::*;

impl NativeRuntimeAdapter {
    pub(crate) fn render_stdout(result: &crate::native::AgentLoopResult) -> String {
        let mut lines = Vec::new();
        for turn in &result.turns {
            let directive = match &turn.directive {
                crate::native::ModelDirective::Think { message } => format!("THINK {message}"),
                crate::native::ModelDirective::Act { action } => format!("ACT {action}"),
                crate::native::ModelDirective::Done { summary } => format!("DONE {summary}"),
            };
            lines.push(format!(
                "native turn {}: {} -> {} | {directive}",
                turn.turn_index,
                turn.from_state.as_str(),
                turn.to_state.as_str(),
            ));
        }
        if let Some(summary) = result.final_summary.as_ref() {
            lines.push(format!("native summary: {summary}"));
        }
        lines.join("\n")
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn trace_from_success(
        invocation_id: &str,
        config: &NativeAdapterConfig,
        result: &crate::native::AgentLoopResult,
        transport: NativeTransportTelemetry,
        runtime_state: Option<NativeRuntimeStateTelemetry>,
        readiness_transitions: Vec<NativeReadinessTransition>,
        tool_engine: &NativeToolEngine,
        tool_context: &ToolExecutionContext<'_>,
    ) -> NativeInvocationTrace {
        let turns: Vec<NativeTurnTrace> = result
            .turns
            .iter()
            .map(|turn| {
                let model_request = serde_json::to_string(&turn.request).unwrap_or_else(|_| {
                    format!(
                        "turn={} state={} prompt={}",
                        turn.request.turn_index,
                        turn.request.state.as_str(),
                        turn.request.prompt
                    )
                });

                let tool_calls = match &turn.directive {
                    ModelDirective::Act { action } => Self::tool_trace_for_act(
                        invocation_id,
                        turn.turn_index,
                        action,
                        tool_engine,
                        tool_context,
                    ),
                    _ => Vec::new(),
                };

                let turn_summary = if let ModelDirective::Done { summary } = &turn.directive {
                    Some(summary.clone())
                } else {
                    None
                };

                NativeTurnTrace {
                    turn_index: turn.turn_index,
                    from_state: turn.from_state.as_str().to_string(),
                    to_state: turn.to_state.as_str().to_string(),
                    model_request,
                    model_response: turn.raw_output.clone(),
                    tool_calls,
                    turn_summary,
                }
            })
            .collect();

        let failure = turns
            .iter()
            .flat_map(|turn| turn.tool_calls.iter())
            .find_map(|call| {
                call.failure
                    .as_ref()
                    .map(|failure| NativeInvocationFailure {
                        code: failure.code.clone(),
                        message: failure.message.clone(),
                        recoverable: failure.recoverable,
                        recovery_hint: None,
                    })
            });

        NativeInvocationTrace {
            invocation_id: invocation_id.to_string(),
            provider: config.provider_name.clone(),
            model: config.model_name.clone(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            capture_mode: Self::capture_mode(&config.native),
            turns,
            final_state: result.final_state.as_str().to_string(),
            final_summary: result.final_summary.clone(),
            failure,
            transport,
            runtime_state,
            readiness_transitions,
        }
    }

    pub(crate) fn trace_from_failure(
        invocation_id: &str,
        config: &NativeAdapterConfig,
        final_state: crate::native::AgentLoopState,
        err: &crate::native::NativeRuntimeError,
        transport: NativeTransportTelemetry,
        runtime_state: Option<NativeRuntimeStateTelemetry>,
        readiness_transitions: Vec<NativeReadinessTransition>,
    ) -> NativeInvocationTrace {
        NativeInvocationTrace {
            invocation_id: invocation_id.to_string(),
            provider: config.provider_name.clone(),
            model: config.model_name.clone(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            capture_mode: Self::capture_mode(&config.native),
            turns: Vec::new(),
            final_state: final_state.as_str().to_string(),
            final_summary: None,
            failure: Some(NativeInvocationFailure {
                code: err.code().to_string(),
                message: err.message(),
                recoverable: err.recoverable(),
                recovery_hint: err.recovery_hint(),
            }),
            transport,
            runtime_state,
            readiness_transitions,
        }
    }

    fn tool_trace_for_act(
        invocation_id: &str,
        turn_index: u32,
        action: &str,
        tool_engine: &NativeToolEngine,
        tool_context: &ToolExecutionContext<'_>,
    ) -> Vec<NativeToolCallTrace> {
        let call_id = format!("{invocation_id}:turn:{turn_index}:tool:0");
        if let Some(message) = action.strip_prefix("fail_tool:") {
            return vec![NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                response: None,
                failure: Some(NativeToolCallFailure {
                    code: "native_tool_call_failed".to_string(),
                    message: message.trim().to_string(),
                    recoverable: false,
                }),
                policy_tags: Vec::new(),
            }];
        }

        match NativeToolAction::parse(action) {
            Ok(Some(tool_action)) => {
                vec![tool_engine.execute_action_trace(call_id, &tool_action, tool_context)]
            }
            Ok(None) => vec![NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                response: Some(format!("completed:{action}")),
                failure: None,
                policy_tags: Vec::new(),
            }],
            Err(error) => vec![NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                response: None,
                failure: Some(NativeToolCallFailure {
                    code: error.code,
                    message: error.message,
                    recoverable: error.recoverable,
                }),
                policy_tags: error.policy_tags,
            }],
        }
    }
}
