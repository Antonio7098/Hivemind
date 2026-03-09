use super::*;

impl Registry {
    pub(super) fn append_native_runtime_state_events(
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

    pub(super) fn append_native_readiness_events(
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

    pub(super) fn append_native_invocation_completed_event(
        &self,
        correlation: &CorrelationIds,
        invocation: &NativeInvocationTrace,
        native_correlation: NativeEventCorrelation,
        _saw_tool_failure: bool,
        origin: &'static str,
    ) -> Result<()> {
        let success = invocation.failure.is_none();
        let (error_code, error_message, recoverable) = invocation.failure.as_ref().map_or_else(
            || (None, None, None),
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
