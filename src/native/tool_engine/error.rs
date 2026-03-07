use super::*;

impl NativeToolEngineError {
    pub(crate) fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        recoverable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable,
            policy_tags: Vec::new(),
        }
    }

    pub(crate) fn with_policy_tags(mut self, policy_tags: Vec<String>) -> Self {
        self.policy_tags = policy_tags;
        self
    }

    pub(crate) fn unknown_tool(tool_name: &str) -> Self {
        Self::new(
            "native_tool_unknown",
            format!("Unknown native tool '{tool_name}'"),
            false,
        )
    }

    pub(crate) fn validation(error: impl Into<String>) -> Self {
        Self::new("native_tool_input_invalid", error, false)
    }

    pub(crate) fn output_validation(tool_name: &str, error: impl Into<String>) -> Self {
        Self::new(
            "native_tool_output_invalid",
            format!(
                "Tool '{tool_name}' produced invalid output: {}",
                error.into()
            ),
            false,
        )
    }

    pub(crate) fn scope_violation(error: impl Into<String>) -> Self {
        Self::new("native_scope_violation", error, false)
    }

    pub(crate) fn policy_violation(error: impl Into<String>) -> Self {
        Self::new("native_policy_violation", error, false)
    }

    pub(crate) fn execution(error: impl Into<String>) -> Self {
        Self::new("native_tool_execution_failed", error, false)
    }

    pub(crate) fn timeout(tool_name: &str, timeout_ms: u64) -> Self {
        Self::new(
            "native_tool_timeout",
            format!("Tool '{tool_name}' exceeded timeout envelope ({timeout_ms}ms)"),
            false,
        )
    }
}
