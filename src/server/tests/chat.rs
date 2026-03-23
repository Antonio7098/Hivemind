use crate::server::tests::support::{api_request, json_value, test_app};
use crate::server::ApiMethod;

#[test]
fn api_chat_invoke_ok_with_mock_provider() {
    let app = test_app();
    let create = serde_json::json!({"name": "proj-chat", "description": "chat project"});
    let create = serde_json::to_vec(&create).expect("json body");
    let _ = api_request(&app, ApiMethod::Post, "/api/projects/create", Some(&create));
    let body = serde_json::json!({"mode": "plan", "project": "proj-chat", "provider": "mock", "message": "Plan the next verification step", "history": [{"role": "user", "content": "We have a failing build."}, {"role": "assistant", "content": "We should inspect the first compiler error."}]});
    let body = serde_json::to_vec(&body).expect("json body");
    let resp = api_request(&app, ApiMethod::Post, "/api/chat/invoke", Some(&body));
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
    let app = test_app();
    let create_project =
        serde_json::json!({"name": "proj-chat-session", "description": "chat session project"});
    let create_project = serde_json::to_vec(&create_project).expect("json body");
    let _ = api_request(
        &app,
        ApiMethod::Post,
        "/api/projects/create",
        Some(&create_project),
    );
    let create_session = serde_json::json!({"mode": "plan", "project": "proj-chat-session", "title": "Session test"});
    let create_session = serde_json::to_vec(&create_session).expect("json body");
    let create_resp = api_request(
        &app,
        ApiMethod::Post,
        "/api/chat/sessions/create",
        Some(&create_session),
    );
    assert_eq!(create_resp.status_code, 200);
    let created = json_value(&create_resp.body);
    let session_id = created["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert_eq!(created["data"]["title"], "Session test");
    assert_eq!(created["data"]["mode"], "plan");
    let list_resp = api_request(
        &app,
        ApiMethod::Get,
        "/api/chat/sessions?project=proj-chat-session",
        None,
    );
    assert_eq!(list_resp.status_code, 200);
    let listed = json_value(&list_resp.body);
    assert_eq!(listed["success"], true);
    assert_eq!(listed["data"][0]["session_id"], session_id);
    let send_payload = serde_json::json!({"session_id": session_id, "message": "Plan the next verification step", "provider": "mock"});
    let send_body = serde_json::to_vec(&send_payload).expect("json body");
    let send_resp = api_request(
        &app,
        ApiMethod::Post,
        "/api/chat/sessions/send",
        Some(&send_body),
    );
    assert_eq!(send_resp.status_code, 200);
    let sent = json_value(&send_resp.body);
    assert_eq!(sent["success"], true);
    assert_eq!(sent["data"]["response"]["provider"], "mock");
    assert_eq!(sent["data"]["response"]["final_state"], "done");
    let inspect_path = format!("/api/chat/sessions/inspect?session_id={session_id}");
    let inspect_resp = api_request(&app, ApiMethod::Get, &inspect_path, None);
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
