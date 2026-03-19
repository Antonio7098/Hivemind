use crate::server::tests::support::{api_request, json_value, test_app};
use crate::server::{handle_api_request_inner, ApiMethod};

#[test]
fn api_version_ok() {
    let app = test_app();
    let resp = api_request(&app, ApiMethod::Get, "/api/version", None);
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert!(v["data"]["version"].is_string());
}

#[test]
fn api_state_ok_empty() {
    let app = test_app();
    let resp = api_request(&app, ApiMethod::Get, "/api/state", None);
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert!(v["data"]["projects"].is_array());
}

#[test]
fn api_unknown_endpoint_404() {
    let app = test_app();
    let resp = api_request(&app, ApiMethod::Get, "/api/nope", None);
    assert_eq!(resp.status_code, 404);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], false);
    assert_eq!(v["error"]["code"], "endpoint_not_found");
}

#[test]
fn api_post_project_create_ok() {
    let app = test_app();
    let body = serde_json::json!({
        "name": "proj-a",
        "description": "project from api"
    });
    let body = serde_json::to_vec(&body).expect("json body");
    let resp = api_request(&app, ApiMethod::Post, "/api/projects/create", Some(&body));
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert_eq!(v["data"]["name"], "proj-a");
}

#[test]
fn api_post_project_delete_ok() {
    let app = test_app();
    let create = serde_json::json!({ "name": "proj-delete" });
    let create = serde_json::to_vec(&create).expect("json body");
    let _ = api_request(&app, ApiMethod::Post, "/api/projects/create", Some(&create));

    let body = serde_json::json!({ "project": "proj-delete" });
    let body = serde_json::to_vec(&body).expect("json body");
    let resp = api_request(&app, ApiMethod::Post, "/api/projects/delete", Some(&body));
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert!(v["data"]["project_id"].is_string());
}

#[test]
fn api_runtime_stream_ok_empty() {
    let app = test_app();
    let resp = api_request(&app, ApiMethod::Get, "/api/runtime-stream", None);
    assert_eq!(resp.status_code, 200);
    let v = json_value(&resp.body);
    assert_eq!(v["success"], true);
    assert!(v["data"].is_array());
}

#[test]
fn api_worktree_restore_turn_requires_confirmation() {
    let app = test_app();
    let body = serde_json::json!({
        "attempt_id": uuid::Uuid::new_v4().to_string(),
        "ordinal": 1,
        "confirm": false
    });
    let body = serde_json::to_vec(&body).expect("json body");
    let err = handle_api_request_inner(&app, ApiMethod::Post, "/api/worktrees/restore-turn", 10, Some(&body))
        .expect_err("restore turn should require confirmation");
    assert_eq!(err.code, "confirmation_required");
}
