use super::*;

impl NativeRuntimeError {
    fn preview_raw_output(raw_output: &str) -> String {
        let sanitized = raw_output.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut preview = sanitized.chars().take(160).collect::<String>();
        if sanitized.chars().count() > 160 {
            preview.push_str(" …");
        }
        preview
    }

    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidTransition { .. } => "native_invalid_transition",
            Self::TurnBudgetExceeded { .. } => "native_turn_budget_exceeded",
            Self::TimeoutBudgetExceeded { .. } => "native_timeout_budget_exceeded",
            Self::TokenBudgetExceeded { .. } => "native_token_budget_exceeded",
            Self::ModelRequestFailed { code, .. } => code.as_str(),
            Self::MalformedModelOutput { .. } => "native_malformed_model_output",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::InvalidTransition { from, to } => {
                format!(
                    "Invalid native loop transition: {} -> {}",
                    from.as_str(),
                    to.as_str()
                )
            }
            Self::TurnBudgetExceeded { max_turns } => {
                format!("Native loop exceeded turn budget ({max_turns})")
            }
            Self::TimeoutBudgetExceeded { budget_ms } => {
                format!("Native loop exceeded timeout budget ({budget_ms}ms)")
            }
            Self::TokenBudgetExceeded { budget, used } => {
                format!("Native loop exceeded token budget ({used}/{budget})")
            }
            Self::ModelRequestFailed { message, .. } => {
                format!("Native model request failed: {message}")
            }
            Self::MalformedModelOutput {
                expected,
                raw_output,
                ..
            } => {
                let preview = Self::preview_raw_output(raw_output);
                format!(
                    "Malformed native model output. Expected {expected}. Raw preview: {preview}"
                )
            }
        }
    }

    #[must_use]
    pub const fn recoverable(&self) -> bool {
        match self {
            Self::InvalidTransition { .. }
            | Self::TurnBudgetExceeded { .. }
            | Self::TimeoutBudgetExceeded { .. }
            | Self::TokenBudgetExceeded { .. }
            | Self::MalformedModelOutput { .. } => false,
            Self::ModelRequestFailed { recoverable, .. } => *recoverable,
        }
    }

    #[must_use]
    pub fn recovery_hint(&self) -> Option<String> {
        match self {
            Self::MalformedModelOutput { recovery_hint, .. } => Some(recovery_hint.clone()),
            Self::TurnBudgetExceeded { .. } => {
                Some("Increase native turn budget or simplify prompt objectives".to_string())
            }
            Self::TimeoutBudgetExceeded { .. } => {
                Some("Increase timeout budget or investigate model/runtime latency".to_string())
            }
            Self::TokenBudgetExceeded { .. } => Some(
                "Reduce context size or raise native token budget in configuration".to_string(),
            ),
            Self::ModelRequestFailed { code, .. } => {
                if code == "native_stream_idle_timeout" {
                    Some(
                        "Stream idle timeout exceeded. Increase idle timeout or verify provider network health."
                            .to_string(),
                    )
                } else if code == "native_stream_terminal_incomplete"
                    || code == "native_stream_terminal_failed"
                {
                    Some(
                        "Model stream terminated incompletely. Retry, then inspect provider logs or fallback transport configuration."
                            .to_string(),
                    )
                } else {
                    Some("Check provider connectivity and credentials, then retry".to_string())
                }
            }
            Self::InvalidTransition { .. } => Some(
                "Inspect model directives and FSM transition rules for native runtime".to_string(),
            ),
        }
    }

    #[must_use]
    pub fn to_hivemind_error(&self, origin: &'static str) -> HivemindError {
        let mut err =
            HivemindError::new(ErrorCategory::Runtime, self.code(), self.message(), origin)
                .recoverable(self.recoverable());
        if let Some(hint) = self.recovery_hint() {
            err = err.with_hint(hint);
        }
        err
    }

    #[must_use]
    pub fn to_runtime_error(&self) -> RuntimeError {
        RuntimeError::new(self.code(), self.message(), self.recoverable())
    }
}
