//! Structured error types.
//!
//! Errors must be classifiable, attributable, and actionable.
//! Every error answers: What failed? Why? What can be done next?

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Error category for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// System-level errors (IO, network, etc.)
    System,
    /// Runtime adapter errors
    Runtime,
    /// Agent execution errors
    Agent,
    /// Scope violation errors
    Scope,
    /// Verification check failures
    Verification,
    /// Git operation errors
    Git,
    /// User input errors
    User,
    /// Policy violation errors
    Policy,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::Runtime => write!(f, "runtime"),
            Self::Agent => write!(f, "agent"),
            Self::Scope => write!(f, "scope"),
            Self::Verification => write!(f, "verification"),
            Self::Git => write!(f, "git"),
            Self::User => write!(f, "user"),
            Self::Policy => write!(f, "policy"),
        }
    }
}

/// Structured error with full context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HivemindError {
    /// Error category for classification.
    pub category: ErrorCategory,
    /// Unique error code within category.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Component and identifier that originated the error.
    pub origin: String,
    /// Whether this error is potentially recoverable.
    pub recoverable: bool,
    /// Hint for recovery action.
    pub recovery_hint: Option<String>,
    /// Additional context key-value pairs.
    pub context: HashMap<String, String>,
}

impl HivemindError {
    /// Creates a new error with the given parameters.
    #[must_use]
    pub fn new(
        category: ErrorCategory,
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self {
            category,
            code: code.into(),
            message: message.into(),
            origin: origin.into(),
            recoverable: false,
            recovery_hint: None,
            context: HashMap::new(),
        }
    }

    /// Sets whether the error is recoverable.
    #[must_use]
    pub fn recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = recoverable;
        self
    }

    /// Sets the recovery hint.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.recovery_hint = Some(hint.into());
        self
    }

    /// Adds context to the error.
    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Creates a system error.
    #[must_use]
    pub fn system(
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self::new(ErrorCategory::System, code, message, origin)
    }

    /// Creates a user input error.
    #[must_use]
    pub fn user(
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self::new(ErrorCategory::User, code, message, origin).recoverable(true)
    }

    /// Creates a git error.
    #[must_use]
    pub fn git(
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self::new(ErrorCategory::Git, code, message, origin)
    }

    /// Creates a runtime adapter error.
    #[must_use]
    pub fn runtime(
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self::new(ErrorCategory::Runtime, code, message, origin).recoverable(true)
    }

    /// Creates an agent execution error.
    #[must_use]
    pub fn agent(
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self::new(ErrorCategory::Agent, code, message, origin).recoverable(true)
    }

    /// Creates a scope violation error.
    #[must_use]
    pub fn scope(
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self::new(ErrorCategory::Scope, code, message, origin)
    }

    /// Creates a verification error.
    #[must_use]
    pub fn verification(
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self::new(ErrorCategory::Verification, code, message, origin).recoverable(true)
    }

    /// Creates a policy violation error.
    #[must_use]
    pub fn policy(
        code: impl Into<String>,
        message: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self::new(ErrorCategory::Policy, code, message, origin)
    }
}

impl std::fmt::Display for HivemindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}] {}", self.category, self.code, self.message)
    }
}

impl std::error::Error for HivemindError {}

/// Result type using `HivemindError`.
pub type Result<T> = std::result::Result<T, HivemindError>;

/// Exit codes for CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success = 0,
    Error = 1,
    NotFound = 2,
    Conflict = 3,
    PermissionDenied = 4,
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        code as Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = HivemindError::system("io_error", "Failed to read file", "storage:event_store");
        assert!(err.to_string().contains("system"));
        assert!(err.to_string().contains("io_error"));
    }

    #[test]
    fn error_with_context() {
        let err = HivemindError::user(
            "invalid_name",
            "Project name cannot be empty",
            "cli:project",
        )
        .with_context("field", "name")
        .with_hint("Provide a non-empty project name");

        assert_eq!(err.context.get("field"), Some(&"name".to_string()));
        assert!(err.recovery_hint.is_some());
        assert!(err.recoverable);
    }

    #[test]
    fn error_serialization() {
        let err = HivemindError::git("clone_failed", "Failed to clone repository", "git:clone")
            .with_context("repo", "https://github.com/example/repo");

        let json = serde_json::to_string(&err).expect("serialize");
        let restored: HivemindError = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.category, ErrorCategory::Git);
        assert_eq!(restored.code, "clone_failed");
    }
}
