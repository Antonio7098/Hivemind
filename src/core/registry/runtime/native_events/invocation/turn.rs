use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
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
                agent_mode: turn.agent_mode.clone(),
                prompt_hash: turn.prompt_hash.clone(),
                context_manifest_hash: turn.context_manifest_hash.clone(),
                delivered_context_hash: turn.delivered_context_hash.clone(),
                mode_contract_hash: turn.mode_contract_hash.clone(),
                inputs_hash: turn.inputs_hash.clone(),
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
                prompt_headroom: turn.prompt_headroom,
                visible_item_count: turn.visible_item_count,
                available_budget: turn.available_budget,
                rendered_prompt_bytes: turn.rendered_prompt_bytes,
                runtime_context_bytes: turn.runtime_context_bytes,
                static_prompt_chars: turn.static_prompt_chars,
                selected_history_count: turn.selected_history_count,
                selected_history_chars: turn.selected_history_chars,
                code_navigation_count: turn.code_navigation_count,
                code_navigation_chars: turn.code_navigation_chars,
                compacted_summary_count: turn.compacted_summary_count,
                compacted_summary_chars: turn.compacted_summary_chars,
                tool_contract_count: turn.tool_contract_count,
                tool_contract_chars: turn.tool_contract_chars,
                assembly_duration_ms: turn.assembly_duration_ms,
                skipped_item_count: turn.skipped_item_count,
                truncated_item_count: turn.truncated_item_count,
                tool_result_items_visible: turn.tool_result_items_visible,
                latest_tool_result_turn_index: turn.latest_tool_result_turn_index,
                latest_tool_names_visible: turn.latest_tool_names_visible.clone(),
                active_code_window_trace: turn.active_code_window_trace.clone(),
                delivery_target: turn.delivery_target.clone(),
            },
            correlation,
            origin,
        )?;

        for threshold in &turn.budget_thresholds_crossed {
            self.append_native_event(
                EventPayload::NativeBudgetThresholdReached {
                    native_correlation: native_correlation.clone(),
                    invocation_id: invocation.invocation_id.clone(),
                    turn_index: turn.turn_index,
                    threshold_percent: *threshold,
                    used_budget: turn.budget_used_after,
                    total_budget: invocation
                        .turns
                        .iter()
                        .find(|candidate| candidate.turn_index == turn.turn_index)
                        .map_or(
                            turn.budget_used_after + turn.budget_remaining,
                            |candidate| candidate.budget_used_after + candidate.budget_remaining,
                        ),
                    remaining_budget: turn.budget_remaining,
                },
                correlation,
                origin,
            )?;
        }

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
                tool_result_count: turn.tool_result_count,
                model_latency_ms: turn.model_latency_ms,
                tool_latency_ms: turn.tool_latency_ms,
                turn_duration_ms: turn.turn_duration_ms,
                elapsed_since_invocation_ms: turn.elapsed_since_invocation_ms,
                request_tokens: turn.request_tokens,
                response_tokens: turn.response_tokens,
                budget_used_before: turn.budget_used_before,
                budget_used_after: turn.budget_used_after,
                budget_remaining: turn.budget_remaining,
                summary: turn.turn_summary.clone(),
            },
            correlation,
            origin,
        )?;

        self.append_native_event(
            EventPayload::NativeTurnSummaryRecorded {
                native_correlation: native_correlation.clone(),
                invocation_id: invocation.invocation_id.clone(),
                turn_index: turn.turn_index,
                agent_mode: turn.agent_mode.clone(),
                from_state: turn.from_state.clone(),
                to_state: turn.to_state.clone(),
                prompt_hash: turn.prompt_hash.clone(),
                context_manifest_hash: turn.context_manifest_hash.clone(),
                delivered_context_hash: turn.delivered_context_hash.clone(),
                mode_contract_hash: turn.mode_contract_hash.clone(),
                inputs_hash: turn.inputs_hash.clone(),
                prompt_headroom: turn.prompt_headroom,
                available_budget: turn.available_budget,
                rendered_prompt_bytes: turn.rendered_prompt_bytes,
                runtime_context_bytes: turn.runtime_context_bytes,
                static_prompt_chars: turn.static_prompt_chars,
                visible_item_count: turn.visible_item_count,
                selected_history_count: turn.selected_history_count,
                selected_history_chars: turn.selected_history_chars,
                code_navigation_count: turn.code_navigation_count,
                code_navigation_chars: turn.code_navigation_chars,
                compacted_summary_count: turn.compacted_summary_count,
                compacted_summary_chars: turn.compacted_summary_chars,
                tool_contract_count: turn.tool_contract_count,
                tool_contract_chars: turn.tool_contract_chars,
                assembly_duration_ms: turn.assembly_duration_ms,
                skipped_item_count: turn.skipped_item_count,
                truncated_item_count: turn.truncated_item_count,
                tool_result_items_visible: turn.tool_result_items_visible,
                latest_tool_result_turn_index: turn.latest_tool_result_turn_index,
                latest_tool_names_visible: turn.latest_tool_names_visible.clone(),
                active_code_window_trace: turn.active_code_window_trace.clone(),
                tool_call_count: turn.tool_calls.len(),
                tool_failure_count: turn
                    .tool_calls
                    .iter()
                    .filter(|call| call.failure.is_some())
                    .count(),
                model_latency_ms: turn.model_latency_ms,
                tool_latency_ms: turn.tool_latency_ms,
                turn_duration_ms: turn.turn_duration_ms,
                elapsed_since_invocation_ms: turn.elapsed_since_invocation_ms,
                request_tokens: turn.request_tokens,
                response_tokens: turn.response_tokens,
                budget_used_before: turn.budget_used_before,
                budget_used_after: turn.budget_used_after,
                budget_remaining: turn.budget_remaining,
                budget_thresholds_crossed: turn.budget_thresholds_crossed.clone(),
                summary: turn.turn_summary.clone(),
            },
            correlation,
            origin,
        )
    }
}
