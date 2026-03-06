use super::*;
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::thread;

const OPENROUTER_CHAT_COMPLETIONS_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const OPENROUTER_RETRY_MAX_ATTEMPTS_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_MAX_ATTEMPTS";
const OPENROUTER_RETRY_BASE_DELAY_MS_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_BASE_DELAY_MS";
const OPENROUTER_RETRY_ON_429_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_429";
const OPENROUTER_RETRY_ON_5XX_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_5XX";
const OPENROUTER_RETRY_ON_TRANSPORT_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_TRANSPORT";
const OPENROUTER_STREAM_IDLE_TIMEOUT_MS_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_STREAM_IDLE_TIMEOUT_MS";
const OPENROUTER_FALLBACK_ENDPOINT_ENV: &str = "OPENROUTER_API_FALLBACK_BASE_URL";

const OPENROUTER_RETRY_MAX_ATTEMPTS_DEFAULT: u32 = 3;
const OPENROUTER_RETRY_MAX_ATTEMPTS_MAX: u32 = 8;
const OPENROUTER_RETRY_BASE_DELAY_MS_DEFAULT: u64 = 250;
const OPENROUTER_RETRY_BASE_DELAY_MS_MAX: u64 = 10_000;
const OPENROUTER_RETRY_BACKOFF_MS_MAX: u64 = 30_000;
const NATIVE_DIRECTIVE_SYSTEM_PROMPT: &str = "You are the Hivemind native runtime controller.\nReturn exactly one directive per response with one of these formats:\n- THINK:<short reasoning>\n- ACT:tool:<tool_name>:<json_object>\n- DONE:<summary>\nRules:\n- Never return markdown, code fences, or prose outside the directive line.\n- For ACT, always use tool syntax accepted by Hivemind: tool:<name>:<json>.\n- Only use built-in tool names: read_file, list_files, write_file, run_command, exec_command, write_stdin, git_status, git_diff, graph_query.\n- Do not invent tool names.\n- Prefer ACT over DONE while required work remains.\n- Do not return DONE on the first turn unless the user explicitly requested a no-op.\n- When the user prompt defines explicit steps, execute them in order before DONE.\n- Return DONE only after the requested deliverable has been created or verified.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OpenRouterMessage {
    role: String,
    content: String,
}

impl OpenRouterMessage {
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

    fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

#[derive(Clone)]
pub struct OpenRouterModelClient {
    model: String,
    api_key: String,
    primary_endpoint: String,
    fallback_endpoint: Option<String>,
    active_transport: OpenRouterTransport,
    retry_policy: OpenRouterRetryPolicy,
    stream_idle_timeout_ms: u64,
    http: reqwest::blocking::Client,
    history: Vec<OpenRouterMessage>,
    transport_telemetry: NativeTransportTelemetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenRouterRetryPolicy {
    max_attempts: u32,
    base_delay_ms: u64,
    retry_on_429: bool,
    retry_on_5xx: bool,
    retry_on_transport: bool,
}

impl OpenRouterRetryPolicy {
    fn from_env(env: &HashMap<String, String>) -> Self {
        let max_attempts = parse_u32_env(
            env,
            OPENROUTER_RETRY_MAX_ATTEMPTS_ENV,
            OPENROUTER_RETRY_MAX_ATTEMPTS_DEFAULT,
            1,
            OPENROUTER_RETRY_MAX_ATTEMPTS_MAX,
        );
        let base_delay_ms = parse_u64_env(
            env,
            OPENROUTER_RETRY_BASE_DELAY_MS_ENV,
            OPENROUTER_RETRY_BASE_DELAY_MS_DEFAULT,
            1,
            OPENROUTER_RETRY_BASE_DELAY_MS_MAX,
        );
        Self {
            max_attempts,
            base_delay_ms,
            retry_on_429: parse_bool_env(env, OPENROUTER_RETRY_ON_429_ENV, true),
            retry_on_5xx: parse_bool_env(env, OPENROUTER_RETRY_ON_5XX_ENV, true),
            retry_on_transport: parse_bool_env(env, OPENROUTER_RETRY_ON_TRANSPORT_ENV, true),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenRouterTransport {
    HttpPrimary,
    HttpFallback,
}

impl OpenRouterTransport {
    const fn as_label(self) -> &'static str {
        match self {
            Self::HttpPrimary => "http_primary",
            Self::HttpFallback => "http_fallback",
        }
    }
}

#[derive(Debug, Clone)]
struct OpenRouterAttemptFailure {
    code: String,
    message: String,
    retryable: bool,
    rate_limited: bool,
    status_code: Option<u16>,
}

fn parse_bool_env(env: &HashMap<String, String>, key: &str, default: bool) -> bool {
    env.get(key).map_or(default, |raw| {
        match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        }
    })
}

fn parse_u32_env(
    env: &HashMap<String, String>,
    key: &str,
    default: u32,
    min: u32,
    max: u32,
) -> u32 {
    env.get(key)
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .map_or(default, |value| value.clamp(min, max))
}

fn parse_u64_env(
    env: &HashMap<String, String>,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    env.get(key)
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map_or(default, |value| value.clamp(min, max))
}

impl OpenRouterModelClient {
    fn normalize_model_id(model: impl Into<String>) -> String {
        let model = model.into();
        model
            .strip_prefix("openrouter/")
            .map_or_else(|| model.clone(), ToString::to_string)
    }

    pub fn from_env(
        model: impl Into<String>,
        env: &HashMap<String, String>,
    ) -> Result<Self, NativeRuntimeError> {
        let model = Self::normalize_model_id(model);
        let api_key = env
            .get("OPENROUTER_API_KEY")
            .cloned()
            .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| NativeRuntimeError::ModelRequestFailed {
                code: "native_model_request_failed".to_string(),
                message: "Missing OpenRouter API key. Set OPENROUTER_API_KEY in runtime env or shell environment.".to_string(),
                recoverable: false,
            })?;

        let primary_endpoint = env
            .get("OPENROUTER_API_BASE_URL")
            .cloned()
            .unwrap_or_else(|| OPENROUTER_CHAT_COMPLETIONS_URL.to_string());
        let fallback_endpoint = env
            .get(OPENROUTER_FALLBACK_ENDPOINT_ENV)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let timeout_ms = env
            .get("HIVEMIND_NATIVE_OPENROUTER_TIMEOUT_MS")
            .and_then(|raw| raw.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(60_000);
        let retry_policy = OpenRouterRetryPolicy::from_env(env);
        let stream_idle_timeout_ms = parse_u64_env(
            env,
            OPENROUTER_STREAM_IDLE_TIMEOUT_MS_ENV,
            timeout_ms.min(30_000),
            100,
            timeout_ms.max(100),
        );
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|error| NativeRuntimeError::ModelRequestFailed {
                code: "native_model_request_failed".to_string(),
                message: format!("Failed to initialize OpenRouter HTTP client: {error}"),
                recoverable: false,
            })?;

        Ok(Self {
            model,
            api_key,
            primary_endpoint,
            fallback_endpoint,
            active_transport: OpenRouterTransport::HttpPrimary,
            retry_policy,
            stream_idle_timeout_ms,
            http,
            history: vec![OpenRouterMessage::system(NATIVE_DIRECTIVE_SYSTEM_PROMPT)],
            transport_telemetry: NativeTransportTelemetry::default(),
        })
    }

    fn user_prompt_for_turn(request: &ModelTurnRequest) -> String {
        let mut prompt = format!(
            "Turn index: {}\nCurrent state: {}\nTask prompt:\n{}\n",
            request.turn_index,
            request.state.as_str(),
            request.prompt
        );
        if let Some(context) = request.context.as_ref() {
            prompt.push_str("\nAdditional context:\n");
            prompt.push_str(context);
            prompt.push('\n');
        }
        prompt.push_str("\nReturn one directive line now.");
        prompt
    }

    fn extract_text_content(body: &Value) -> Option<String> {
        let content = body
            .get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("content")?;

        if let Some(text) = content.as_str() {
            return Some(text.to_string());
        }

        let segments = content.as_array()?;
        let mut merged = String::new();
        for segment in segments {
            if let Some(text) = segment.get("text").and_then(Value::as_str) {
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

    fn api_error_message(body: &Value) -> Option<String> {
        body.get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }

    fn endpoint_for_transport(&self, transport: OpenRouterTransport) -> &str {
        match transport {
            OpenRouterTransport::HttpPrimary => self.primary_endpoint.as_str(),
            OpenRouterTransport::HttpFallback => self
                .fallback_endpoint
                .as_deref()
                .unwrap_or(self.primary_endpoint.as_str()),
        }
    }

    fn can_fallback(&self) -> bool {
        self.fallback_endpoint.is_some()
    }

    fn classify_http_failure(
        &self,
        status: reqwest::StatusCode,
        details: &str,
    ) -> OpenRouterAttemptFailure {
        if status.as_u16() == 429 {
            return OpenRouterAttemptFailure {
                code: "native_transport_http_429".to_string(),
                message: format!("OpenRouter request rejected ({status}): {details}"),
                retryable: self.retry_policy.retry_on_429,
                rate_limited: true,
                status_code: Some(status.as_u16()),
            };
        }
        if status.is_server_error() {
            return OpenRouterAttemptFailure {
                code: "native_transport_http_5xx".to_string(),
                message: format!("OpenRouter request rejected ({status}): {details}"),
                retryable: self.retry_policy.retry_on_5xx,
                rate_limited: false,
                status_code: Some(status.as_u16()),
            };
        }

        OpenRouterAttemptFailure {
            code: "native_model_request_rejected".to_string(),
            message: format!("OpenRouter request rejected ({status}): {details}"),
            retryable: false,
            rate_limited: false,
            status_code: Some(status.as_u16()),
        }
    }

    fn classify_transport_error(&self, error: &reqwest::Error) -> OpenRouterAttemptFailure {
        if error.is_timeout() {
            return OpenRouterAttemptFailure {
                code: "native_stream_idle_timeout".to_string(),
                message: format!(
                    "OpenRouter stream idle timeout exceeded ({}ms): {error}",
                    self.stream_idle_timeout_ms
                ),
                retryable: self.retry_policy.retry_on_transport,
                rate_limited: false,
                status_code: None,
            };
        }

        OpenRouterAttemptFailure {
            code: "native_transport_error".to_string(),
            message: format!("OpenRouter request failed: {error}"),
            retryable: self.retry_policy.retry_on_transport,
            rate_limited: false,
            status_code: None,
        }
    }

    fn classify_decode_error(&self, error: &serde_json::Error) -> OpenRouterAttemptFailure {
        OpenRouterAttemptFailure {
            code: "native_stream_terminal_failed".to_string(),
            message: format!("OpenRouter response decode failed: {error}"),
            retryable: self.retry_policy.retry_on_transport,
            rate_limited: false,
            status_code: None,
        }
    }

    fn classify_missing_content(&self) -> OpenRouterAttemptFailure {
        OpenRouterAttemptFailure {
            code: "native_stream_terminal_incomplete".to_string(),
            message: "OpenRouter response missing choices[0].message.content".to_string(),
            retryable: self.retry_policy.retry_on_transport,
            rate_limited: false,
            status_code: None,
        }
    }

    fn classify_stream_terminal_event(&self, body: &Value) -> Option<OpenRouterAttemptFailure> {
        let finish_reason = body
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(Value::as_str)
            .map(|value| value.trim().to_ascii_lowercase());

        match finish_reason.as_deref() {
            Some("length") => Some(OpenRouterAttemptFailure {
                code: "native_stream_terminal_incomplete".to_string(),
                message: "OpenRouter stream finished with `length`; output may be incomplete"
                    .to_string(),
                retryable: self.retry_policy.retry_on_transport,
                rate_limited: false,
                status_code: Some(200),
            }),
            Some("error" | "failed" | "cancelled") => Some(OpenRouterAttemptFailure {
                code: "native_stream_terminal_failed".to_string(),
                message: "OpenRouter stream reported terminal failure".to_string(),
                retryable: self.retry_policy.retry_on_transport,
                rate_limited: false,
                status_code: Some(200),
            }),
            _ => None,
        }
    }

    fn next_backoff_ms(
        &self,
        turn_index: u32,
        attempt: u32,
        code: &str,
        transport: OpenRouterTransport,
    ) -> u64 {
        let exponent = attempt.saturating_sub(1).min(16);
        let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
        let exp_delay = self.retry_policy.base_delay_ms.saturating_mul(multiplier);
        let capped = exp_delay.min(OPENROUTER_RETRY_BACKOFF_MS_MAX);

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.model.hash(&mut hasher);
        turn_index.hash(&mut hasher);
        attempt.hash(&mut hasher);
        transport.as_label().hash(&mut hasher);
        code.hash(&mut hasher);
        let jitter_cap = (capped / 3).max(1);
        let jitter = hasher.finish() % (jitter_cap.saturating_add(1));
        capped
            .saturating_add(jitter)
            .min(OPENROUTER_RETRY_BACKOFF_MS_MAX)
    }

    fn activate_fallback(&mut self, turn_index: u32, reason: &str) {
        if self.active_transport == OpenRouterTransport::HttpFallback || !self.can_fallback() {
            return;
        }
        let from_transport = self.active_transport;
        self.active_transport = OpenRouterTransport::HttpFallback;
        self.transport_telemetry
            .fallback_activations
            .push(NativeTransportFallbackTrace {
                turn_index,
                from_transport: from_transport.as_label().to_string(),
                to_transport: OpenRouterTransport::HttpFallback.as_label().to_string(),
                reason: reason.to_string(),
            });
    }

    fn request_once(
        &self,
        payload: &Value,
        transport: OpenRouterTransport,
    ) -> Result<String, OpenRouterAttemptFailure> {
        let endpoint = self.endpoint_for_transport(transport);
        let response = self
            .http
            .post(endpoint)
            .timeout(Duration::from_millis(self.stream_idle_timeout_ms))
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://hivemind.local")
            .header("X-Title", "hivemind-native-runtime")
            .json(payload)
            .send()
            .map_err(|error| self.classify_transport_error(&error))?;

        let status = response.status();
        let body_text = response
            .text()
            .map_err(|error| self.classify_transport_error(&error))?;
        let parsed_body = serde_json::from_str::<Value>(&body_text).ok();

        if !status.is_success() {
            let details = parsed_body
                .as_ref()
                .and_then(Self::api_error_message)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    if body_text.trim().is_empty() {
                        "unknown OpenRouter error".to_string()
                    } else {
                        body_text
                    }
                });
            return Err(self.classify_http_failure(status, &details));
        }

        let Some(body) = parsed_body else {
            let decode_error = serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "non-JSON success payload",
            ));
            return Err(self.classify_decode_error(&decode_error));
        };

        if let Some(failure) = self.classify_stream_terminal_event(&body) {
            return Err(failure);
        }

        Self::extract_text_content(&body).ok_or_else(|| self.classify_missing_content())
    }

    fn complete_turn_with_retry(
        &mut self,
        request: &ModelTurnRequest,
        payload: &Value,
    ) -> Result<String, NativeRuntimeError> {
        let max_attempts = self.retry_policy.max_attempts.max(1);
        for attempt in 1..=max_attempts {
            let active_transport = self.active_transport;
            let attempt_result = self.request_once(payload, active_transport);
            match attempt_result {
                Ok(response) => {
                    self.transport_telemetry.active_transport =
                        Some(self.active_transport.as_label().to_string());
                    return Ok(response);
                }
                Err(failure) => {
                    let last_attempt = attempt >= max_attempts;
                    let backoff_ms = if failure.retryable && !last_attempt {
                        Some(self.next_backoff_ms(
                            request.turn_index,
                            attempt,
                            &failure.code,
                            active_transport,
                        ))
                    } else {
                        None
                    };

                    self.transport_telemetry
                        .attempts
                        .push(NativeTransportAttemptTrace {
                            turn_index: request.turn_index,
                            attempt,
                            transport: active_transport.as_label().to_string(),
                            code: failure.code.clone(),
                            message: failure.message.clone(),
                            retryable: failure.retryable,
                            rate_limited: failure.rate_limited,
                            status_code: failure.status_code,
                            backoff_ms,
                        });

                    if failure.retryable && self.can_fallback() {
                        self.activate_fallback(request.turn_index, &failure.code);
                    }

                    self.transport_telemetry.active_transport =
                        Some(self.active_transport.as_label().to_string());

                    if let Some(delay) = backoff_ms {
                        thread::sleep(Duration::from_millis(delay));
                        continue;
                    }

                    return Err(NativeRuntimeError::ModelRequestFailed {
                        code: failure.code,
                        message: failure.message,
                        recoverable: failure.retryable,
                    });
                }
            }
        }

        Err(NativeRuntimeError::ModelRequestFailed {
            code: "native_model_request_failed".to_string(),
            message: "OpenRouter retry loop exhausted unexpectedly".to_string(),
            recoverable: false,
        })
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
}

impl ModelClient for OpenRouterModelClient {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        self.history
            .push(OpenRouterMessage::user(Self::user_prompt_for_turn(request)));

        let payload = serde_json::json!({
            "model": self.model,
            "temperature": 0.0,
            "messages": self.history,
        });
        let response_text = self.complete_turn_with_retry(request, &payload)?;
        let directive = Self::normalize_directive(&response_text);
        self.history
            .push(OpenRouterMessage::assistant(directive.clone()));
        Ok(directive)
    }

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        std::mem::take(&mut self.transport_telemetry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

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
        let Some((primary_endpoint, primary_handle)) =
            spawn_mock_http_server(vec![MockHttpResponse {
                status: 503,
                body: serde_json::json!({"error": { "message": "primary unavailable" }})
                    .to_string(),
                delay_ms: 0,
            }])
        else {
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
}
