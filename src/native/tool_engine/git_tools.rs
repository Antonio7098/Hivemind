use super::*;

fn ensure_repository_read(scope: Option<&Scope>) -> Result<(), NativeToolEngineError> {
    let Some(scope) = scope else {
        return Ok(());
    };
    if scope.repositories.is_empty() {
        return Ok(());
    }
    let readable = scope.repositories.iter().any(|repo| {
        matches!(
            repo.mode,
            crate::core::scope::RepoAccessMode::ReadOnly
                | crate::core::scope::RepoAccessMode::ReadWrite
        )
    });
    if readable {
        Ok(())
    } else {
        Err(NativeToolEngineError::scope_violation(
            "repository read access is not permitted by scope",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct NoInput {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GitDiffInput {
    #[serde(default)]
    staged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GitStatusOutput {
    output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GitDiffOutput {
    output: String,
}

pub(super) fn handle_git_status(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let _ = decode_input::<NoInput>(input)?;
    ensure_repository_read(ctx.scope)?;
    let mut cmd = Command::new("git");
    cmd.args(["status", "--short", "--branch"])
        .current_dir(ctx.worktree)
        .env_clear();
    for (key, value) in deterministic_env_pairs(ctx.env) {
        cmd.env(key, value);
    }
    let output = cmd
        .output()
        .map_err(|error| NativeToolEngineError::execution(format!("git status failed: {error}")))?;
    if !output.status.success() {
        return Err(NativeToolEngineError::execution(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let result = GitStatusOutput {
        output: String::from_utf8_lossy(&output.stdout).to_string(),
    };
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode git_status output: {error}"))
    })
}

pub(super) fn handle_git_diff(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<GitDiffInput>(input)?;
    ensure_repository_read(ctx.scope)?;

    let mut args = vec!["diff"];
    if input.staged {
        args.push("--staged");
    }
    args.push("--no-ext-diff");

    let mut cmd = Command::new("git");
    cmd.args(&args).current_dir(ctx.worktree).env_clear();
    for (key, value) in deterministic_env_pairs(ctx.env) {
        cmd.env(key, value);
    }
    let output = cmd
        .output()
        .map_err(|error| NativeToolEngineError::execution(format!("git diff failed: {error}")))?;
    if !output.status.success() {
        return Err(NativeToolEngineError::execution(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let result = GitDiffOutput {
        output: String::from_utf8_lossy(&output.stdout).to_string(),
    };
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode git_diff output: {error}"))
    })
}
