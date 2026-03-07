use super::*;

pub(crate) const OPENROUTER_RETRY_MAX_ATTEMPTS_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_RETRY_MAX_ATTEMPTS";
pub(crate) const OPENROUTER_RETRY_BASE_DELAY_MS_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_RETRY_BASE_DELAY_MS";
pub(crate) const OPENROUTER_RETRY_ON_429_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_429";
pub(crate) const OPENROUTER_RETRY_ON_5XX_ENV: &str = "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_5XX";
pub(crate) const OPENROUTER_RETRY_ON_TRANSPORT_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_RETRY_ON_TRANSPORT";
pub(crate) const OPENROUTER_STREAM_IDLE_TIMEOUT_MS_ENV: &str =
    "HIVEMIND_NATIVE_OPENROUTER_STREAM_IDLE_TIMEOUT_MS";
pub(crate) const OPENROUTER_RETRY_MAX_ATTEMPTS_DEFAULT: u32 = 3;
pub(crate) const OPENROUTER_RETRY_MAX_ATTEMPTS_MAX: u32 = 8;
pub(crate) const OPENROUTER_RETRY_BASE_DELAY_MS_DEFAULT: u64 = 250;
pub(crate) const OPENROUTER_RETRY_BASE_DELAY_MS_MAX: u64 = 10_000;
pub(crate) const OPENROUTER_RETRY_BACKOFF_MS_MAX: u64 = 30_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpenRouterRetryPolicy {
    pub(crate) max_attempts: u32,
    pub(crate) base_delay_ms: u64,
    pub(crate) retry_on_429: bool,
    pub(crate) retry_on_5xx: bool,
    pub(crate) retry_on_transport: bool,
}
impl OpenRouterRetryPolicy {
    pub(crate) fn from_env(env: &HashMap<String, String>) -> Self {
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
pub(crate) enum OpenRouterTransport {
    HttpPrimary,
    HttpFallback,
}
impl OpenRouterTransport {
    pub(crate) const fn as_label(self) -> &'static str {
        match self {
            Self::HttpPrimary => "http_primary",
            Self::HttpFallback => "http_fallback",
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct OpenRouterAttemptFailure {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) retryable: bool,
    pub(crate) rate_limited: bool,
    pub(crate) status_code: Option<u16>,
}
pub(crate) fn parse_bool_env(env: &HashMap<String, String>, key: &str, default: bool) -> bool {
    env.get(key).map_or(default, |raw| {
        match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        }
    })
}
pub(crate) fn parse_u32_env(
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
pub(crate) fn parse_u64_env(
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
