use crate::app::AppContext;
use crate::core::events::{
    CorrelationIds, Event, EventPayload, NativeBlobRef, NativeEventCorrelation,
    NativeEventPayloadCaptureMode, RuntimeOutputStream, RuntimeRole,
};
use crate::core::registry::{Registry, RegistryConfig};
use crate::server::{handle_api_request_inner, ApiMethod, ApiResponse};
use chrono::Utc;
use serde_json::Value;
use std::mem;
use uuid::Uuid;

pub(crate) fn test_app() -> AppContext {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().to_path_buf();
    mem::forget(tmp);
    let config = RegistryConfig::with_dir(data_dir);
    AppContext::with_registry_config(config)
}

pub(crate) fn json_value(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("json")
}

pub(crate) fn api_request(
    app: &AppContext,
    method: ApiMethod,
    url: &str,
    body: Option<&[u8]>,
) -> ApiResponse {
    handle_api_request_inner(app, method, url, 10, body).expect("api response")
}

pub(crate) fn native_blob_ref(label: &str) -> NativeBlobRef {
    NativeBlobRef {
        digest: format!("digest-{label}"),
        byte_size: label.len() as u64,
        media_type: "application/json".to_string(),
        blob_path: format!("blobs/{label}.json"),
        payload: Some(format!(r#"{{"label":"{label}"}}"#)),
    }
}

// ARCH_DEBT: legacy oversized test helper awaiting refactor
#[allow(clippy::too_many_lines)]
pub(crate) fn seed_runtime_projection_attempt(registry: &Registry) -> (Uuid, Uuid, Uuid) {
    let project_id = Uuid::new_v4();
    let graph_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();
    let corr = CorrelationIds::for_graph_flow_task_attempt(
        project_id, graph_id, flow_id, task_id, attempt_id,
    );

    let events = [
        Event::new(
            EventPayload::AttemptStarted {
                flow_id,
                task_id,
                attempt_id,
                attempt_number: 1,
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::RuntimeStarted {
                adapter_name: "native".to_string(),
                role: RuntimeRole::Worker,
                task_id,
                attempt_id,
                prompt: "plan the task".to_string(),
                flags: vec!["--json".to_string()],
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::AgentInvocationStarted {
                native_correlation: NativeEventCorrelation {
                    project_id,
                    graph_id,
                    flow_id,
                    task_id,
                    attempt_id,
                },
                invocation_id: "inv-runtime-1".to_string(),
                adapter_name: "native".to_string(),
                provider: "mock".to_string(),
                model: "mock-model".to_string(),
                runtime_version: "1.0".to_string(),
                capture_mode: NativeEventPayloadCaptureMode::MetadataOnly,
                agent_mode: Some("plan".to_string()),
                allowed_tools: vec![],
                allowed_capabilities: vec![],
                configured_max_turns: Some(4),
                configured_timeout_budget_ms: None,
                configured_token_budget: None,
                configured_prompt_headroom: None,
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::NativeTurnSummaryRecorded {
                native_correlation: NativeEventCorrelation {
                    project_id,
                    graph_id,
                    flow_id,
                    task_id,
                    attempt_id,
                },
                invocation_id: "inv-runtime-1".to_string(),
                turn_index: 0,
                agent_mode: Some("plan".to_string()),
                from_state: "thinking".to_string(),
                to_state: "completed".to_string(),
                prompt_hash: None,
                context_manifest_hash: None,
                delivered_context_hash: None,
                mode_contract_hash: None,
                inputs_hash: None,
                prompt_headroom: None,
                available_budget: 0,
                rendered_prompt_bytes: 0,
                runtime_context_bytes: 0,
                static_prompt_chars: 0,
                selected_history_chars: 0,
                compacted_summary_chars: 0,
                code_navigation_chars: 0,
                tool_contract_chars: 0,
                assembly_duration_ms: 0,
                visible_item_count: 0,
                selected_history_count: 0,
                code_navigation_count: 0,
                compacted_summary_count: 0,
                tool_contract_count: 0,
                skipped_item_count: 0,
                truncated_item_count: 0,
                tool_result_items_visible: 0,
                latest_tool_result_turn_index: None,
                latest_tool_names_visible: vec![],
                tool_call_count: 0,
                tool_failure_count: 0,
                model_latency_ms: 0,
                tool_latency_ms: 0,
                turn_duration_ms: 0,
                elapsed_since_invocation_ms: 0,
                request_tokens: 0,
                response_tokens: 0,
                budget_used_before: 0,
                budget_used_after: 0,
                budget_remaining: 0,
                budget_thresholds_crossed: vec![],
                summary: Some("Drafted the implementation plan".to_string()),
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::RuntimeCommandObserved {
                attempt_id,
                stream: RuntimeOutputStream::Stdout,
                command: "cargo test runtime_projection".to_string(),
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::ToolCallRequested {
                native_correlation: NativeEventCorrelation {
                    project_id,
                    graph_id,
                    flow_id,
                    task_id,
                    attempt_id,
                },
                task_id: Some(task_id),
                invocation_id: "inv-runtime-1".to_string(),
                turn_index: 0,
                call_id: "call-exec-1".to_string(),
                tool_name: "run_command".to_string(),
                request: native_blob_ref("call-exec-1-request"),
                policy_tags: vec![
                    "approval_required:true".to_string(),
                    "approval_outcome:approved_for_session".to_string(),
                    "exec_danger_reason:write_outside_workspace".to_string(),
                ],
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::ToolCallCompleted {
                native_correlation: NativeEventCorrelation {
                    project_id,
                    graph_id,
                    flow_id,
                    task_id,
                    attempt_id,
                },
                task_id: Some(task_id),
                invocation_id: "inv-runtime-1".to_string(),
                turn_index: 0,
                call_id: "call-exec-1".to_string(),
                tool_name: "run_command".to_string(),
                response: native_blob_ref("call-exec-1-response"),
                duration_ms: Some(44),
                policy_tags: vec![
                    "approval_required:true".to_string(),
                    "approval_outcome:approved_for_session".to_string(),
                    "exec_danger_reason:write_outside_workspace".to_string(),
                ],
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::ToolCallRequested {
                native_correlation: NativeEventCorrelation {
                    project_id,
                    graph_id,
                    flow_id,
                    task_id,
                    attempt_id,
                },
                task_id: Some(task_id),
                invocation_id: "inv-runtime-1".to_string(),
                turn_index: 0,
                call_id: "call-net-1".to_string(),
                tool_name: "web_fetch".to_string(),
                request: native_blob_ref("call-net-1-request"),
                policy_tags: vec![
                    "network_approval_required:true".to_string(),
                    "network_target:api.github.com:443".to_string(),
                    "network_approval_outcome:deferred_pending".to_string(),
                ],
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::CheckpointCompleted {
                flow_id,
                task_id,
                attempt_id,
                checkpoint_id: "verify".to_string(),
                order: 1,
                commit_hash: "abc123".to_string(),
                timestamp: Utc::now(),
                summary: Some("checkpoint captured".to_string()),
            },
            corr.clone(),
        ),
        Event::new(
            EventPayload::RuntimeExited {
                attempt_id,
                exit_code: 0,
                duration_ms: 512,
            },
            corr,
        ),
    ];

    for event in events {
        registry
            .append_event(event, "server:test:seed_runtime_projection_attempt")
            .expect("append event");
    }

    (flow_id, task_id, attempt_id)
}
