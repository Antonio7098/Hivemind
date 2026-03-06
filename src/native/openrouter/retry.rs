use super::OpenRouterModelClient;
use crate::adapters::runtime::{NativeTransportAttemptTrace, NativeTransportFallbackTrace};
use crate::native::NativeRuntimeError;
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::thread;
use std::time::Duration;

pub(super) const OPENROUTER_RETRY_MAX_ATTEMPTS_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_RETRY_MAX_ATTEMPTS";
pub(super) const OPENROUTER_RETRY_BASE_DELAY_MS_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_RETRY_BASE_DELAY_MS";
pub(super) const OPENROUTER_RETRY_ON_429_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_429";
pub(super) const OPENROUTER_RETRY_ON_5XX_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_5XX";
pub(super) const OPENROUTER_RETRY_ON_TRANSPORT_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_TRANSPORT";
pub(super) const OPENROUTER_STREAM_IDLE_TIMEOUT_MS_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_STREAM_IDLE_TIMEOUT_MS";

const OPENROUTER_RETRY_MAX_ATTEMPTS_DEFAULT: u32 = 3;
const OPENROUTER_RETRY_MAX_ATTEMPTS_MAX: u32 = 8;
const OPENROUTER_RETRY_BASE_DELAY_MS_DEFAULT: u64 = 250;
const OPENROUTER_RETRY_BASE_DELAY_MS_MAX: u64 = 10_000;
const OPENROUTER_RETRY_BACKOFF_MS_MAX: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OpenRouterRetryPolicy {
    pub(super) max_attempts: u32,
    pub(super) base_delay_ms: u64,
    pub(super) retry_on_429: bool,
    pub(super) retry_on_5xx: bool,
    pub(super) retry_on_transport: bool,
}

impl OpenRouterRetryPolicy {
    pub(super) fn from_env(env: &HashMap<String, String>) -> Self {
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
pub(super) enum OpenRouterTransport {
    HttpPrimary,
    HttpFallback,
}

impl OpenRouterTransport {
    pub(super) const fn as_label(self) -> &'static str {
        match self {
            Self::HttpPrimary => "http_primary",
            Self::HttpFallback => "http_fallback",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct OpenRouterAttemptFailure {
    pub(super) code: String,
    pub(super) message: String,
    pub(super) retryable: bool,
    pub(super) rate_limited: bool,
    pub(super) status_code: Option<u16>,
}

pub(super) fn parse_bool_env(env: &HashMap<String, String>, key: &str, default: bool) -> bool {
    env.get(key).map_or(default, |raw| {
        match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        }
    })
}

pub(super) fn parse_u32_env(
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

pub(super) fn parse_u64_env(
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
    pub(super) fn can_fallback(&self) -> bool {
        self.fallback_endpoint.is_some()
    }

    pub(super) fn classify_http_failure(
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

    pub(super) fn classify_transport_error(
        &self,
        error: &reqwest::Error,
    ) -> OpenRouterAttemptFailure {
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

    pub(super) fn classify_decode_error(
        &self,
        error: &serde_json::Error,
    ) -> OpenRouterAttemptFailure {
        OpenRouterAttemptFailure {
            code: "native_stream_terminal_failed".to_string(),
            message: format!("OpenRouter response decode failed: {error}"),
            retryable: self.retry_policy.retry_on_transport,
            rate_limited: false,
            status_code: None,
        }
    }

    pub(super) fn classify_missing_content(&self) -> OpenRouterAttemptFailure {
        OpenRouterAttemptFailure {
            code: "native_stream_terminal_incomplete".to_string(),
            message: "OpenRouter response missing choices[0].message.content".to_string(),
            retryable: self.retry_policy.retry_on_transport,
            rate_limited: false,
            status_code: None,
        }
    }

    pub(super) fn classify_stream_terminal_event(
        &self,
        body: &Value,
    ) -> Option<OpenRouterAttemptFailure> {
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

    pub(super) fn next_backoff_ms(
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

    pub(super) fn activate_fallback(&mut self, turn_index: u32, reason: &str) {
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

    pub(super) fn complete_turn_with_retry(
        &mut self,
        request: &crate::native::ModelTurnRequest,
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
}
