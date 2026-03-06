use super::*;
use crate::adapters::runtime::{NativeToolCallTrace, NativeTurnTrace};

impl Registry {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn append_native_invocation_events(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        adapter_name: &str,
        invocation: &NativeInvocationTrace,
        origin: &'static str,
    ) -> Result<()> {
        let native_correlation = Self::native_event_correlation(flow, task_id, attempt_id);
        let capture_mode = Self::native_capture_mode_for_event(invocation.capture_mode);
        self.append_native_event(
            EventPayload::AgentInvocationStarted {
                native_correlation: native_correlation.clone(),
                invocation_id: invocation.invocation_id.clone(),
                adapter_name: adapter_name.to_string(),
                provider: invocation.provider.clone(),
                model: invocation.model.clone(),
                runtime_version: invocation.runtime_version.clone(),
                capture_mode,
            },
            correlation,
            origin,
        )?;

        let mut saw_tool_failure = false;
        for turn in &invocation.turns {
            self.append_native_turn_events(
                flow,
                task_id,
                attempt_id,
                correlation,
                invocation,
                turn,
                &native_correlation,
                &mut saw_tool_failure,
                origin,
            )?;
        }

        self.append_native_transport_events(
            attempt_id,
            correlation,
            adapter_name,
            invocation,
            origin,
        )?;
        self.append_native_runtime_state_events(
            attempt_id,
            correlation,
            adapter_name,
            invocation,
            origin,
        )?;
        self.append_native_readiness_events(
            attempt_id,
            correlation,
            adapter_name,
            invocation,
            origin,
        )?;
        self.append_native_invocation_completed_event(
            correlation,
            invocation,
            native_correlation,
            saw_tool_failure,
            origin,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn append_native_turn_events(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        invocation: &NativeInvocationTrace,
        turn: &NativeTurnTrace,
        native_correlation: &NativeEventCorrelation,
        saw_tool_failure: &mut bool,
        origin: &'static str,
    ) -> Result<()> {
        self.append_native_event(
            EventPayload::AgentTurnStarted {
                native_correlation: native_correlation.clone(),
                invocation_id: invocation.invocation_id.clone(),
                turn_index: turn.turn_index,
                from_state: turn.from_state.clone(),
            },
            correlation,
            origin,
        )?;

        let request = self.persist_native_blob(
            "application/json",
            &turn.model_request,
            invocation.capture_mode,
            origin,
        )?;
        self.append_native_event(
            EventPayload::ModelRequestPrepared {
                native_correlation: native_correlation.clone(),
                invocation_id: invocation.invocation_id.clone(),
                turn_index: turn.turn_index,
                request,
            },
            correlation,
            origin,
        )?;

        let response = self.persist_native_blob(
            "text/plain; charset=utf-8",
            &turn.model_response,
            invocation.capture_mode,
            origin,
        )?;
        self.append_native_event(
            EventPayload::ModelResponseReceived {
                native_correlation: native_correlation.clone(),
                invocation_id: invocation.invocation_id.clone(),
                turn_index: turn.turn_index,
                response,
            },
            correlation,
            origin,
        )?;

        for tool_call in &turn.tool_calls {
            self.append_native_tool_call_events(
                flow,
                task_id,
                attempt_id,
                correlation,
                invocation,
                turn,
                tool_call,
                native_correlation,
                saw_tool_failure,
                origin,
            )?;
        }

        self.append_native_event(
            EventPayload::AgentTurnCompleted {
                native_correlation: native_correlation.clone(),
                invocation_id: invocation.invocation_id.clone(),
                turn_index: turn.turn_index,
                to_state: turn.to_state.clone(),
                summary: turn.turn_summary.clone(),
            },
            correlation,
            origin,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn append_native_tool_call_events(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        invocation: &NativeInvocationTrace,
        turn: &NativeTurnTrace,
        tool_call: &NativeToolCallTrace,
        native_correlation: &NativeEventCorrelation,
        saw_tool_failure: &mut bool,
        origin: &'static str,
    ) -> Result<()> {
        let request = self.persist_native_blob(
            "text/plain; charset=utf-8",
            &tool_call.request,
            invocation.capture_mode,
            origin,
        )?;
        self.append_native_event(
            EventPayload::ToolCallRequested {
                native_correlation: native_correlation.clone(),
                task_id: Some(task_id),
                invocation_id: invocation.invocation_id.clone(),
                turn_index: turn.turn_index,
                call_id: tool_call.call_id.clone(),
                tool_name: tool_call.tool_name.clone(),
                request,
                policy_tags: tool_call.policy_tags.clone(),
            },
            correlation,
            origin,
        )?;
        self.append_native_event(
            EventPayload::ToolCallStarted {
                native_correlation: native_correlation.clone(),
                task_id: Some(task_id),
                invocation_id: invocation.invocation_id.clone(),
                turn_index: turn.turn_index,
                call_id: tool_call.call_id.clone(),
                tool_name: tool_call.tool_name.clone(),
                policy_tags: tool_call.policy_tags.clone(),
            },
            correlation,
            origin,
        )?;

        if let Some(failure) = &tool_call.failure {
            *saw_tool_failure = true;
            self.append_native_event(
                EventPayload::ToolCallFailed {
                    native_correlation: native_correlation.clone(),
                    task_id: Some(task_id),
                    invocation_id: invocation.invocation_id.clone(),
                    turn_index: turn.turn_index,
                    call_id: tool_call.call_id.clone(),
                    tool_name: tool_call.tool_name.clone(),
                    code: failure.code.clone(),
                    message: failure.message.clone(),
                    recoverable: failure.recoverable,
                    policy_tags: tool_call.policy_tags.clone(),
                },
                correlation,
                origin,
            )?;
        } else if let Some(response_payload) = tool_call.response.as_deref() {
            let response = self.persist_native_blob(
                "text/plain; charset=utf-8",
                response_payload,
                invocation.capture_mode,
                origin,
            )?;
            self.append_native_event(
                EventPayload::ToolCallCompleted {
                    native_correlation: native_correlation.clone(),
                    task_id: Some(task_id),
                    invocation_id: invocation.invocation_id.clone(),
                    turn_index: turn.turn_index,
                    call_id: tool_call.call_id.clone(),
                    tool_name: tool_call.tool_name.clone(),
                    response,
                    policy_tags: tool_call.policy_tags.clone(),
                },
                correlation,
                origin,
            )?;
            self.append_graph_query_event_for_native_tool_call(
                flow,
                task_id,
                attempt_id,
                &tool_call.tool_name,
                response_payload,
                correlation.clone(),
                origin,
            )?;
        }
        Ok(())
    }

    fn append_native_transport_events(
        &self,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        adapter_name: &str,
        invocation: &NativeInvocationTrace,
        origin: &'static str,
    ) -> Result<()> {
        for attempt in &invocation.transport.attempts {
            let category = if attempt.rate_limited {
                "rate_limit"
            } else if attempt.code.starts_with("native_stream_") {
                "transport_stream"
            } else {
                "transport"
            };
            self.append_native_event(
                EventPayload::RuntimeErrorClassified {
                    attempt_id,
                    adapter_name: adapter_name.to_string(),
                    code: attempt.code.clone(),
                    category: category.to_string(),
                    message: attempt.message.clone(),
                    recoverable: attempt.retryable,
                    retryable: attempt.retryable,
                    rate_limited: attempt.rate_limited,
                },
                correlation,
                origin,
            )?;

            if let Some(backoff_ms) = attempt.backoff_ms {
                self.append_native_event(
                    EventPayload::RuntimeRecoveryScheduled {
                        attempt_id,
                        from_adapter: format!("{adapter_name}:{}", attempt.transport),
                        to_adapter: format!("{adapter_name}:{}", attempt.transport),
                        strategy: "native_transport_retry".to_string(),
                        reason: format!(
                            "turn={} code={} retryable={}",
                            attempt.turn_index, attempt.code, attempt.retryable
                        ),
                        backoff_ms,
                    },
                    correlation,
                    origin,
                )?;
            }
        }

        for fallback in &invocation.transport.fallback_activations {
            self.append_native_event(
                EventPayload::RuntimeRecoveryScheduled {
                    attempt_id,
                    from_adapter: format!("{adapter_name}:{}", fallback.from_transport),
                    to_adapter: format!("{adapter_name}:{}", fallback.to_transport),
                    strategy: "native_transport_fallback".to_string(),
                    reason: format!("turn={} reason={}", fallback.turn_index, fallback.reason),
                    backoff_ms: 0,
                },
                correlation,
                origin,
            )?;
        }

        Ok(())
    }

    fn append_native_runtime_state_events(
        &self,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        adapter_name: &str,
        invocation: &NativeInvocationTrace,
        origin: &'static str,
    ) -> Result<()> {
        if let Some(runtime_state) = invocation.runtime_state.as_ref() {
            self.append_native_event(
                EventPayload::RuntimeRecoveryScheduled {
                    attempt_id,
                    from_adapter: format!("{adapter_name}:runtime_state:init"),
                    to_adapter: format!("{adapter_name}:runtime_state:ready"),
                    strategy: "native_runtime_state_bootstrap".to_string(),
                    reason: format!(
                        "db_path={} busy_timeout_ms={} log_batch_size={} retention_days={}",
                        runtime_state.db_path,
                        runtime_state.busy_timeout_ms,
                        runtime_state.log_batch_size,
                        runtime_state.log_retention_days
                    ),
                    backoff_ms: 0,
                },
                correlation,
                origin,
            )?;
        }
        Ok(())
    }

    fn append_native_readiness_events(
        &self,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        adapter_name: &str,
        invocation: &NativeInvocationTrace,
        origin: &'static str,
    ) -> Result<()> {
        for transition in &invocation.readiness_transitions {
            self.append_native_event(
                EventPayload::RuntimeRecoveryScheduled {
                    attempt_id,
                    from_adapter: format!(
                        "{adapter_name}:{}:{}",
                        transition.component, transition.from_state
                    ),
                    to_adapter: format!(
                        "{adapter_name}:{}:{}",
                        transition.component, transition.to_state
                    ),
                    strategy: "native_component_readiness".to_string(),
                    reason: format!(
                        "token={} ts_ms={} reason={}",
                        transition.token, transition.timestamp_ms, transition.reason
                    ),
                    backoff_ms: 0,
                },
                correlation,
                origin,
            )?;

            if transition.to_state == "failed" {
                self.append_native_event(
                    EventPayload::RuntimeErrorClassified {
                        attempt_id,
                        adapter_name: adapter_name.to_string(),
                        code: "native_component_not_ready".to_string(),
                        category: "runtime_setup".to_string(),
                        message: format!(
                            "Native component '{}' failed readiness transition: {}",
                            transition.component, transition.reason
                        ),
                        recoverable: false,
                        retryable: false,
                        rate_limited: false,
                    },
                    correlation,
                    origin,
                )?;
            }
        }
        Ok(())
    }

    fn append_native_invocation_completed_event(
        &self,
        correlation: &CorrelationIds,
        invocation: &NativeInvocationTrace,
        native_correlation: NativeEventCorrelation,
        saw_tool_failure: bool,
        origin: &'static str,
    ) -> Result<()> {
        let success = invocation.failure.is_none() && !saw_tool_failure;
        let (error_code, error_message, recoverable) = invocation.failure.as_ref().map_or_else(
            || {
                if saw_tool_failure {
                    (
                        Some("native_tool_call_failed".to_string()),
                        Some("One or more native tool calls failed".to_string()),
                        Some(false),
                    )
                } else {
                    (None, None, None)
                }
            },
            |failure| {
                (
                    Some(failure.code.clone()),
                    Some(failure.message.clone()),
                    Some(failure.recoverable),
                )
            },
        );

        self.append_native_event(
            EventPayload::AgentInvocationCompleted {
                native_correlation,
                invocation_id: invocation.invocation_id.clone(),
                total_turns: u32::try_from(invocation.turns.len()).unwrap_or(u32::MAX),
                final_state: invocation.final_state.clone(),
                success,
                final_summary: invocation.final_summary.clone(),
                error_code,
                error_message,
                recoverable,
            },
            correlation,
            origin,
        )
    }
}
