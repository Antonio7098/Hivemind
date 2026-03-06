use super::*;
use crate::core::registry::{Registry, RegistryConfig};
use std::mem;

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
