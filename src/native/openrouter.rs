use super::*;

mod client;
mod protocol;
mod retry;
mod transport;

#[cfg(test)]
mod tests;

use protocol::OpenRouterMessage;
use retry::{OpenRouterRetryPolicy, OpenRouterTransport};

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
