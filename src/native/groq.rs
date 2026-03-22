use crate::adapters::runtime::{NativeTransportAttemptTrace, NativeTransportTelemetry};
use crate::native::tool_engine::{NativeToolEngine, ToolContract};
use crate::native::{ModelClient, ModelDirective, ModelTurnRequest, NativeRuntimeError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

const GROQ_CHAT_COMPLETIONS_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const NATIVE_DIRECTIVE_SYSTEM_PROMPT: &str = "You are the Hivemind native runtime controller. Return the next step by calling exactly one provided function. Use the real Hivemind tool functions for small ACT steps. Use emit_action for large or complex ACT steps, especially write_file payloads for existing files. Use emit_think only when you cannot yet choose the next real tool call. Use emit_done only after all explicit task requirements are satisfied, including required file edits and tests. Do not invent tool names. Hard rules: (1) if the task names concrete file paths, start with those exact files or their parent directories and do not begin with repo-wide or `src`-wide recursive listings, (2) do not use run_command for simple inspection like `ls`, `find`, or file reading; use list_files/read_file instead, (3) do not reread the same truncated file path more than once unless the file changed, (4) after roughly 3-5 targeted reads of named files, switch to an editing action or run_command for focused cargo tests instead of continuing exploration. Prefer read_file for known file paths and list_files only for narrow directories.";
const GROQ_ALLOWED_TOOL_NAMES: &[&str] = &[
    "read_file",
    "list_files",
    "run_command",
    "git_status",
    "git_diff",
    "checkpoint_complete",
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct GroqToolFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct GroqToolCall {
    #[allow(dead_code)]
    id: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    kind: Option<String>,
    function: GroqToolFunctionCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GroqMessage {
    role: String,
    content: String,
}

impl GroqMessage {
    fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
}

#[derive(Clone)]
pub struct GroqModelClient {
    model: String,
    api_key: String,
    endpoint: String,
    request_timeout_ms: u64,
    max_transport_attempts: u32,
    retry_backoff_ms: u64,
    http: reqwest::blocking::Client,
    history: Vec<GroqMessage>,
    transport_telemetry: NativeTransportTelemetry,
}

#[derive(Debug, Clone)]
struct GroqAttemptFailure {
    code: String,
    message: String,
    recoverable: bool,
    retryable: bool,
    rate_limited: bool,
    status_code: Option<u16>,
}

impl GroqModelClient {
    pub fn from_env(
        model: impl Into<String>,
        env: &HashMap<String, String>,
    ) -> Result<Self, NativeRuntimeError> {
        let model = Self::normalize_model_id(model);
        let api_key = env
            .get("GROQ_API_KEY")
            .cloned()
            .or_else(|| std::env::var("GROQ_API_KEY").ok())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| NativeRuntimeError::ModelRequestFailed {
                code: "native_model_request_failed".to_string(),
                message:
                    "Missing Groq API key. Set GROQ_API_KEY in runtime env or shell environment."
                        .to_string(),
                recoverable: false,
            })?;
        let endpoint = env
            .get("GROQ_API_BASE_URL")
            .cloned()
            .unwrap_or_else(|| GROQ_CHAT_COMPLETIONS_URL.to_string());
        let request_timeout_ms = env
            .get("HIVEMIND_NATIVE_GROQ_TIMEOUT_MS")
            .and_then(|raw| raw.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(180_000);
        let max_transport_attempts = env
            .get("HIVEMIND_NATIVE_GROQ_MAX_ATTEMPTS")
            .and_then(|raw| raw.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(2);
        let retry_backoff_ms = env
            .get("HIVEMIND_NATIVE_GROQ_RETRY_BACKOFF_MS")
            .and_then(|raw| raw.parse::<u64>().ok())
            .unwrap_or(1_000);
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(request_timeout_ms))
            .build()
            .map_err(|error| NativeRuntimeError::ModelRequestFailed {
                code: "native_model_request_failed".to_string(),
                message: format!("Failed to initialize Groq HTTP client: {error}"),
                recoverable: false,
            })?;

        Ok(Self {
            model,
            api_key,
            endpoint,
            request_timeout_ms,
            max_transport_attempts,
            retry_backoff_ms,
            http,
            history: vec![GroqMessage::system(NATIVE_DIRECTIVE_SYSTEM_PROMPT)],
            transport_telemetry: NativeTransportTelemetry::default(),
        })
    }

    fn normalize_model_id(model: impl Into<String>) -> String {
        let model = model.into();
        model
            .strip_prefix("groq/")
            .map_or_else(|| model.clone(), ToString::to_string)
    }

    fn user_prompt_for_turn(request: &ModelTurnRequest) -> String {
        let mut prompt = format!(
            "Turn index: {}\nCurrent state: {}\nAgent mode: {}\nTask prompt:\n{}\n",
            request.turn_index,
            request.state.as_str(),
            request.agent_mode.as_str(),
            request.prompt
        );
        if let Some(context) = request.context.as_ref() {
            prompt.push_str("\nAdditional context:\n");
            prompt.push_str(context);
            prompt.push('\n');
        }
        prompt.push_str(
            "\nCall exactly one provided function now. Prefer a real tool function over emit_think. If the task already names concrete files, use those exact files first and avoid broad recursive listings. Never use run_command for `ls`, `find`, or file reading. For file modifications or any large JSON payload action, use emit_action with the full Hivemind action string (for example tool:write_file:{...}) instead of trying to encode a large direct function call. After about 3-5 targeted reads/listings, your next step should usually be an editing action or run_command for focused cargo tests unless a missing path blocks you. Do not reread the same truncated file path more than once. After the first write, continue editing remaining required files or run tests. Do not checkpoint early.",
        );
        prompt
    }

    fn directive_tools(tool_contracts: &[ToolContract]) -> Value {
        let mut tools = tool_contracts
            .iter()
            .filter(|contract| GROQ_ALLOWED_TOOL_NAMES.contains(&contract.name.as_str()))
            .map(Self::contract_tool_definition)
            .collect::<Vec<_>>();
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "emit_action",
                "description": "Emit a full ACT directive string for large or complex actions, especially write_file payloads.",
                "parameters": {
                    "type": "object",
                    "properties": { "action": { "type": "string" } },
                    "required": ["action"],
                    "additionalProperties": false
                }
            }
        }));
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "emit_think",
                "description": "Emit a short THINK directive when you cannot yet choose the next real tool.",
                "parameters": {
                    "type": "object",
                    "properties": { "message": { "type": "string" } },
                    "required": ["message"],
                    "additionalProperties": false
                }
            }
        }));
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "emit_done",
                "description": "Emit a DONE directive when the task is complete.",
                "parameters": {
                    "type": "object",
                    "properties": { "summary": { "type": "string" } },
                    "required": ["summary"],
                    "additionalProperties": false
                }
            }
        }));
        Value::Array(tools)
    }

    fn contract_tool_definition(contract: &ToolContract) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": contract.name,
                "description": format!(
                    "Hivemind native tool `{}` (scope={}, timeout_ms={}).",
                    contract.name, contract.required_scope, contract.timeout_ms
                ),
                "parameters": contract.input_schema.clone(),
            }
        })
    }

    fn extract_text_content(body: &Value) -> Option<String> {
        body.get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("content")?
            .as_str()
            .map(ToString::to_string)
    }

    fn extract_tool_call(body: &Value) -> Option<GroqToolCall> {
        body.get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("tool_calls")?
            .as_array()?
            .first()
            .cloned()
            .and_then(|value| serde_json::from_value::<GroqToolCall>(value).ok())
    }

    fn extract_finish_reason(body: &Value) -> Option<String> {
        body.get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(Value::as_str)
            .map(|value| value.trim().to_ascii_lowercase())
    }

    fn api_error_message(body: &Value) -> Option<String> {
        let error = body.get("error")?;
        let message = error.get("message").and_then(Value::as_str)?;
        let failed_generation = error
            .get("failed_generation")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        Some(match failed_generation {
            Some(failed_generation) => format!("{message} | failed_generation={failed_generation}"),
            None => message.to_string(),
        })
    }

    fn terminal_finish_reason_error(
        &mut self,
        request: &ModelTurnRequest,
        finish_reason: Option<&str>,
    ) -> Result<(), NativeRuntimeError> {
        match finish_reason {
            Some("length") => Err(self.record_failure(
                request,
                "native_stream_terminal_incomplete",
                "Groq response finished with `length`; output may be incomplete",
                false,
                false,
                Some(200),
            )),
            Some("error" | "failed" | "cancelled") => Err(self.record_failure(
                request,
                "native_stream_terminal_failed",
                "Groq response reported terminal failure",
                false,
                false,
                Some(200),
            )),
            _ => Ok(()),
        }
    }

    fn parse_tool_directive(
        &mut self,
        request: &ModelTurnRequest,
        tool_call: GroqToolCall,
    ) -> Result<String, NativeRuntimeError> {
        let args =
            serde_json::from_str::<Value>(&tool_call.function.arguments).map_err(|error| {
                self.record_failure(
                    request,
                    "native_stream_terminal_failed",
                    format!("Groq tool-call arguments decode failed: {error}"),
                    false,
                    false,
                    None,
                )
            })?;

        match tool_call.function.name.as_str() {
            "emit_action" => {
                let action = args.get("action").and_then(Value::as_str).ok_or_else(|| {
                    self.record_failure(
                        request,
                        "native_stream_terminal_incomplete",
                        "Groq emit_action call missing `action`",
                        false,
                        false,
                        None,
                    )
                })?;
                Ok(format!("ACT:{}", action.trim()))
            }
            "emit_think" => {
                let message = args.get("message").and_then(Value::as_str).ok_or_else(|| {
                    self.record_failure(
                        request,
                        "native_stream_terminal_incomplete",
                        "Groq emit_think call missing `message`",
                        false,
                        false,
                        None,
                    )
                })?;
                Ok(format!("THINK:{}", message.trim()))
            }
            "emit_done" => {
                let summary = args.get("summary").and_then(Value::as_str).ok_or_else(|| {
                    self.record_failure(
                        request,
                        "native_stream_terminal_incomplete",
                        "Groq emit_done call missing `summary`",
                        false,
                        false,
                        None,
                    )
                })?;
                Ok(format!("DONE:{}", summary.trim()))
            }
            tool_name => Ok(format!("ACT:tool:{tool_name}:{}", args)),
        }
    }

    fn parse_content_directive(
        &mut self,
        request: &ModelTurnRequest,
        raw: &str,
    ) -> Result<String, NativeRuntimeError> {
        let directive = ModelDirective::parse_relaxed(raw).ok_or_else(|| {
            self.record_failure(
                request,
                "native_stream_terminal_failed",
                "Groq response did not contain a tool call or parseable directive",
                false,
                false,
                None,
            )
        })?;
        Ok(match directive {
            ModelDirective::Think { message } => format!("THINK:{message}"),
            ModelDirective::Act { action } => format!("ACT:{action}"),
            ModelDirective::Done { summary } => format!("DONE:{summary}"),
        })
    }

    fn record_attempt_failure(
        &mut self,
        request: &ModelTurnRequest,
        attempt: u32,
        failure: &GroqAttemptFailure,
    ) -> NativeRuntimeError {
        self.transport_telemetry
            .attempts
            .push(NativeTransportAttemptTrace {
                turn_index: request.turn_index,
                attempt,
                transport: "http_primary".to_string(),
                code: failure.code.clone(),
                message: failure.message.clone(),
                retryable: failure.retryable,
                rate_limited: failure.rate_limited,
                status_code: failure.status_code,
                backoff_ms: None,
            });
        NativeRuntimeError::ModelRequestFailed {
            code: failure.code.clone(),
            message: failure.message.clone(),
            recoverable: failure.recoverable,
        }
    }

    fn record_failure(
        &mut self,
        request: &ModelTurnRequest,
        code: &str,
        message: impl Into<String>,
        recoverable: bool,
        rate_limited: bool,
        status_code: Option<u16>,
    ) -> NativeRuntimeError {
        let failure = GroqAttemptFailure {
            code: code.to_string(),
            message: message.into(),
            recoverable,
            retryable: false,
            rate_limited,
            status_code,
        };
        self.record_attempt_failure(request, 1, &failure)
    }

    fn request_once(
        &self,
        payload: &Value,
    ) -> Result<reqwest::blocking::Response, GroqAttemptFailure> {
        let response = self
            .http
            .post(&self.endpoint)
            .timeout(Duration::from_millis(self.request_timeout_ms))
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .json(payload)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    GroqAttemptFailure {
                        code: "native_stream_idle_timeout".to_string(),
                        message: format!(
                            "Groq request timeout exceeded ({}ms): {error}",
                            self.request_timeout_ms
                        ),
                        recoverable: true,
                        retryable: true,
                        rate_limited: false,
                        status_code: None,
                    }
                } else {
                    GroqAttemptFailure {
                        code: "native_transport_error".to_string(),
                        message: format!("Groq request failed: {error}"),
                        recoverable: true,
                        retryable: true,
                        rate_limited: false,
                        status_code: None,
                    }
                }
            })?;

        let status = response.status();
        if status.is_success() {
            Ok(response)
        } else {
            let status_code = Some(status.as_u16());
            let recoverable = status.as_u16() == 429 || status.is_server_error();
            let retryable = recoverable;
            let rate_limited = status.as_u16() == 429;
            let code = if rate_limited {
                "native_transport_http_429"
            } else if status.is_server_error() {
                "native_transport_http_5xx"
            } else {
                "native_model_request_rejected"
            };
            let body_text = response.text().unwrap_or_default();
            let parsed_body = serde_json::from_str::<Value>(&body_text).ok();
            let details = parsed_body
                .as_ref()
                .and_then(Self::api_error_message)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    if body_text.trim().is_empty() {
                        "unknown Groq error".to_string()
                    } else {
                        body_text.clone()
                    }
                });
            Err(GroqAttemptFailure {
                code: code.to_string(),
                message: format!("Groq request rejected ({status}): {details}"),
                recoverable,
                retryable,
                rate_limited,
                status_code,
            })
        }
    }
}

impl ModelClient for GroqModelClient {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        let mut messages = self.history.clone();
        messages.push(GroqMessage::user(Self::user_prompt_for_turn(request)));
        let tool_contracts = NativeToolEngine::new()
            .map_err(|error| NativeRuntimeError::ModelRequestFailed {
                code: "native_model_request_failed".to_string(),
                message: format!("Failed to build native tool contracts for Groq: {error:?}"),
                recoverable: false,
            })?
            .contracts_for_mode(request.agent_mode);

        let payload = json!({
            "model": self.model,
            "temperature": 0.0,
            "messages": messages,
            "parallel_tool_calls": false,
            "tools": Self::directive_tools(&tool_contracts),
        });
        self.transport_telemetry.active_transport = Some("http_primary".to_string());

        for attempt in 1..=self.max_transport_attempts.max(1) {
            let response = match self.request_once(&payload) {
                Ok(response) => response,
                Err(failure) => {
                    let retryable = failure.retryable && attempt < self.max_transport_attempts;
                    let error = self.record_attempt_failure(request, attempt, &failure);
                    if retryable {
                        std::thread::sleep(Duration::from_millis(self.retry_backoff_ms));
                        continue;
                    }
                    return Err(error);
                }
            };

            let body_text = match response.text() {
                Ok(body_text) => body_text,
                Err(error) => {
                    let failure = GroqAttemptFailure {
                        code: "native_transport_error".to_string(),
                        message: format!("Groq response read failed: {error}"),
                        recoverable: true,
                        retryable: attempt < self.max_transport_attempts,
                        rate_limited: false,
                        status_code: None,
                    };
                    let error = self.record_attempt_failure(request, attempt, &failure);
                    if failure.retryable {
                        std::thread::sleep(Duration::from_millis(self.retry_backoff_ms));
                        continue;
                    }
                    return Err(error);
                }
            };
            let body = serde_json::from_str::<Value>(&body_text).map_err(|error| {
                self.record_failure(
                    request,
                    "native_stream_terminal_failed",
                    format!("Groq response decode failed: {error}"),
                    false,
                    false,
                    None,
                )
            })?;
            let finish_reason = Self::extract_finish_reason(&body);
            self.terminal_finish_reason_error(request, finish_reason.as_deref())?;
            if let Some(tool_call) = Self::extract_tool_call(&body) {
                return self.parse_tool_directive(request, tool_call);
            }
            let response_text = Self::extract_text_content(&body).ok_or_else(|| {
                self.record_failure(
                    request,
                    "native_stream_terminal_incomplete",
                    "Groq response missing both tool_calls and choices[0].message.content",
                    false,
                    false,
                    None,
                )
            })?;
            return self.parse_content_directive(request, &response_text);
        }

        Err(NativeRuntimeError::ModelRequestFailed {
            code: "native_transport_error".to_string(),
            message: "Groq exhausted transport retry attempts".to_string(),
            recoverable: true,
        })
    }

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        std::mem::take(&mut self.transport_telemetry)
    }
}

#[cfg(test)]
mod tests {
    use super::GroqModelClient;
    use crate::native::{
        AgentLoopState, AgentMode, ModelClient, ModelTurnRequest, NativeRuntimeError,
    };
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    #[derive(Clone)]
    struct MockHttpResponse {
        status: u16,
        body: String,
    }

    fn http_reason_phrase(status: u16) -> &'static str {
        match status {
            200 => "OK",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            _ => "Unknown",
        }
    }

    fn spawn_mock_http_server(
        responses: Vec<MockHttpResponse>,
    ) -> Option<(String, mpsc::Receiver<String>, std::thread::JoinHandle<()>)> {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("skipping network transport test: {error}");
                return None;
            }
        };
        let addr = listener.local_addr().expect("server addr");
        let (sender, receiver) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept connection");
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));

                let mut buf = [0_u8; 65536];
                let bytes_read = stream.read(&mut buf).unwrap_or(0);
                let raw_request = String::from_utf8_lossy(&buf[..bytes_read]).to_string();
                let _ = sender.send(raw_request);

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

        Some((format!("http://{addr}"), receiver, handle))
    }

    fn tool_call_body(name: &str, arguments: serde_json::Value, finish_reason: &str) -> String {
        serde_json::json!({
            "choices": [
                {
                    "message": {
                        "tool_calls": [
                            {
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": arguments.to_string()
                                }
                            }
                        ]
                    },
                    "finish_reason": finish_reason
                }
            ]
        })
        .to_string()
    }

    fn request() -> ModelTurnRequest {
        ModelTurnRequest {
            turn_index: 0,
            state: AgentLoopState::Think,
            agent_mode: AgentMode::TaskExecutor,
            prompt: "run".to_string(),
            context: None,
            prompt_assembly: None,
        }
    }

    #[test]
    fn groq_normalize_model_id_strips_provider_prefix() {
        let normalized = GroqModelClient::normalize_model_id("groq/openai/gpt-oss-120b");
        assert_eq!(normalized, "openai/gpt-oss-120b");
    }

    #[test]
    fn groq_complete_turn_uses_tool_call_payload() {
        let Some((endpoint, receiver, server_handle)) =
            spawn_mock_http_server(vec![MockHttpResponse {
                status: 200,
                body: tool_call_body(
                    "emit_done",
                    serde_json::json!({
                        "summary": "handled by groq"
                    }),
                    "stop",
                ),
            }])
        else {
            return;
        };

        let mut env = HashMap::new();
        env.insert("GROQ_API_KEY".to_string(), "test-key".to_string());
        env.insert("GROQ_API_BASE_URL".to_string(), endpoint);
        let mut client =
            GroqModelClient::from_env("groq/openai/gpt-oss-120b", &env).expect("client init");

        let directive = client
            .complete_turn(&request())
            .expect("request should succeed");
        assert_eq!(directive, "DONE:handled by groq");

        let raw_request = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("request");
        assert!(
            raw_request.contains("\"model\":\"openai/gpt-oss-120b\""),
            "{raw_request}"
        );
        assert!(raw_request.contains("\"tools\""), "{raw_request}");
        assert!(raw_request.contains("\"read_file\""), "{raw_request}");
        assert!(raw_request.contains("\"emit_done\""), "{raw_request}");

        server_handle.join().expect("server thread should join");
    }

    #[test]
    fn groq_maps_native_tool_call_into_act_directive() {
        let Some((endpoint, _, server_handle)) = spawn_mock_http_server(vec![MockHttpResponse {
            status: 200,
            body: tool_call_body(
                "read_file",
                serde_json::json!({"path": "src/native/prompt_assembly.rs"}),
                "tool_calls",
            ),
        }]) else {
            return;
        };

        let mut env = HashMap::new();
        env.insert("GROQ_API_KEY".to_string(), "test-key".to_string());
        env.insert("GROQ_API_BASE_URL".to_string(), endpoint);
        let mut client =
            GroqModelClient::from_env("groq/openai/gpt-oss-120b", &env).expect("client init");

        let directive = client
            .complete_turn(&request())
            .expect("request should succeed");
        assert_eq!(
            directive,
            "ACT:tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}"
        );

        server_handle.join().expect("server thread should join");
    }

    #[test]
    fn groq_classifies_http_rejection_and_records_transport_attempt() {
        let Some((endpoint, _, server_handle)) = spawn_mock_http_server(vec![MockHttpResponse {
            status: 429,
            body: serde_json::json!({"error": { "message": "rate limited" }}).to_string(),
        }]) else {
            return;
        };

        let mut env = HashMap::new();
        env.insert("GROQ_API_KEY".to_string(), "test-key".to_string());
        env.insert("GROQ_API_BASE_URL".to_string(), endpoint);
        env.insert(
            "HIVEMIND_NATIVE_GROQ_MAX_ATTEMPTS".to_string(),
            "1".to_string(),
        );
        let mut client =
            GroqModelClient::from_env("openai/gpt-oss-120b", &env).expect("client init");

        let err = client
            .complete_turn(&request())
            .expect_err("rate limit should fail");
        let NativeRuntimeError::ModelRequestFailed {
            code, recoverable, ..
        } = err
        else {
            panic!("expected model request failure");
        };
        assert_eq!(code, "native_transport_http_429");
        assert!(recoverable);

        let telemetry = client.take_transport_telemetry();
        assert_eq!(telemetry.attempts.len(), 1);
        assert_eq!(telemetry.attempts[0].code, "native_transport_http_429");
        assert!(telemetry.attempts[0].rate_limited);

        server_handle.join().expect("server thread should join");
    }
}
