use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_native_tool_call_events(
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
                    duration_ms: tool_call.duration_ms,
                    policy_source: failure.policy_source.clone(),
                    denial_reason: failure.denial_reason.clone(),
                    recovery_hint: failure.recovery_hint.clone(),
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
                    duration_ms: tool_call.duration_ms,
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
}
