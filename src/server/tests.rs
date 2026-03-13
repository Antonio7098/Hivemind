use super::*;
use crate::core::events::{
    CorrelationIds, Event, EventPayload, NativeBlobRef, NativeEventCorrelation,
    NativeEventPayloadCaptureMode, RuntimeOutputStream, RuntimeRole,
};
use crate::core::registry::{Registry, RegistryConfig};
use crate::core::workflow::WorkflowStepKind;
use chrono::Utc;
use std::collections::BTreeMap;
use std::mem;
use std::path::Path;
use std::process::Command;
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

fn init_git_repo(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).expect("create repo dir");
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(repo_dir)
        .output()
        .expect("git init")
        .status
        .success());
    assert!(Command::new("git")
        .args(["config", "user.name", "Hivemind"])
        .current_dir(repo_dir)
        .output()
        .expect("git config name")
        .status
        .success());
    assert!(Command::new("git")
        .args(["config", "user.email", "hivemind@example.com"])
        .current_dir(repo_dir)
        .output()
        .expect("git config email")
        .status
        .success());
    std::fs::write(repo_dir.join("README.md"), "test\n").expect("write readme");
    assert!(Command::new("git")
        .args(["add", "."])
        .current_dir(repo_dir)
        .output()
        .expect("git add")
        .status
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo_dir)
        .output()
        .expect("git commit")
        .status
        .success());
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
                active_code_window_trace: Vec::new(),
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
fn api_chat_invoke_ok_with_mock_provider() {
    let reg = test_registry();

    let create = serde_json::json!({
        "name": "proj-chat",
        "description": "chat project"
    });
    let create = serde_json::to_vec(&create).expect("json body");
    let _ = handle_api_request_inner(
        ApiMethod::Post,
        "/api/projects/create",
        10,
        Some(&create),
        &reg,
    )
    .expect("create project");

    let body = serde_json::json!({
        "mode": "plan",
        "project": "proj-chat",
        "provider": "mock",
        "message": "Plan the next verification step",
        "history": [
            { "role": "user", "content": "We have a failing build." },
            { "role": "assistant", "content": "We should inspect the first compiler error." }
        ]
    });
    let body = serde_json::to_vec(&body).expect("json body");
    let resp = handle_api_request_inner(ApiMethod::Post, "/api/chat/invoke", 10, Some(&body), &reg)
        .expect("chat invoke response");

    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert_eq!(v["data"]["mode"], "plan");
    assert_eq!(v["data"]["provider"], "mock");
    assert_eq!(v["data"]["final_state"], "done");
    assert_eq!(
        v["data"]["assistant_message"],
        "native runtime completed deterministically"
    );
    assert!(v["data"]["project_id"].is_string());
    assert_eq!(v["data"]["turns"][0]["directive_kind"], "act");
    assert_eq!(v["data"]["turns"][1]["directive_kind"], "done");
}

#[test]
fn api_chat_sessions_create_send_and_inspect_round_trip() {
    let reg = test_registry();

    let create_project = serde_json::json!({
        "name": "proj-chat-session",
        "description": "chat session project"
    });
    let create_project = serde_json::to_vec(&create_project).expect("json body");
    let _ = handle_api_request_inner(
        ApiMethod::Post,
        "/api/projects/create",
        10,
        Some(&create_project),
        &reg,
    )
    .expect("create project");

    let create_session = serde_json::json!({
        "mode": "plan",
        "project": "proj-chat-session",
        "title": "Session test"
    });
    let create_session = serde_json::to_vec(&create_session).expect("json body");
    let create_resp = handle_api_request_inner(
        ApiMethod::Post,
        "/api/chat/sessions/create",
        10,
        Some(&create_session),
        &reg,
    )
    .expect("create chat session response");
    assert_eq!(create_resp.status_code, 200);
    let created = json_value(&create_resp.body);
    let session_id = created["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert_eq!(created["data"]["title"], "Session test");
    assert_eq!(created["data"]["mode"], "plan");

    let list_resp = handle_api_request_inner(
        ApiMethod::Get,
        "/api/chat/sessions?project=proj-chat-session",
        10,
        None,
        &reg,
    )
    .expect("list chat sessions response");
    assert_eq!(list_resp.status_code, 200);
    let listed = json_value(&list_resp.body);
    assert_eq!(listed["success"], true);
    assert_eq!(listed["data"][0]["session_id"], session_id);

    let send_payload = serde_json::json!({
        "session_id": session_id,
        "message": "Plan the next verification step",
        "provider": "mock"
    });
    let send_body = serde_json::to_vec(&send_payload).expect("json body");
    let send_resp = handle_api_request_inner(
        ApiMethod::Post,
        "/api/chat/sessions/send",
        10,
        Some(&send_body),
        &reg,
    )
    .expect("send chat message response");
    assert_eq!(send_resp.status_code, 200);
    let sent = json_value(&send_resp.body);
    assert_eq!(sent["success"], true);
    assert_eq!(sent["data"]["response"]["provider"], "mock");
    assert_eq!(sent["data"]["response"]["final_state"], "done");

    let inspect_path = format!("/api/chat/sessions/inspect?session_id={session_id}");
    let inspect_resp = handle_api_request_inner(ApiMethod::Get, &inspect_path, 10, None, &reg)
        .expect("inspect chat session response");
    assert_eq!(inspect_resp.status_code, 200);
    let inspected = json_value(&inspect_resp.body);
    assert_eq!(inspected["success"], true);
    assert_eq!(
        inspected["data"]["messages"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(inspected["data"]["messages"][0]["role"], "user");
    assert_eq!(inspected["data"]["messages"][1]["role"], "assistant");
    assert_eq!(
        inspected["data"]["messages"][1]["content"],
        "native runtime completed deterministically"
    );
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

#[allow(clippy::too_many_lines)]
#[test]
fn workflow_endpoints_create_update_run_and_complete() {
    let registry = test_registry();
    let project = registry
        .create_project("workflow-api", None)
        .expect("project");

    let create_body = serde_json::to_vec(&serde_json::json!({
        "project": project.id.to_string(),
        "name": "api-workflow",
        "description": "workflow foundation"
    }))
    .expect("create workflow body");
    let create_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/create",
        Some(create_body.as_slice()),
    );
    let created_workflow = json_value(&create_resp.body);
    let workflow_id = created_workflow["data"]["id"]
        .as_str()
        .expect("workflow id");

    let update_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow_id,
        "description": "workflow updated"
    }))
    .expect("update workflow body");
    let _update_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/update",
        Some(update_body.as_slice()),
    );

    let step_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow_id,
        "name": "root-step",
        "kind": "task"
    }))
    .expect("step add body");
    let step_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/steps/add",
        Some(step_body.as_slice()),
    );
    let step_workflow = json_value(&step_resp.body);
    let step_id = step_workflow["data"]["steps"]
        .as_object()
        .and_then(|steps| steps.values().next())
        .and_then(|step| step.get("id"))
        .and_then(serde_json::Value::as_str)
        .expect("step id");

    let run_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow_id,
    }))
    .expect("create run body");
    let run_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/create",
        Some(run_body.as_slice()),
    );
    let created_run = json_value(&run_resp.body);
    let run_id = created_run["data"]["id"].as_str().expect("run id");

    let start_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
    }))
    .expect("start run body");
    let _start_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/start",
        Some(start_body.as_slice()),
    );

    let list_resp = api_request(&registry, ApiMethod::Get, "/api/workflow-runs", None);
    let listed_runs = json_value(&list_resp.body);
    assert_eq!(listed_runs["data"].as_array().unwrap().len(), 1);

    let inspect_resp = api_request(
        &registry,
        ApiMethod::Get,
        &format!("/api/workflow-runs/inspect?workflow_run_id={run_id}"),
        None,
    );
    let inspected_run = json_value(&inspect_resp.body);
    assert_eq!(inspected_run["data"]["state"], "running");

    let step_running_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
        "step_id": step_id,
        "state": "running"
    }))
    .expect("step running body");
    let _step_running_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/steps/state",
        Some(step_running_body.as_slice()),
    );

    let step_succeeded_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
        "step_id": step_id,
        "state": "succeeded"
    }))
    .expect("step succeeded body");
    let _step_succeeded_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/steps/state",
        Some(step_succeeded_body.as_slice()),
    );

    let complete_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
    }))
    .expect("complete run body");
    let _complete_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/complete",
        Some(complete_body.as_slice()),
    );

    let completed_resp = api_request(
        &registry,
        ApiMethod::Get,
        &format!("/api/workflow-runs/inspect?workflow_run_id={run_id}"),
        None,
    );
    let completed_run = json_value(&completed_resp.body);
    assert_eq!(completed_run["data"]["state"], "completed");
}

#[test]
fn workflow_spec_endpoints_validate_and_bind() {
    let registry = test_registry();
    let project = registry
        .create_project("workflow-api-spec", None)
        .expect("project");
    let workflow = registry
        .create_workflow(&project.id.to_string(), "api-spec-workflow", Some("spec"))
        .expect("workflow");
    let workflow = registry
        .workflow_add_step(
            &workflow.id.to_string(),
            "leaf",
            WorkflowStepKind::Task,
            None,
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("step");
    let step_id = workflow.steps.values().next().unwrap().id;

    let spec = serde_json::json!({
        "schema": "workflow_spec",
        "schema_version": 1,
        "root": {
            "id": "root-spec",
            "kind": "workflow",
            "title": "Root",
            "intent": "Describe the workflow",
            "workflow_id": workflow.id,
            "acceptance_criteria": ["Workflow succeeds"],
            "verification": {
                "posture": "manual"
            },
            "children": [
                {
                    "id": "leaf-spec",
                    "kind": "task",
                    "title": "Leaf",
                    "intent": "Handle the leaf step",
                    "workflow_id": workflow.id,
                    "step_id": step_id,
                    "acceptance_criteria": ["Leaf succeeds"],
                    "verification": {
                        "posture": "worker_output"
                    }
                }
            ]
        }
    });

    let validate_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow.id.to_string(),
        "spec": spec
    }))
    .expect("validate body");
    let validate_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/spec/validate",
        Some(validate_body.as_slice()),
    );
    assert_eq!(validate_resp.status_code, 200);
    let validated = json_value(&validate_resp.body);
    assert_eq!(validated["data"]["root"]["id"], "root-spec");

    let bind_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow.id.to_string(),
        "spec": spec
    }))
    .expect("bind body");
    let bind_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/spec/bind",
        Some(bind_body.as_slice()),
    );
    assert_eq!(bind_resp.status_code, 200);

    let inspect_resp = api_request(
        &registry,
        ApiMethod::Get,
        &format!("/api/workflows/inspect?workflow_id={}", workflow.id),
        None,
    );
    let inspected = json_value(&inspect_resp.body);
    assert_eq!(inspected["data"]["spec"]["root"]["id"], "root-spec");
}

#[test]
#[allow(clippy::too_many_lines)]
fn workflow_endpoints_tick_pause_resume_and_abort() {
    let registry = test_registry();
    let project = registry
        .create_project("workflow-api-lifecycle", None)
        .expect("project");

    let create_body = serde_json::to_vec(&serde_json::json!({
        "project": project.id.to_string(),
        "name": "api-workflow-lifecycle"
    }))
    .expect("create workflow body");
    let create_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/create",
        Some(create_body.as_slice()),
    );
    let created_workflow = json_value(&create_resp.body);
    let workflow_id = created_workflow["data"]["id"]
        .as_str()
        .expect("workflow id");

    let step_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow_id,
        "name": "root-step",
        "kind": "task"
    }))
    .expect("step add body");
    let _step_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/steps/add",
        Some(step_body.as_slice()),
    );

    let run_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow_id,
    }))
    .expect("create run body");
    let run_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/create",
        Some(run_body.as_slice()),
    );
    let created_run = json_value(&run_resp.body);
    let run_id = created_run["data"]["id"].as_str().expect("run id");

    let run_id_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
    }))
    .expect("run id body");
    let _start_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/start",
        Some(run_id_body.as_slice()),
    );

    let tick_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
        "max_parallel": 1
    }))
    .expect("tick body");
    let tick_err = handle_api_request_inner(
        ApiMethod::Post,
        "/api/workflow-runs/tick",
        10,
        Some(tick_body.as_slice()),
        &registry,
    )
    .expect_err("tick should fail without runtime");
    assert_eq!(tick_err.code, "runtime_not_configured");

    let pause_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/pause",
        Some(run_id_body.as_slice()),
    );
    let paused_run = json_value(&pause_resp.body);
    assert_eq!(paused_run["data"]["state"], "paused");

    let resume_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/resume",
        Some(run_id_body.as_slice()),
    );
    let resumed_run = json_value(&resume_resp.body);
    assert_eq!(resumed_run["data"]["state"], "running");

    let abort_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
        "reason": "stop-now"
    }))
    .expect("abort body");
    let abort_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/abort",
        Some(abort_body.as_slice()),
    );
    let aborted_run = json_value(&abort_resp.body);
    assert_eq!(aborted_run["data"]["state"], "aborted");

    let inspect_resp = api_request(
        &registry,
        ApiMethod::Get,
        &format!("/api/workflow-runs/inspect?workflow_run_id={run_id}"),
        None,
    );
    let inspected_run = json_value(&inspect_resp.body);
    assert_eq!(inspected_run["data"]["state"], "aborted");
}

#[test]
fn workflow_run_inspect_endpoint_includes_nested_lineage_tree() {
    let registry = test_registry();
    let project = registry
        .create_project("workflow-api-nested", None)
        .expect("project");

    let child = registry
        .create_workflow(&project.id.to_string(), "child-workflow", Some("child"))
        .expect("child workflow");
    let parent = registry
        .create_workflow(&project.id.to_string(), "parent-workflow", Some("parent"))
        .expect("parent workflow");
    let child_id = child.id.to_string();
    let parent = registry
        .workflow_add_step(
            &parent.id.to_string(),
            "child-step",
            WorkflowStepKind::Workflow,
            None,
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some(&child_id),
            None,
            None,
        )
        .expect("workflow child step");

    let parent_run = registry
        .create_workflow_run(&parent.id.to_string(), None, None, BTreeMap::new())
        .expect("parent run");
    let parent_run = registry
        .start_workflow_run(&parent_run.id.to_string())
        .expect("start parent");
    let parent_run = registry
        .tick_workflow_run(&parent_run.id.to_string(), false, Some(1))
        .expect("tick parent");

    let inspect_resp = api_request(
        &registry,
        ApiMethod::Get,
        &format!(
            "/api/workflow-runs/inspect?workflow_run_id={}",
            parent_run.id
        ),
        None,
    );
    let inspected_run = json_value(&inspect_resp.body);
    assert_eq!(inspected_run["data"]["state"], "completed");
    assert_eq!(inspected_run["data"]["workflow_name"], "parent-workflow");
    let child_runs = inspected_run["data"]["child_runs"]
        .as_array()
        .expect("child runs array");
    assert_eq!(child_runs.len(), 1, "{inspected_run}");
    assert_eq!(child_runs[0]["workflow_name"], "child-workflow");
    assert_eq!(child_runs[0]["parent_step_name"], "child-step");
    assert_eq!(
        child_runs[0]["root_workflow_run_id"],
        parent_run.id.to_string()
    );
    assert!(child_runs[0]["children"]
        .as_array()
        .is_some_and(|children| children.is_empty()));
}

#[test]
fn workflow_endpoints_reject_unknown_step_dependency() {
    let registry = test_registry();
    let project = registry
        .create_project("workflow-api-deps", None)
        .expect("project");

    let create_body = serde_json::to_vec(&serde_json::json!({
        "project": project.id.to_string(),
        "name": "api-workflow-deps"
    }))
    .expect("create workflow body");
    let create_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/create",
        Some(create_body.as_slice()),
    );
    let workflow_id = json_value(&create_resp.body)["data"]["id"]
        .as_str()
        .expect("workflow id")
        .to_string();

    let step_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow_id,
        "name": "dependent-step",
        "kind": "task",
        "depends_on": [Uuid::new_v4().to_string()]
    }))
    .expect("step add body");
    let err = handle_api_request_inner(
        ApiMethod::Post,
        "/api/workflows/steps/add",
        10,
        Some(step_body.as_slice()),
        &registry,
    )
    .expect_err("unknown dependency should fail");

    assert_eq!(err.code, "workflow_step_dependency_not_found");
}

#[test]
fn workflow_endpoints_reject_invalid_step_transition() {
    let registry = test_registry();
    let project = registry
        .create_project("workflow-api-transition", None)
        .expect("project");

    let create_body = serde_json::to_vec(&serde_json::json!({
        "project": project.id.to_string(),
        "name": "api-workflow-transition"
    }))
    .expect("create workflow body");
    let create_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/create",
        Some(create_body.as_slice()),
    );
    let workflow_id = json_value(&create_resp.body)["data"]["id"]
        .as_str()
        .expect("workflow id")
        .to_string();

    let step_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow_id,
        "name": "step-a",
        "kind": "task"
    }))
    .expect("step add body");
    let step_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflows/steps/add",
        Some(step_body.as_slice()),
    );
    let step_id = json_value(&step_resp.body)["data"]["steps"]
        .as_object()
        .and_then(|steps| steps.values().next())
        .and_then(|step| step.get("id"))
        .and_then(serde_json::Value::as_str)
        .expect("step id")
        .to_string();

    let run_body = serde_json::to_vec(&serde_json::json!({
        "workflow_id": workflow_id,
    }))
    .expect("create run body");
    let run_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/create",
        Some(run_body.as_slice()),
    );
    let run_id = json_value(&run_resp.body)["data"]["id"]
        .as_str()
        .expect("run id")
        .to_string();

    let start_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
    }))
    .expect("start body");
    let _start_resp = api_request(
        &registry,
        ApiMethod::Post,
        "/api/workflow-runs/start",
        Some(start_body.as_slice()),
    );

    let invalid_transition_body = serde_json::to_vec(&serde_json::json!({
        "workflow_run_id": run_id,
        "step_id": step_id,
        "state": "ready"
    }))
    .expect("invalid transition body");
    let err = handle_api_request_inner(
        ApiMethod::Post,
        "/api/workflow-runs/steps/state",
        10,
        Some(invalid_transition_body.as_slice()),
        &registry,
    )
    .expect_err("pending to ready should fail");

    assert_eq!(err.code, "invalid_workflow_step_transition");
}

#[test]
fn api_runtime_stream_accepts_workflow_run_id() {
    let registry = test_registry();
    let project = registry
        .create_project("workflow-api-runtime-stream", None)
        .expect("project");

    let workflow = registry
        .create_workflow(&project.id.to_string(), "runtime-stream", None)
        .expect("workflow");
    registry
        .workflow_add_step(
            &workflow.id.to_string(),
            "step-a",
            crate::core::workflow::WorkflowStepKind::Task,
            None,
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("step");

    let run = registry
        .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
        .expect("run");
    registry
        .start_workflow_run(&run.id.to_string())
        .expect("start workflow");
    let _ = registry.tick_workflow_run(&run.id.to_string(), false, Some(1));

    let resp = api_request(
        &registry,
        ApiMethod::Get,
        &format!(
            "/api/runtime-stream?workflow_run_id={}&limit=5&detail=summary",
            run.id
        ),
        None,
    );
    let json = json_value(&resp.body);
    assert!(json["data"].is_array());
}

#[test]
fn api_worktrees_accept_workflow_run_id_and_step_id() {
    let registry = test_registry();
    let project = registry
        .create_project("workflow-api-worktrees", None)
        .expect("project");
    let repo_tmp = tempfile::tempdir().expect("tempdir");
    let repo_dir = repo_tmp.path().join("repo");
    init_git_repo(&repo_dir);
    registry
        .attach_repo(
            &project.id.to_string(),
            repo_dir.to_str().expect("repo path"),
            Some("primary"),
            crate::core::scope::RepoAccessMode::ReadWrite,
        )
        .expect("attach repo");

    let workflow = registry
        .create_workflow(&project.id.to_string(), "worktrees", None)
        .expect("workflow");
    let workflow = registry
        .workflow_add_step(
            &workflow.id.to_string(),
            "step-a",
            crate::core::workflow::WorkflowStepKind::Task,
            None,
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("step");
    let step_id = workflow.steps.values().next().expect("step exists").id;

    let run = registry
        .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
        .expect("run");
    registry
        .start_workflow_run(&run.id.to_string())
        .expect("start workflow");
    let _ = registry.tick_workflow_run(&run.id.to_string(), false, Some(1));

    let list_resp = api_request(
        &registry,
        ApiMethod::Get,
        &format!("/api/worktrees?workflow_run_id={}", run.id),
        None,
    );
    let listed = json_value(&list_resp.body);
    let statuses = listed["data"].as_array().expect("status array");
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0]["workflow_run_id"], run.id.to_string());
    assert_eq!(statuses[0]["step_id"], step_id.to_string());

    let inspect_resp = api_request(
        &registry,
        ApiMethod::Get,
        &format!(
            "/api/worktrees/inspect?workflow_run_id={}&step_id={}",
            run.id, step_id
        ),
        None,
    );
    let inspected = json_value(&inspect_resp.body);
    assert_eq!(inspected["data"]["workflow_run_id"], run.id.to_string());
    assert_eq!(inspected["data"]["step_id"], step_id.to_string());
}
