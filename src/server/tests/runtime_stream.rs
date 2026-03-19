use crate::server::tests::support::{api_request, json_value, seed_runtime_projection_attempt, test_app};
use crate::server::ApiMethod;

#[test]
fn api_runtime_stream_returns_projected_runtime_items() {
    let app = test_app();
    let reg = app.open_registry().expect("registry");
    let (flow_id, _task_id, attempt_id) = seed_runtime_projection_attempt(&reg);
    let resp = api_request(
        &app,
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
    let app = test_app();
    let reg = app.open_registry().expect("registry");
    let (flow_id, _task_id, attempt_id) = seed_runtime_projection_attempt(&reg);
    let summary_resp = api_request(
        &app,
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
    let observability_resp = api_request(&app, ApiMethod::Get, &format!("/api/runtime-stream?attempt_id={attempt_id}&flow_id={flow_id}&limit=20&detail=observability"), None);
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
