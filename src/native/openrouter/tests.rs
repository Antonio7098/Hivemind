use super::retry::{
    OPENROUTER_RETRY_BASE_DELAY_MS_ENV, OPENROUTER_RETRY_MAX_ATTEMPTS_ENV,
    OPENROUTER_STREAM_IDLE_TIMEOUT_MS_ENV,
};
use super::transport::OPENROUTER_FALLBACK_ENDPOINT_ENV;
use super::OpenRouterModelClient;
use crate::native::{AgentLoopState, ModelClient, ModelTurnRequest, NativeRuntimeError};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

#[derive(Clone)]
struct MockHttpResponse {
    status: u16,
    body: String,
    delay_ms: u64,
}

fn http_reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

fn spawn_mock_http_server(
    responses: Vec<MockHttpResponse>,
) -> Option<(String, std::thread::JoinHandle<()>)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("skipping network transport test: {error}");
            return None;
        }
    };
    let addr = listener.local_addr().expect("server addr");
    let handle = std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));

            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);
            if response.delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(response.delay_ms));
            }

            let reason = http_reason_phrase(response.status);
            let payload = response.body;
            let response_text = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.status,
                reason,
                payload.len(),
                payload
            );
            let _ = stream.write_all(response_text.as_bytes());
            let _ = stream.flush();
        }
    });

    Some((format!("http://{addr}"), handle))
}

fn openrouter_success_body(content: &str, finish_reason: &str) -> String {
    serde_json::json!({
        "choices": [
            {
                "message": { "content": content },
                "finish_reason": finish_reason
            }
        ]
    })
    .to_string()
}

#[test]
fn openrouter_normalize_directive_extracts_prefix_line() {
    let raw = "```text\nTHINK:plan next step\n```";
    let normalized = OpenRouterModelClient::normalize_directive(raw);
    assert_eq!(normalized, "THINK:plan next step");
}

#[test]
fn openrouter_extract_text_content_supports_array_segments() {
    let body = serde_json::json!({
        "choices": [
            {
                "message": {
                    "content": [
                        {"type": "text", "text": "ACT:tool:list_files:{\"path\":\".\",\"recursive\":false}"}
                    ]
                }
            }
        ]
    });
    let content = OpenRouterModelClient::extract_text_content(&body).expect("content");
    assert!(content.starts_with("ACT:tool:list_files:"));
}

#[test]
fn openrouter_normalize_model_id_strips_provider_prefix() {
    let normalized = OpenRouterModelClient::normalize_model_id(
        "openrouter/meta-llama/llama-3.2-3b-instruct:free",
    );
    assert_eq!(normalized, "meta-llama/llama-3.2-3b-instruct:free");
}

#[test]
fn openrouter_retries_rate_limit_with_backoff_and_telemetry() {
    let Some((endpoint, server_handle)) = spawn_mock_http_server(vec![
        MockHttpResponse {
            status: 429,
            body: serde_json::json!({"error": { "message": "rate limited" }}).to_string(),
            delay_ms: 0,
        },
        MockHttpResponse {
            status: 200,
            body: openrouter_success_body(
                "ACT:tool:list_files:{\"path\":\".\",\"recursive\":false}",
                "stop",
            ),
            delay_ms: 0,
        },
    ]) else {
        return;
    };

    let mut env = HashMap::new();
    env.insert("OPENROUTER_API_KEY".to_string(), "test-key".to_string());
    env.insert("OPENROUTER_API_BASE_URL".to_string(), endpoint);
    env.insert(
        OPENROUTER_RETRY_MAX_ATTEMPTS_ENV.to_string(),
        "3".to_string(),
    );
    env.insert(
        OPENROUTER_RETRY_BASE_DELAY_MS_ENV.to_string(),
        "1".to_string(),
    );
    let mut client = OpenRouterModelClient::from_env("openrouter/test-model", &env)
        .expect("client should initialize");
    let request = ModelTurnRequest {
        turn_index: 0,
        state: AgentLoopState::Think,
        prompt: "run".to_string(),
        context: None,
    };
    let directive = client
        .complete_turn(&request)
        .expect("request should recover");
    assert_eq!(
        directive,
        "ACT:tool:list_files:{\"path\":\".\",\"recursive\":false}"
    );

    let telemetry = client.take_transport_telemetry();
    assert_eq!(telemetry.attempts.len(), 1);
    assert_eq!(telemetry.attempts[0].code, "native_transport_http_429");
    assert!(telemetry.attempts[0].retryable);
    assert!(telemetry.attempts[0].rate_limited);
    assert!(
        telemetry.attempts[0]
            .backoff_ms
            .expect("retry should include backoff")
            >= 1
    );
    assert!(telemetry.fallback_activations.is_empty());

    server_handle.join().expect("server thread should join");
}

#[test]
fn openrouter_activates_fallback_transport_after_primary_failure() {
    let Some((primary_endpoint, primary_handle)) = spawn_mock_http_server(vec![MockHttpResponse {
        status: 503,
        body: serde_json::json!({"error": { "message": "primary unavailable" }}).to_string(),
        delay_ms: 0,
    }]) else {
        return;
    };
    let Some((fallback_endpoint, fallback_handle)) = spawn_mock_http_server(vec![
        MockHttpResponse {
            status: 200,
            body: openrouter_success_body("THINK:using fallback", "stop"),
            delay_ms: 0,
        },
        MockHttpResponse {
            status: 200,
            body: openrouter_success_body("DONE:fallback stayed active", "stop"),
            delay_ms: 0,
        },
    ]) else {
        return;
    };

    let mut env = HashMap::new();
    env.insert("OPENROUTER_API_KEY".to_string(), "test-key".to_string());
    env.insert("OPENROUTER_API_BASE_URL".to_string(), primary_endpoint);
    env.insert(
        OPENROUTER_FALLBACK_ENDPOINT_ENV.to_string(),
        fallback_endpoint,
    );
    env.insert(
        OPENROUTER_RETRY_MAX_ATTEMPTS_ENV.to_string(),
        "3".to_string(),
    );
    env.insert(
        OPENROUTER_RETRY_BASE_DELAY_MS_ENV.to_string(),
        "1".to_string(),
    );

    let mut client = OpenRouterModelClient::from_env("openrouter/test-model", &env)
        .expect("client should initialize");
    let first = ModelTurnRequest {
        turn_index: 0,
        state: AgentLoopState::Think,
        prompt: "first".to_string(),
        context: None,
    };
    let second = ModelTurnRequest {
        turn_index: 1,
        state: AgentLoopState::Act,
        prompt: "second".to_string(),
        context: None,
    };

    let first_out = client
        .complete_turn(&first)
        .expect("fallback should recover");
    assert_eq!(first_out, "THINK:using fallback");
    let second_out = client
        .complete_turn(&second)
        .expect("fallback should stay active");
    assert_eq!(second_out, "DONE:fallback stayed active");

    let telemetry = client.take_transport_telemetry();
    assert_eq!(telemetry.attempts.len(), 1);
    assert_eq!(telemetry.attempts[0].code, "native_transport_http_5xx");
    assert_eq!(telemetry.fallback_activations.len(), 1);
    assert_eq!(
        telemetry.fallback_activations[0].from_transport,
        "http_primary"
    );
    assert_eq!(
        telemetry.fallback_activations[0].to_transport,
        "http_fallback"
    );
    assert_eq!(telemetry.active_transport.as_deref(), Some("http_fallback"));

    primary_handle.join().expect("primary server should join");
    fallback_handle.join().expect("fallback server should join");
}

#[test]
fn openrouter_classifies_incomplete_terminal_event() {
    let Some((endpoint, server_handle)) = spawn_mock_http_server(vec![MockHttpResponse {
        status: 200,
        body: openrouter_success_body(
            "ACT:tool:list_files:{\"path\":\".\",\"recursive\":false}",
            "length",
        ),
        delay_ms: 0,
    }]) else {
        return;
    };

    let mut env = HashMap::new();
    env.insert("OPENROUTER_API_KEY".to_string(), "test-key".to_string());
    env.insert("OPENROUTER_API_BASE_URL".to_string(), endpoint);
    env.insert(
        OPENROUTER_RETRY_MAX_ATTEMPTS_ENV.to_string(),
        "1".to_string(),
    );
    let mut client = OpenRouterModelClient::from_env("openrouter/test-model", &env)
        .expect("client should initialize");
    let request = ModelTurnRequest {
        turn_index: 0,
        state: AgentLoopState::Think,
        prompt: "run".to_string(),
        context: None,
    };
    let err = client
        .complete_turn(&request)
        .expect_err("incomplete terminal event should fail");
    let NativeRuntimeError::ModelRequestFailed {
        code, recoverable, ..
    } = err
    else {
        panic!("expected model request failure");
    };
    assert_eq!(code, "native_stream_terminal_incomplete");
    assert!(recoverable);

    let telemetry = client.take_transport_telemetry();
    assert_eq!(telemetry.attempts.len(), 1);
    assert_eq!(
        telemetry.attempts[0].code,
        "native_stream_terminal_incomplete"
    );
    assert!(telemetry.attempts[0].backoff_ms.is_none());

    server_handle.join().expect("server thread should join");
}

#[test]
fn openrouter_classifies_idle_timeout() {
    let Some((endpoint, server_handle)) = spawn_mock_http_server(vec![MockHttpResponse {
        status: 200,
        body: openrouter_success_body("DONE:late response", "stop"),
        delay_ms: 200,
    }]) else {
        return;
    };

    let mut env = HashMap::new();
    env.insert("OPENROUTER_API_KEY".to_string(), "test-key".to_string());
    env.insert("OPENROUTER_API_BASE_URL".to_string(), endpoint);
    env.insert(
        OPENROUTER_RETRY_MAX_ATTEMPTS_ENV.to_string(),
        "1".to_string(),
    );
    env.insert(
        OPENROUTER_STREAM_IDLE_TIMEOUT_MS_ENV.to_string(),
        "50".to_string(),
    );
    let mut client = OpenRouterModelClient::from_env("openrouter/test-model", &env)
        .expect("client should initialize");
    let request = ModelTurnRequest {
        turn_index: 0,
        state: AgentLoopState::Think,
        prompt: "run".to_string(),
        context: None,
    };
    let err = client
        .complete_turn(&request)
        .expect_err("idle timeout should fail");
    let NativeRuntimeError::ModelRequestFailed { code, .. } = err else {
        panic!("expected model request failure");
    };
    assert_eq!(code, "native_stream_idle_timeout");

    let telemetry = client.take_transport_telemetry();
    assert_eq!(telemetry.attempts.len(), 1);
    assert_eq!(telemetry.attempts[0].code, "native_stream_idle_timeout");

    server_handle.join().expect("server thread should join");
}
