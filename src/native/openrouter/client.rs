use super::protocol::{OpenRouterMessage, NATIVE_DIRECTIVE_SYSTEM_PROMPT};
use super::retry::{
    parse_u64_env, OpenRouterRetryPolicy, OpenRouterTransport,
    OPENROUTER_STREAM_IDLE_TIMEOUT_MS_ENV,
};
use super::transport::{OPENROUTER_CHAT_COMPLETIONS_URL, OPENROUTER_FALLBACK_ENDPOINT_ENV};
use super::OpenRouterModelClient;
use crate::adapters::runtime::NativeTransportTelemetry;
use crate::native::{ModelClient, ModelTurnRequest, NativeRuntimeError};
use std::collections::HashMap;
use std::time::Duration;

impl OpenRouterModelClient {
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
            .unwrap_or(180_000);
        let retry_policy = OpenRouterRetryPolicy::from_env(env);
        let stream_idle_timeout_ms = parse_u64_env(
            env,
            OPENROUTER_STREAM_IDLE_TIMEOUT_MS_ENV,
            timeout_ms.min(120_000),
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
}

impl ModelClient for OpenRouterModelClient {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        let mut messages = self.history.clone();
        messages.push(OpenRouterMessage::user(Self::user_prompt_for_turn(request)));

        let payload = serde_json::json!({
            "model": self.model,
            "temperature": 0.0,
            "messages": messages,
        });
        let response_text = self.complete_turn_with_retry(request, &payload)?;
        Ok(Self::normalize_directive(&response_text))
    }

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        std::mem::take(&mut self.transport_telemetry)
    }
}
