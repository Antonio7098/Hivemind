use super::*;
use crate::core::events::{
    CorrelationIds, Event, EventPayload, NativeBlobRef, NativeEventCorrelation,
    NativeEventPayloadCaptureMode, RuntimeOutputStream, RuntimeRole,
};
use crate::core::registry::{Registry, RegistryConfig};
use chrono::Utc;
use std::mem;
use uuid::Uuid;

fn test_registry() -> Registry {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().to_path_buf();
    mem::forget(tmp);
    let config = RegistryConfig::with_dir(data_dir);
    Registry::open_with_config(config).expect("registry")
}

fn json_value(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("json")
}

fn api_request(
    registry: &Registry,
    method: ApiMethod,
    url: &str,
    body: Option<&[u8]>,
) -> ApiResponse {
    handle_api_request_inner(method, url, 10, body, registry).expect("api response")
}

fn native_blob_ref(label: &str) -> NativeBlobRef {
    NativeBlobRef {
        digest: format!("digest-{label}"),
        byte_size: label.len() as u64,
        media_type: "application/json".to_string(),
        blob_path: format!("blobs/{label}.json"),
        payload: Some(format!(r#"{{"label":"{label}"}}"#)),
    }
}

#[allow(clippy::too_many_lines)]
fn seed_runtime_projection_attempt(registry: &Registry) -> (Uuid, Uuid, Uuid) {
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

#[test]
fn api_version_ok() {
    let reg = test_registry();
    let resp = handle_api_request_inner(ApiMethod::Get, "/api/version", 10, None, &reg).unwrap();
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert!(v["data"]["version"].is_string());
}

#[test]
fn api_state_ok_empty() {
    let reg = test_registry();
    let resp = handle_api_request_inner(ApiMethod::Get, "/api/state", 10, None, &reg).unwrap();
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert!(v["data"]["projects"].is_array());
}

#[test]
fn api_unknown_endpoint_404() {
    let reg = test_registry();
    let resp = handle_api_request_inner(ApiMethod::Get, "/api/nope", 10, None, &reg).unwrap();
    assert_eq!(resp.status_code, 404);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], false);
    assert_eq!(v["error"]["code"], "endpoint_not_found");
}

#[test]
fn api_post_project_create_ok() {
    let reg = test_registry();
    let body = serde_json::json!({
        "name": "proj-a",
        "description": "project from api"
    });
    let body = serde_json::to_vec(&body).expect("json body");
    let resp = handle_api_request_inner(
        ApiMethod::Post,
        "/api/projects/create",
        10,
        Some(&body),
        &reg,
    )
    .unwrap();
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert_eq!(v["data"]["name"], "proj-a");
}

#[test]
fn api_post_project_delete_ok() {
    let reg = test_registry();
    let create = serde_json::json!({ "name": "proj-delete" });
    let create = serde_json::to_vec(&create).expect("json body");
    let _ = handle_api_request_inner(
        ApiMethod::Post,
        "/api/projects/create",
        10,
        Some(&create),
        &reg,
    )
    .unwrap();

    let body = serde_json::json!({ "project": "proj-delete" });
    let body = serde_json::to_vec(&body).expect("json body");
    let resp = handle_api_request_inner(
        ApiMethod::Post,
        "/api/projects/delete",
        10,
        Some(&body),
        &reg,
    )
    .unwrap();
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert!(v["data"]["project_id"].is_string());
}

#[test]
fn api_runtime_stream_ok_empty() {
    let reg = test_registry();
    let resp = handle_api_request_inner(ApiMethod::Get, "/api/runtime-stream", 10, None, &reg)
        .expect("runtime stream response");
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert!(v["data"].is_array());
}

#[test]
fn api_worktree_restore_turn_requires_confirmation() {
    let reg = test_registry();
    let body = serde_json::json!({
        "attempt_id": uuid::Uuid::new_v4().to_string(),
        "ordinal": 1,
        "confirm": false
    });
    let body = serde_json::to_vec(&body).expect("json body");
    let err = handle_api_request_inner(
        ApiMethod::Post,
        "/api/worktrees/restore-turn",
        10,
        Some(&body),
        &reg,
    )
    .expect_err("restore turn should require confirmation");
    assert_eq!(err.code, "confirmation_required");
}

#[test]
fn api_runtime_stream_returns_projected_runtime_items() {
    let reg = test_registry();
    let (flow_id, _task_id, attempt_id) = seed_runtime_projection_attempt(&reg);

    let resp = api_request(
        &reg,
        ApiMethod::Get,
        &format!("/api/runtime-stream?attempt_id={attempt_id}&flow_id={flow_id}&limit=6"),
        None,
    );

    assert_eq!(resp.status_code, 200);
    let body = json_value(&resp.body);
    let items = body["data"].as_array().expect("runtime items");
    assert_eq!(items.len(), 6);
    assert_eq!(items[0]["kind"], "command");
    assert_eq!(items[1]["kind"], "approval");
    assert_eq!(items[2]["kind"], "tool_call_completed");
    assert_eq!(items[3]["kind"], "approval");
    assert_eq!(items[4]["kind"], "checkpoint_completed");
    assert_eq!(items[5]["kind"], "runtime_exited");
}

#[test]
fn api_runtime_stream_supports_detail_levels() {
    let reg = test_registry();
    let (flow_id, _task_id, attempt_id) = seed_runtime_projection_attempt(&reg);

    let summary_resp = api_request(
        &reg,
        ApiMethod::Get,
        &format!(
            "/api/runtime-stream?attempt_id={attempt_id}&flow_id={flow_id}&limit=20&detail=summary"
        ),
        None,
    );
    assert_eq!(summary_resp.status_code, 200);
    let summary = json_value(&summary_resp.body);
    let summary_items = summary["data"].as_array().expect("summary runtime items");
    assert!(summary_items.iter().any(|item| item["kind"] == "turn"));
    assert!(summary_items.iter().any(|item| item["kind"] == "approval"));
    assert!(summary_items
        .iter()
        .any(|item| item["kind"] == "checkpoint_completed"));
    assert!(!summary_items.iter().any(|item| item["kind"] == "command"));
    assert!(!summary_items
        .iter()
        .any(|item| item["kind"] == "tool_call_completed"));

    let observability_resp = api_request(
        &reg,
        ApiMethod::Get,
        &format!(
            "/api/runtime-stream?attempt_id={attempt_id}&flow_id={flow_id}&limit=20&detail=observability"
        ),
        None,
    );
    assert_eq!(observability_resp.status_code, 200);
    let observability = json_value(&observability_resp.body);
    let observability_items = observability["data"]
        .as_array()
        .expect("observability runtime items");
    assert!(observability_items
        .iter()
        .any(|item| item["kind"] == "command"));
    assert!(observability_items
        .iter()
        .any(|item| item["kind"] == "tool_call_completed"));
    assert!(!observability_items
        .iter()
        .any(|item| item["kind"] == "output"));
}
