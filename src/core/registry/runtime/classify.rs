use super::*;

impl Registry {
    pub(crate) fn classify_runtime_error(
        code: &str,
        message: &str,
        recoverable: bool,
        stdout: &str,
        stderr: &str,
    ) -> ClassifiedRuntimeError {
        let combined = format!("{code} {message} {stdout} {stderr}").to_ascii_lowercase();
        let rate_limited = combined.contains("rate limit")
            || combined.contains("rate_limit")
            || combined.contains("too many requests")
            || combined.contains(" 429")
            || combined.contains("status 429")
            || combined.contains("http 429");

        let category = if rate_limited {
            "rate_limit"
        } else if code.starts_with("native_stream_") {
            "transport_stream"
        } else if code.starts_with("native_transport_") {
            "transport"
        } else if code == "timeout" {
            "timeout"
        } else if code == "checkpoints_incomplete" {
            "checkpoint_incomplete"
        } else if matches!(
            code,
            "binary_not_found" | "health_check_failed" | "missing_args" | "worktree_not_found"
        ) {
            "configuration"
        } else if code.contains("initialize") || code.contains("prepare") {
            "runtime_setup"
        } else {
            "runtime_execution"
        };

        let effective_recoverable = recoverable || rate_limited || code == "checkpoints_incomplete";
        let retryable = effective_recoverable
            && !matches!(
                code,
                "binary_not_found"
                    | "health_check_failed"
                    | "missing_args"
                    | "worktree_not_found"
                    | "checkpoints_incomplete"
            );

        ClassifiedRuntimeError {
            code: code.to_string(),
            category: category.to_string(),
            message: message.to_string(),
            recoverable: effective_recoverable,
            retryable,
            rate_limited,
        }
    }

    pub(crate) fn detect_runtime_output_failure(
        stdout: &str,
        stderr: &str,
    ) -> Option<(String, String, bool)> {
        let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();

        if combined.contains("incorrect api key")
            || combined.contains("invalid api key")
            || combined.contains("authentication failed")
            || combined.contains("unauthorized")
            || combined.contains("forbidden")
        {
            return Some((
                "runtime_auth_failed".to_string(),
                "Runtime authentication failed; inspect runtime stderr for details".to_string(),
                false,
            ));
        }

        if combined.contains("rate limit")
            || combined.contains("too many requests")
            || combined.contains("http 429")
            || combined.contains("status 429")
            || combined.contains(" 429")
        {
            return Some((
                "runtime_rate_limited".to_string(),
                "Runtime reported rate limiting; retry recommended".to_string(),
                true,
            ));
        }

        None
    }

    pub(crate) fn emit_runtime_error_classified(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        adapter_name: &str,
        classified: &ClassifiedRuntimeError,
        origin: &'static str,
    ) -> Result<()> {
        let state = self.state()?;
        let corr_attempt =
            Self::correlation_for_flow_task_attempt_event(&state, flow, task_id, attempt_id);

        self.append_event(
            Event::new(
                EventPayload::RuntimeErrorClassified {
                    attempt_id,
                    adapter_name: adapter_name.to_string(),
                    code: classified.code.clone(),
                    category: classified.category.clone(),
                    message: classified.message.clone(),
                    recoverable: classified.recoverable,
                    retryable: classified.retryable,
                    rate_limited: classified.rate_limited,
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        let mut err = HivemindError::runtime(
            format!("runtime_{}", classified.code),
            classified.message.clone(),
            origin,
        )
        .recoverable(classified.recoverable)
        .with_context("adapter", adapter_name.to_string())
        .with_context("runtime_error_code", classified.code.clone())
        .with_context("runtime_error_category", classified.category.clone());

        if classified.rate_limited {
            err = err.with_hint(
                "Rate limiting detected. Hivemind will schedule a retry and may switch to a backup runtime if configured.",
            );
        }

        self.record_error_event(&err, corr_attempt);
        Ok(())
    }
}
