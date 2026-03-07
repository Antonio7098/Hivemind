use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_native_turn_events(
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
}
