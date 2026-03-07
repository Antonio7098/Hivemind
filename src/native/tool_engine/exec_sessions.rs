use super::*;

pub(super) const DEFAULT_EXEC_SESSION_CAPTURE_MS: u64 = 80;
pub(super) const DEFAULT_EXEC_SESSION_WRITE_WAIT_MS: u64 = 80;
const DEFAULT_EXEC_SESSION_STREAM_MAX_BYTES: usize = 16_384;
const DEFAULT_EXEC_SESSION_CAP: usize = 24;
const PROTECTED_RECENT_SESSION_COUNT: usize = 4;
pub(super) const EXEC_SESSION_CAP_ENV_KEY: &str = "HIVEMIND_NATIVE_EXEC_SESSION_CAP";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecCommandInput {
    pub(super) cmd: String,
    #[serde(default)]
    pub(super) args: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    initial_input: Option<String>,
    #[serde(default)]
    capture_ms: Option<u64>,
    #[serde(default)]
    max_bytes_per_stream: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct WriteStdinInput {
    session_id: u64,
    #[serde(default)]
    chars: Option<String>,
    #[serde(default)]
    wait_ms: Option<u64>,
    #[serde(default)]
    max_bytes_per_stream: Option<usize>,
    #[serde(default)]
    close_stdin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecSessionOutput {
    pub(super) session_id: u64,
    #[serde(default)]
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) stdout_truncated: bool,
    pub(super) stderr_truncated: bool,
    pub(super) stdout_truncated_bytes: usize,
    pub(super) stderr_truncated_bytes: usize,
    #[serde(default)]
    pub(super) warnings: Vec<String>,
}

mod commands;
mod support;

#[cfg(test)]
mod tests;

pub use support::cleanup_exec_sessions;

pub(super) fn handle_exec_command(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    commands::handle_exec_command(ctx, input, default_timeout_ms)
}

pub(super) fn handle_write_stdin(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    commands::handle_write_stdin(ctx, input, default_timeout_ms)
}

#[allow(dead_code)]
pub(super) fn clamp_exec_wait_ms(
    requested: Option<u64>,
    default_wait_ms: u64,
    timeout_ms: u64,
    started: Instant,
) -> u64 {
    support::clamp_exec_wait_ms(requested, default_wait_ms, timeout_ms, started)
}
