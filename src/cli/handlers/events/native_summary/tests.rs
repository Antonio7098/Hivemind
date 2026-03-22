use super::*;
use crate::core::events::{
    CorrelationIds, Event, EventPayload, NativeEventCorrelation, NativeEventPayloadCaptureMode,
};
use uuid::Uuid;

#[test]
// ARCH_DEBT: legacy oversized test awaiting refactor
#[allow(clippy::too_many_lines)]
fn native_summary_reports_passed_verification() {
    let flow = NativeEventCorrelation {
        project_id: Uuid::new_v4(),
        graph_id: Uuid::new_v4(),
        flow_id: Uuid::new_v4(),
        task_id: Uuid::new_v4(),
        attempt_id: Uuid::new_v4(),
    };
    let correlation = CorrelationIds::for_graph(flow.project_id, flow.graph_id);
    let events = vec![
        Event::new(
            EventPayload::AgentInvocationStarted {
                native_correlation: flow.clone(),
                invocation_id: "inv-1".to_string(),
                adapter_name: "native".to_string(),
                provider: "test".to_string(),
                model: "mock".to_string(),
                runtime_version: "1".to_string(),
                capture_mode: NativeEventPayloadCaptureMode::MetadataOnly,
                agent_mode: Some("planner".to_string()),
                allowed_tools: vec!["read_file".to_string()],
                allowed_capabilities: vec!["read".to_string()],
                configured_max_turns: Some(8),
                configured_timeout_budget_ms: Some(300_000),
                configured_token_budget: Some(32_000),
                configured_prompt_headroom: Some(4_096),
            },
            correlation.clone(),
        ),
        Event::new(
            EventPayload::ToolCallFailed {
                native_correlation: flow.clone(),
                task_id: None,
                invocation_id: "inv-1".to_string(),
                turn_index: 0,
                call_id: "call-1".to_string(),
                tool_name: "read_file".to_string(),
                code: "native_tool_input_invalid".to_string(),
                message: "bad input".to_string(),
                recoverable: true,
                duration_ms: Some(1),
                policy_source: None,
                denial_reason: None,
                recovery_hint: Some("retry with valid schema".to_string()),
                policy_tags: Vec::new(),
            },
            correlation.clone(),
        ),
        Event::new(
            EventPayload::NativeTurnSummaryRecorded {
                native_correlation: flow.clone(),
                invocation_id: "inv-1".to_string(),
                turn_index: 0,
                agent_mode: Some("planner".to_string()),
                from_state: "think".to_string(),
                to_state: "done".to_string(),
                prompt_hash: Some("prompt".to_string()),
                context_manifest_hash: Some("manifest".to_string()),
                delivered_context_hash: Some("context".to_string()),
                mode_contract_hash: Some("mode".to_string()),
                inputs_hash: Some("inputs".to_string()),
                prompt_headroom: Some(128),
                available_budget: 1024,
                rendered_prompt_bytes: 512,
                runtime_context_bytes: 64,
                static_prompt_chars: 128,
                visible_item_count: 3,
                selected_history_count: 1,
                selected_history_chars: 48,
                code_navigation_count: 1,
                code_navigation_chars: 24,
                compacted_summary_count: 0,
                compacted_summary_chars: 0,
                tool_contract_count: 1,
                tool_contract_chars: 32,
                assembly_duration_ms: 1,
                skipped_item_count: 0,
                truncated_item_count: 0,
                tool_result_items_visible: 0,
                latest_tool_result_turn_index: None,
                latest_tool_names_visible: Vec::new(),
                active_code_window_trace: Vec::new(),
                tool_call_count: 0,
                tool_failure_count: 0,
                model_latency_ms: 1,
                tool_latency_ms: 0,
                turn_duration_ms: 1,
                elapsed_since_invocation_ms: 1,
                request_tokens: 10,
                response_tokens: 5,
                budget_used_before: 0,
                budget_used_after: 10,
                budget_remaining: 1014,
                budget_thresholds_crossed: Vec::new(),
                summary: Some("done".to_string()),
            },
            correlation.clone(),
        ),
        Event::new(
            EventPayload::AgentInvocationCompleted {
                native_correlation: flow,
                invocation_id: "inv-1".to_string(),
                total_turns: 1,
                final_state: "done".to_string(),
                success: true,
                final_summary: Some("done".to_string()),
                error_code: None,
                error_message: None,
                recoverable: None,
            },
            correlation,
        ),
    ];

    let summary = build_native_summary(&events);
    assert_eq!(summary.invocation_count, 1);
    assert!(
        summary.verification.passed,
        "{:?}",
        summary.verification.failures
    );
    assert_eq!(summary.invocations[0].success, Some(true));
    assert!(summary.invocations[0].failure_codes.is_empty());
    assert_eq!(summary.invocations[0].configured_max_turns, Some(8));
    assert_eq!(
        summary.invocations[0].turns[0]
            .mode_contract_hash
            .as_deref(),
        Some("mode")
    );
}

#[test]
// ARCH_DEBT: legacy oversized test awaiting refactor
#[allow(clippy::too_many_lines)]
fn native_summary_accepts_prior_tool_results_delivered_via_navigation() {
    let flow = NativeEventCorrelation {
        project_id: Uuid::new_v4(),
        graph_id: Uuid::new_v4(),
        flow_id: Uuid::new_v4(),
        task_id: Uuid::new_v4(),
        attempt_id: Uuid::new_v4(),
    };
    let correlation = CorrelationIds {
        project_id: Some(flow.project_id),
        graph_id: Some(flow.graph_id),
        flow_id: Some(flow.flow_id),
        workflow_id: None,
        workflow_run_id: None,
        root_workflow_run_id: None,
        parent_workflow_run_id: None,
        task_id: Some(flow.task_id),
        step_id: None,
        step_run_id: None,
        attempt_id: Some(flow.attempt_id),
    };
    let events = vec![
        Event::new(
            EventPayload::NativeTurnSummaryRecorded {
                native_correlation: flow.clone(),
                invocation_id: "inv-2".to_string(),
                turn_index: 0,
                from_state: "think".to_string(),
                to_state: "act".to_string(),
                agent_mode: Some("freeflow".to_string()),
                prompt_hash: Some("p0".to_string()),
                context_manifest_hash: Some("m0".to_string()),
                delivered_context_hash: Some("d0".to_string()),
                prompt_headroom: Some(768),
                available_budget: 1000,
                mode_contract_hash: Some("mode".to_string()),
                inputs_hash: Some("inputs".to_string()),
                rendered_prompt_bytes: 100,
                runtime_context_bytes: 50,
                static_prompt_chars: 80,
                visible_item_count: 2,
                selected_history_count: 2,
                selected_history_chars: 40,
                code_navigation_count: 0,
                code_navigation_chars: 0,
                compacted_summary_count: 0,
                compacted_summary_chars: 0,
                tool_contract_count: 1,
                tool_contract_chars: 20,
                assembly_duration_ms: 1,
                skipped_item_count: 0,
                truncated_item_count: 0,
                tool_result_items_visible: 0,
                latest_tool_result_turn_index: None,
                latest_tool_names_visible: Vec::new(),
                active_code_window_trace: Vec::new(),
                tool_call_count: 1,
                tool_failure_count: 0,
                model_latency_ms: 1,
                tool_latency_ms: 1,
                turn_duration_ms: 2,
                elapsed_since_invocation_ms: 2,
                request_tokens: 8,
                response_tokens: 4,
                budget_used_before: 0,
                budget_used_after: 10,
                budget_remaining: 990,
                budget_thresholds_crossed: Vec::new(),
                summary: None,
            },
            correlation.clone(),
        ),
        Event::new(
            EventPayload::NativeTurnSummaryRecorded {
                native_correlation: flow.clone(),
                invocation_id: "inv-2".to_string(),
                turn_index: 1,
                from_state: "act".to_string(),
                to_state: "done".to_string(),
                agent_mode: Some("freeflow".to_string()),
                prompt_hash: Some("p1".to_string()),
                context_manifest_hash: Some("m1".to_string()),
                delivered_context_hash: Some("d1".to_string()),
                prompt_headroom: Some(768),
                available_budget: 980,
                mode_contract_hash: Some("mode".to_string()),
                inputs_hash: Some("inputs".to_string()),
                rendered_prompt_bytes: 120,
                runtime_context_bytes: 60,
                static_prompt_chars: 90,
                visible_item_count: 4,
                selected_history_count: 3,
                selected_history_chars: 60,
                code_navigation_count: 1,
                code_navigation_chars: 24,
                compacted_summary_count: 0,
                compacted_summary_chars: 0,
                tool_contract_count: 1,
                tool_contract_chars: 20,
                assembly_duration_ms: 1,
                skipped_item_count: 0,
                truncated_item_count: 0,
                tool_result_items_visible: 0,
                latest_tool_result_turn_index: None,
                latest_tool_names_visible: Vec::new(),
                active_code_window_trace: Vec::new(),
                tool_call_count: 0,
                tool_failure_count: 0,
                model_latency_ms: 1,
                tool_latency_ms: 0,
                turn_duration_ms: 1,
                elapsed_since_invocation_ms: 3,
                request_tokens: 9,
                response_tokens: 4,
                budget_used_before: 10,
                budget_used_after: 20,
                budget_remaining: 980,
                budget_thresholds_crossed: Vec::new(),
                summary: Some("done".to_string()),
            },
            correlation.clone(),
        ),
        Event::new(
            EventPayload::AgentInvocationCompleted {
                native_correlation: flow,
                invocation_id: "inv-2".to_string(),
                total_turns: 2,
                final_state: "done".to_string(),
                success: true,
                final_summary: Some("done".to_string()),
                error_code: None,
                error_message: None,
                recoverable: None,
            },
            correlation,
        ),
    ];

    let summary = build_native_summary(&events);
    assert!(
        summary.verification.passed,
        "{:?}",
        summary.verification.failures
    );
}
