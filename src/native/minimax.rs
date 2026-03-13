use crate::adapters::runtime::{NativeTransportAttemptTrace, NativeTransportTelemetry};
use crate::native::tool_engine::NativeToolAction;
use crate::native::{ModelClient, ModelDirective, ModelTurnRequest, NativeRuntimeError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::time::Duration;

const MINIMAX_CHAT_COMPLETIONS_URL: &str = "https://api.minimax.io/v1/chat/completions";
const NATIVE_DIRECTIVE_SYSTEM_PROMPT: &str = "You are the Hivemind native runtime controller.\nReturn exactly one directive per response with one of these formats:\n- THINK:<short reasoning>\n- ACT:tool:<tool_name>:<json_object>\n- DONE:<summary>\nRules:\n- Never return markdown, code fences, XML tags, role wrappers, or prose outside the directive line.\n- Do not emit <think> tags, </think> tags, [assistant:act], [assistant:think], or [tool_call:...] wrappers.\n- If you would otherwise reply with plain-English planning text, emit THINK instead.\n- THINK must be one short sentence under 160 characters. Do not summarize files or restate the task at length.\n- If you already know the next tool call, emit ACT immediately instead of THINK.\n- For ACT, always use tool syntax accepted by Hivemind: tool:<name>:<json>.\n- Only use built-in tool names: read_file, list_files, write_file, run_command, checkpoint_complete, exec_command, write_stdin, git_status, git_diff, graph_query.\n- Do not invent tool names.\n- Prefer read_file for known file paths and list_files only for directories.\n- Do not reread the same file unless it changed, the prior read was truncated, or you need a different section.\n- If a path failed with 'not a directory', retry with read_file instead of list_files.\n- After a few targeted reads, switch to write_file or tests instead of continued exploration.\n- After your first successful write_file, keep editing required files or run tests; do not restart broad exploration.\n- After your first successful write_file, your next one or two ACT steps should normally be write_file or run_command unless the previous edit directly failed.\n- Never call checkpoint_complete or DONE until all explicit task requirements are satisfied, including required file edits and required test commands.\n- A partial change to one file is not sufficient when the task requires multiple files or focused tests.\n- Prefer ACT over DONE while required work remains.\n- Do not return DONE on the first turn unless the user explicitly requested a no-op.\n- When the user prompt defines explicit steps, execute them in order before DONE.\n- Return DONE only after the requested deliverable has been created or verified.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MiniMaxMessage {
    role: String,
    content: String,
}

impl MiniMaxMessage {
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
pub struct MiniMaxModelClient {
    model: String,
    api_key: String,
    endpoint: String,
    request_timeout_ms: u64,
    max_transport_attempts: u32,
    retry_backoff_ms: u64,
    http: reqwest::blocking::Client,
    history: Vec<MiniMaxMessage>,
    transport_telemetry: NativeTransportTelemetry,
}

#[derive(Debug, Clone)]
struct MiniMaxAttemptFailure {
    code: String,
    message: String,
    recoverable: bool,
    retryable: bool,
    rate_limited: bool,
    status_code: Option<u16>,
}

impl MiniMaxModelClient {
    pub fn from_env(
        model: impl Into<String>,
        env: &HashMap<String, String>,
    ) -> Result<Self, NativeRuntimeError> {
        let model = Self::normalize_model_id(model);
        let api_key = env
            .get("MINIMAX_API_KEY")
            .cloned()
            .or_else(|| std::env::var("MINIMAX_API_KEY").ok())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| NativeRuntimeError::ModelRequestFailed {
                code: "native_model_request_failed".to_string(),
                message: "Missing MiniMax API key. Set MINIMAX_API_KEY in runtime env or shell environment.".to_string(),
                recoverable: false,
            })?;
        let endpoint = env
            .get("MINIMAX_API_BASE_URL")
            .cloned()
            .unwrap_or_else(|| MINIMAX_CHAT_COMPLETIONS_URL.to_string());
        let request_timeout_ms = env
            .get("HIVEMIND_NATIVE_MINIMAX_TIMEOUT_MS")
            .and_then(|raw| raw.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(600_000);
        let max_transport_attempts = env
            .get("HIVEMIND_NATIVE_MINIMAX_MAX_ATTEMPTS")
            .and_then(|raw| raw.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(2);
        let retry_backoff_ms = env
            .get("HIVEMIND_NATIVE_MINIMAX_RETRY_BACKOFF_MS")
            .and_then(|raw| raw.parse::<u64>().ok())
            .unwrap_or(1_000);
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(request_timeout_ms))
            .build()
            .map_err(|error| NativeRuntimeError::ModelRequestFailed {
                code: "native_model_request_failed".to_string(),
                message: format!("Failed to initialize MiniMax HTTP client: {error}"),
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
            history: vec![MiniMaxMessage::system(NATIVE_DIRECTIVE_SYSTEM_PROMPT)],
            transport_telemetry: NativeTransportTelemetry::default(),
        })
    }

    fn normalize_model_id(model: impl Into<String>) -> String {
        let model = model.into();
        model
            .strip_prefix("minimax/")
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
            "\nReturn one directive line now. Prefer ACT with the next tool call over THINK. If you emit THINK, keep it under 160 characters. Do not reread files that are already visible unless they changed or were truncated. After roughly 4 successful investigative reads/listings, your next ACT should edit or test unless a missing path blocks you. After your first write, continue editing remaining required files or run tests; your next one or two ACTs should usually be write_file or run_command. Do not checkpoint early. Never emit <think> tags, [assistant:act] wrappers, or multiple alternative directives.",
        );
        prompt
    }

    fn extract_text_content(body: &Value) -> Option<String> {
        let content = body
            .get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("content")?;

        Self::extract_text_value(content)
    }

    fn extract_text_value(content: &Value) -> Option<String> {
        if let Some(text) = content.as_str() {
            return Some(text.to_string());
        }

        let segments = content.as_array()?;
        let mut merged = String::new();
        for segment in segments {
            let text = segment
                .get("text")
                .or_else(|| segment.get("content"))
                .and_then(Value::as_str);
            if let Some(text) = text {
                if !merged.is_empty() {
                    merged.push('\n');
                }
                merged.push_str(text);
            }
        }
        if merged.trim().is_empty() {
            None
        } else {
            Some(merged)
        }
    }

    fn extract_stream_text_content(body: &Value) -> Option<String> {
        let choice = body.get("choices")?.as_array()?.first()?;
        choice
            .get("delta")
            .and_then(|delta| delta.get("content"))
            .and_then(Self::extract_text_value)
            .or_else(|| {
                choice
                    .get("message")
                    .and_then(|message| message.get("content"))
                    .and_then(Self::extract_text_value)
            })
    }

    fn extract_finish_reason(body: &Value) -> Option<String> {
        body.get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(Value::as_str)
            .map(|value| value.trim().to_ascii_lowercase())
    }

    fn render_directive(directive: &ModelDirective) -> String {
        match directive {
            ModelDirective::Think { message } => format!("THINK:{message}"),
            ModelDirective::Act { action } => format!("ACT:{action}"),
            ModelDirective::Done { summary } => format!("DONE:{summary}"),
        }
    }

    fn merge_stream_text(current: &mut String, incoming: &str) {
        if incoming.trim().is_empty() {
            return;
        }
        if current.is_empty() {
            current.push_str(incoming);
            return;
        }
        if incoming == current || current.ends_with(incoming) {
            return;
        }
        if incoming.starts_with(current.as_str()) {
            current.clear();
            current.push_str(incoming);
            return;
        }
        current.push_str(incoming);
    }

    fn early_stream_directive(raw: &str) -> Option<String> {
        let directive = ModelDirective::parse_relaxed(raw)?;
        match &directive {
            ModelDirective::Act { action } => {
                if action.contains('{') && NativeToolAction::parse(action).ok().flatten().is_some()
                {
                    Some(Self::render_directive(&directive))
                } else {
                    None
                }
            }
            _ => None,
        }
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
                "MiniMax stream finished with `length`; output may be incomplete",
                false,
                false,
                Some(200),
            )),
            Some("error" | "failed" | "cancelled") => Err(self.record_failure(
                request,
                "native_stream_terminal_failed",
                "MiniMax stream reported terminal failure",
                false,
                false,
                Some(200),
            )),
            _ => Ok(()),
        }
    }

    fn finalize_response_text(&self, raw: &str) -> String {
        ModelDirective::parse_relaxed(raw)
            .map(|directive| Self::render_directive(&directive))
            .unwrap_or_else(|| Self::normalize_directive(raw))
    }

    fn handle_json_response(
        &mut self,
        request: &ModelTurnRequest,
        body_text: String,
    ) -> Result<String, NativeRuntimeError> {
        let body = serde_json::from_str::<Value>(&body_text).map_err(|error| {
            self.record_failure(
                request,
                "native_stream_terminal_failed",
                format!("MiniMax response decode failed: {error}"),
                false,
                false,
                None,
            )
        })?;

        let finish_reason = Self::extract_finish_reason(&body);
        self.terminal_finish_reason_error(request, finish_reason.as_deref())?;

        let response_text = Self::extract_text_content(&body).ok_or_else(|| {
            self.record_failure(
                request,
                "native_stream_terminal_incomplete",
                "MiniMax response missing choices[0].message.content",
                false,
                false,
                None,
            )
        })?;
        Ok(self.finalize_response_text(&response_text))
    }

    fn handle_streaming_response(
        &mut self,
        request: &ModelTurnRequest,
        response: reqwest::blocking::Response,
    ) -> Result<String, NativeRuntimeError> {
        let mut reader = BufReader::new(response);
        let mut event_data_lines = Vec::new();
        let mut latest_content = String::new();
        let mut finish_reason: Option<String> = None;

        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line).map_err(|error| {
                self.record_failure(
                    request,
                    "native_transport_error",
                    format!("MiniMax stream read failed: {error}"),
                    false,
                    false,
                    None,
                )
            })?;
            if bytes_read == 0 {
                break;
            }

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if let Some(directive) = self.process_stream_event(
                    request,
                    &mut event_data_lines,
                    &mut latest_content,
                    &mut finish_reason,
                )? {
                    return Ok(directive);
                }
                continue;
            }

            if let Some(data) = trimmed.strip_prefix("data:") {
                event_data_lines.push(data.trim_start().to_string());
            }
        }

        if let Some(directive) = self.process_stream_event(
            request,
            &mut event_data_lines,
            &mut latest_content,
            &mut finish_reason,
        )? {
            return Ok(directive);
        }

        self.terminal_finish_reason_error(request, finish_reason.as_deref())?;

        if latest_content.trim().is_empty() {
            return Err(self.record_failure(
                request,
                "native_stream_terminal_incomplete",
                "MiniMax streaming response missing choices[0].delta.content",
                false,
                false,
                None,
            ));
        }

        Ok(self.finalize_response_text(&latest_content))
    }

    fn process_stream_event(
        &mut self,
        request: &ModelTurnRequest,
        event_lines: &mut Vec<String>,
        latest_content: &mut String,
        finish_reason: &mut Option<String>,
    ) -> Result<Option<String>, NativeRuntimeError> {
        if event_lines.is_empty() {
            return Ok(None);
        }

        let payload = event_lines.join("\n");
        event_lines.clear();
        if payload.trim() == "[DONE]" {
            return Ok(None);
        }

        let chunk = serde_json::from_str::<Value>(&payload).map_err(|error| {
            self.record_failure(
                request,
                "native_stream_terminal_failed",
                format!("MiniMax stream chunk decode failed: {error}"),
                false,
                false,
                None,
            )
        })?;

        if let Some(reason) = Self::extract_finish_reason(&chunk) {
            *finish_reason = Some(reason);
        }
        if let Some(content) = Self::extract_stream_text_content(&chunk) {
            Self::merge_stream_text(latest_content, &content);
            if let Some(directive) = Self::early_stream_directive(latest_content) {
                return Ok(Some(directive));
            }
        }

        Ok(None)
    }

    fn api_error_message(body: &Value) -> Option<String> {
        body.get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }

    fn normalize_directive(raw: &str) -> String {
        let trimmed = raw.trim().trim_matches('`').trim();
        for line in trimmed.lines() {
            let line = line.trim().trim_matches('`').trim();
            if line.starts_with("THINK:") || line.starts_with("ACT:") || line.starts_with("DONE:") {
                return line.to_string();
            }
        }
        trimmed.to_string()
    }

    fn record_attempt_failure(
        &mut self,
        request: &ModelTurnRequest,
        attempt: u32,
        failure: &MiniMaxAttemptFailure,
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
        let failure = MiniMaxAttemptFailure {
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
    ) -> Result<reqwest::blocking::Response, MiniMaxAttemptFailure> {
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
                    MiniMaxAttemptFailure {
                        code: "native_stream_idle_timeout".to_string(),
                        message: format!(
                            "MiniMax request timeout exceeded ({}ms): {error}",
                            self.request_timeout_ms
                        ),
                        recoverable: true,
                        retryable: true,
                        rate_limited: false,
                        status_code: None,
                    }
                } else {
                    MiniMaxAttemptFailure {
                        code: "native_transport_error".to_string(),
                        message: format!("MiniMax request failed: {error}"),
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
                        "unknown MiniMax error".to_string()
                    } else {
                        body_text.clone()
                    }
                });
            Err(MiniMaxAttemptFailure {
                code: code.to_string(),
                message: format!("MiniMax request rejected ({status}): {details}"),
                recoverable,
                retryable,
                rate_limited,
                status_code,
            })
        }
    }
}

impl ModelClient for MiniMaxModelClient {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        let mut messages = self.history.clone();
        messages.push(MiniMaxMessage::user(Self::user_prompt_for_turn(request)));

        let payload = serde_json::json!({
            "model": self.model,
            "temperature": 1.0,
            "stream": false,
            "reasoning_split": true,
            "messages": messages,
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

            let is_streaming = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.contains("text/event-stream"));
            if is_streaming {
                return self.handle_streaming_response(request, response);
            }

            let body_text = match response.text() {
                Ok(body_text) => body_text,
                Err(error) => {
                    let failure = MiniMaxAttemptFailure {
                        code: "native_transport_error".to_string(),
                        message: format!("MiniMax response read failed: {error}"),
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
            return self.handle_json_response(request, body_text);
        }

        Err(NativeRuntimeError::ModelRequestFailed {
            code: "native_transport_error".to_string(),
            message: "MiniMax exhausted transport retry attempts".to_string(),
            recoverable: true,
        })
    }

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        std::mem::take(&mut self.transport_telemetry)
    }
}

#[cfg(test)]
mod tests {
    use super::MiniMaxModelClient;
    use crate::native::{
        AgentLoopState, AgentMode, ModelClient, ModelTurnRequest, NativeRuntimeError,
    };
    use serde_json::Value;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[derive(Clone)]
    struct MockHttpResponse {
        status: u16,
        content_type: &'static str,
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

                let mut buf = [0_u8; 4096];
                let bytes_read = stream.read(&mut buf).unwrap_or(0);
                let raw_request = String::from_utf8_lossy(&buf[..bytes_read]).to_string();
                let _ = sender.send(raw_request);

                let reason = http_reason_phrase(response.status);
                let payload = response.body;
                let response_text = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.status,
                    reason,
                    response.content_type,
                    payload.len(),
                    payload
                );
                let _ = stream.write_all(response_text.as_bytes());
                let _ = stream.flush();
            }
        });

        Some((format!("http://{addr}"), receiver, handle))
    }

    fn success_body(content: &str, finish_reason: &str) -> String {
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

    fn stream_chunk_body(chunks: &[Value]) -> String {
        let mut body = String::new();
        for chunk in chunks {
            body.push_str("data: ");
            body.push_str(&chunk.to_string());
            body.push_str("\n\n");
        }
        body.push_str("data: [DONE]\n\n");
        body
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
    fn minimax_normalize_model_id_strips_provider_prefix() {
        let normalized = MiniMaxModelClient::normalize_model_id("minimax/MiniMax-M2.5");
        assert_eq!(normalized, "MiniMax-M2.5");
    }

    #[test]
    fn minimax_complete_turn_uses_openai_compatible_payload() {
        let Some((endpoint, receiver, server_handle)) =
            spawn_mock_http_server(vec![MockHttpResponse {
                status: 200,
                content_type: "application/json",
                body: success_body("DONE:handled by minimax", "stop"),
            }])
        else {
            return;
        };

        let mut env = HashMap::new();
        env.insert("MINIMAX_API_KEY".to_string(), "test-key".to_string());
        env.insert("MINIMAX_API_BASE_URL".to_string(), endpoint);
        let mut client = MiniMaxModelClient::from_env("minimax/MiniMax-M2.5", &env)
            .expect("client should initialize");

        let directive = client
            .complete_turn(&request())
            .expect("request should succeed");
        assert_eq!(directive, "DONE:handled by minimax");

        let raw_request = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("server should capture request");
        assert!(
            raw_request.contains("\"model\":\"MiniMax-M2.5\""),
            "{raw_request}"
        );
        assert!(raw_request.contains("\"temperature\":1.0"), "{raw_request}");
        assert!(raw_request.contains("\"stream\":false"), "{raw_request}");
        assert!(
            raw_request.contains("\"reasoning_split\":true"),
            "{raw_request}"
        );

        let telemetry = client.take_transport_telemetry();
        assert_eq!(telemetry.active_transport.as_deref(), Some("http_primary"));
        assert!(telemetry.attempts.is_empty());

        server_handle.join().expect("server thread should join");
    }

    #[test]
    fn minimax_classifies_http_rejection_and_records_transport_attempt() {
        let Some((endpoint, _, server_handle)) = spawn_mock_http_server(vec![MockHttpResponse {
            status: 429,
            content_type: "application/json",
            body: serde_json::json!({"error": { "message": "rate limited" }}).to_string(),
        }]) else {
            return;
        };

        let mut env = HashMap::new();
        env.insert("MINIMAX_API_KEY".to_string(), "test-key".to_string());
        env.insert("MINIMAX_API_BASE_URL".to_string(), endpoint);
        env.insert(
            "HIVEMIND_NATIVE_MINIMAX_MAX_ATTEMPTS".to_string(),
            "1".to_string(),
        );
        let mut client =
            MiniMaxModelClient::from_env("MiniMax-M2.5", &env).expect("client should initialize");

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
        assert_eq!(telemetry.active_transport.as_deref(), Some("http_primary"));
        assert_eq!(telemetry.attempts.len(), 1);
        assert_eq!(telemetry.attempts[0].status_code, Some(429));
        assert!(telemetry.attempts[0].rate_limited);
        assert!(telemetry.attempts[0].retryable);

        server_handle.join().expect("server thread should join");
    }

    #[test]
    fn minimax_retries_http_5xx_once_and_succeeds() {
        let Some((endpoint, _, server_handle)) = spawn_mock_http_server(vec![
            MockHttpResponse {
                status: 500,
                content_type: "application/json",
                body: serde_json::json!({"error": { "message": "temporary failure" }}).to_string(),
            },
            MockHttpResponse {
                status: 200,
                content_type: "application/json",
                body: success_body("DONE:handled after retry", "stop"),
            },
        ]) else {
            return;
        };

        let mut env = HashMap::new();
        env.insert("MINIMAX_API_KEY".to_string(), "test-key".to_string());
        env.insert("MINIMAX_API_BASE_URL".to_string(), endpoint);
        let mut client =
            MiniMaxModelClient::from_env("MiniMax-M2.5", &env).expect("client should initialize");

        let directive = client
            .complete_turn(&request())
            .expect("client should retry transient 5xx once");
        assert_eq!(directive, "DONE:handled after retry");

        let telemetry = client.take_transport_telemetry();
        assert_eq!(telemetry.active_transport.as_deref(), Some("http_primary"));
        assert_eq!(telemetry.attempts.len(), 1);
        assert_eq!(telemetry.attempts[0].code, "native_transport_http_5xx");
        assert!(telemetry.attempts[0].retryable);

        server_handle.join().expect("server thread should join");
    }

    #[test]
    fn minimax_streaming_response_merges_cumulative_content() {
        let chunks = vec![
            serde_json::json!({
                "choices": [{
                    "delta": { "content": "ACT:tool:list_files:{\"path\":\"src\"" },
                    "finish_reason": null
                }]
            }),
            serde_json::json!({
                "choices": [{
                    "delta": { "content": "ACT:tool:list_files:{\"path\":\"src\",\"recursive\":true}" },
                    "finish_reason": "stop"
                }]
            }),
        ];
        let Some((endpoint, _, server_handle)) = spawn_mock_http_server(vec![MockHttpResponse {
            status: 200,
            content_type: "text/event-stream",
            body: stream_chunk_body(&chunks),
        }]) else {
            return;
        };

        let mut env = HashMap::new();
        env.insert("MINIMAX_API_KEY".to_string(), "test-key".to_string());
        env.insert("MINIMAX_API_BASE_URL".to_string(), endpoint);
        let mut client =
            MiniMaxModelClient::from_env("MiniMax-M2.5", &env).expect("client should initialize");

        let directive = client
            .complete_turn(&request())
            .expect("streaming request should succeed");
        assert_eq!(
            directive,
            "ACT:tool:list_files:{\"path\":\"src\",\"recursive\":true}"
        );

        server_handle.join().expect("server thread should join");
    }

    #[test]
    fn minimax_streaming_returns_early_after_complete_tool_directive() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));

            let mut buf = [0_u8; 4096];
            let _ = stream.read(&mut buf);

            let headers = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: text/event-stream\r\n",
                "Connection: close\r\n\r\n"
            );
            let first_chunk = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":",
                "\"ACT:tool:list_files:{\\\"path\\\":\\\"src\\\",\\\"recursive\\\":true}\"},",
                "\"finish_reason\":null}]}\n\n"
            );
            let _ = stream.write_all(headers.as_bytes());
            let _ = stream.write_all(first_chunk.as_bytes());
            let _ = stream.flush();

            std::thread::sleep(Duration::from_secs(3));
            let _ = stream.write_all(b"data: [DONE]\n\n");
            let _ = stream.flush();
        });

        let mut env = HashMap::new();
        env.insert("MINIMAX_API_KEY".to_string(), "test-key".to_string());
        env.insert("MINIMAX_API_BASE_URL".to_string(), format!("http://{addr}"));
        let mut client =
            MiniMaxModelClient::from_env("MiniMax-M2.5", &env).expect("client should initialize");

        let started = Instant::now();
        let directive = client
            .complete_turn(&request())
            .expect("stream should return the first complete tool directive");
        let elapsed = started.elapsed();

        assert_eq!(
            directive,
            "ACT:tool:list_files:{\"path\":\"src\",\"recursive\":true}"
        );
        assert!(elapsed < Duration::from_secs(2), "elapsed={elapsed:?}");

        handle.join().expect("server thread should join");
    }

    #[test]
    fn minimax_streaming_does_not_return_early_for_partial_tool_prefix() {
        assert_eq!(
            MiniMaxModelClient::early_stream_directive("ACT:tool:list"),
            None
        );
        assert_eq!(
            MiniMaxModelClient::early_stream_directive(
                "ACT:tool:list_files:{\"path\":\"src\",\"recursive\":true}"
            ),
            Some("ACT:tool:list_files:{\"path\":\"src\",\"recursive\":true}".to_string())
        );
    }

    #[test]
    fn minimax_streaming_returns_early_for_minimax_tool_wrapper_after_think() {
        assert_eq!(
            MiniMaxModelClient::early_stream_directive(
                "THINK:I should inspect the native events directory first.\n<minimax:tool_call>\n<invoke name=\"list_files\">\n<parameter name=\"path\">src/core/registry/runtime/native_events</parameter>\n<parameter name=\"recursive\">false</parameter>\n</invoke>\n</minimax:tool_call>"
            ),
            Some(
                "ACT:tool:list_files:{\"path\":\"src/core/registry/runtime/native_events\",\"recursive\":false}"
                    .to_string()
            )
        );
    }

    #[test]
    fn minimax_streaming_returns_early_for_wrapped_native_tool_line() {
        assert_eq!(
            MiniMaxModelClient::early_stream_directive(
                "THINK:I should inspect the remaining runtime files.\n<minimax:tool_call>\ntool:read_file:{\"path\":\"src/native/turn_items.rs\"}\n[/tool_call]"
            ),
            Some("ACT:tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string())
        );
    }

    #[test]
    fn minimax_defaults_to_longer_request_timeout() {
        let mut env = HashMap::new();
        env.insert("MINIMAX_API_KEY".to_string(), "test-key".to_string());
        env.insert(
            "MINIMAX_API_BASE_URL".to_string(),
            "https://example.invalid/v1/chat/completions".to_string(),
        );

        let client =
            MiniMaxModelClient::from_env("MiniMax-M2.5", &env).expect("client should initialize");

        assert_eq!(client.request_timeout_ms, 600_000);
        assert_eq!(client.max_transport_attempts, 2);
    }
}
