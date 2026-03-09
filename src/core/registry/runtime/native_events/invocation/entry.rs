use super::*;

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
                agent_mode: invocation.agent_mode.clone(),
                allowed_tools: invocation.allowed_tools.clone(),
                allowed_capabilities: invocation.allowed_capabilities.clone(),
                configured_max_turns: invocation.configured_max_turns,
                configured_timeout_budget_ms: invocation.configured_timeout_budget_ms,
                configured_token_budget: invocation.configured_token_budget,
                configured_prompt_headroom: invocation.configured_prompt_headroom,
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

        for compaction in &invocation.history_compactions {
            self.append_native_event(
                EventPayload::NativeHistoryCompactionRecorded {
                    native_correlation: native_correlation.clone(),
                    invocation_id: invocation.invocation_id.clone(),
                    turn_index: compaction.turn_index,
                    reason: compaction.reason.clone(),
                    rendered_prompt_bytes_before: compaction.rendered_prompt_bytes_before,
                    selected_history_count_before: compaction.selected_history_count_before,
                    selected_history_chars_before: compaction.selected_history_chars_before,
                    visible_items_before: compaction.visible_items_before,
                    visible_items_after: compaction.visible_items_after,
                    prompt_tokens_before: compaction.prompt_tokens_before,
                    projected_budget_used: compaction.projected_budget_used,
                    token_budget: compaction.token_budget,
                    elapsed_since_invocation_ms: compaction.elapsed_since_invocation_ms,
                },
                correlation,
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
}
