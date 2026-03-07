use super::retry::{OpenRouterAttemptFailure, OpenRouterTransport};
use super::OpenRouterModelClient;
use std::time::Duration;

pub(super) const OPENROUTER_CHAT_COMPLETIONS_URL: &str =
    "https://openrouter.ai/api/v1/chat/completions";
pub(super) const OPENROUTER_FALLBACK_ENDPOINT_ENV: &str = "OPENROUTER_API_FALLBACK_BASE_URL";

impl OpenRouterModelClient {
    pub(super) fn endpoint_for_transport(&self, transport: OpenRouterTransport) -> &str {
        match transport {
            OpenRouterTransport::HttpPrimary => self.primary_endpoint.as_str(),
            OpenRouterTransport::HttpFallback => self
                .fallback_endpoint
                .as_deref()
                .unwrap_or(self.primary_endpoint.as_str()),
        }
    }

    pub(super) fn request_once(
        &self,
        payload: &serde_json::Value,
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
        let parsed_body = serde_json::from_str::<serde_json::Value>(&body_text).ok();

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
}
