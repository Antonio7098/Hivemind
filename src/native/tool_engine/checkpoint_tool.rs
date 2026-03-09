use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckpointCompleteInput {
    pub(super) id: String,
    #[serde(default)]
    pub(super) summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckpointCompleteOutput {
    pub(super) checkpoint_id: String,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

pub(super) fn handle_checkpoint_complete(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<CheckpointCompleteInput>(input)?;
    let checkpoint_id = input.id.trim();
    if checkpoint_id.is_empty() {
        return Err(NativeToolEngineError::validation(
            "checkpoint_complete requires a non-empty checkpoint id",
        ));
    }

    let hivemind_bin = ctx
        .env
        .get("HIVEMIND_BIN")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            NativeToolEngineError::execution(
                "checkpoint_complete requires HIVEMIND_BIN in the runtime environment",
            )
        })?;

    let mut cmd = Command::new(hivemind_bin);
    cmd.args(["checkpoint", "complete"])
        .current_dir(ctx.worktree)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(attempt_id) = ctx
        .env
        .get("HIVEMIND_ATTEMPT_ID")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        cmd.args(["--attempt-id", attempt_id]);
    }
    cmd.args(["--id", checkpoint_id]);
    for (key, value) in deterministic_env_pairs(ctx.env) {
        cmd.env(key, value);
    }
    if let Some(summary) = input.summary.as_deref().map(str::trim) {
        if !summary.is_empty() {
            cmd.args(["--summary", summary]);
        }
    }

    let output = cmd.output().map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to execute checkpoint completion via '{hivemind_bin}': {error}"
        ))
    })?;
    if output.status.success() {
        return serde_json::to_value(CheckpointCompleteOutput {
            checkpoint_id: checkpoint_id.to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
        .map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to encode checkpoint_complete output: {error}"
            ))
        });
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(NativeToolEngineError::execution(format!(
        "checkpoint completion failed for '{checkpoint_id}' with exit code {:?}: {}",
        output.status.code(),
        if stderr.is_empty() { stdout } else { stderr }
    )))
}
