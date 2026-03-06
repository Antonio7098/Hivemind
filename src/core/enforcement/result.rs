use super::*;

impl ScopeViolation {
    /// Creates a filesystem violation.
    pub fn filesystem(path: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            violation_type: ViolationType::Filesystem,
            path: Some(path.into()),
            description: description.into(),
            fatal: true,
        }
    }

    /// Creates a git violation.
    pub fn git(description: impl Into<String>) -> Self {
        Self {
            violation_type: ViolationType::Git,
            path: None,
            description: description.into(),
            fatal: true,
        }
    }

    /// Creates an execution violation.
    pub fn execution(description: impl Into<String>) -> Self {
        Self {
            violation_type: ViolationType::Execution,
            path: None,
            description: description.into(),
            fatal: true,
        }
    }
}

impl VerificationResult {
    /// Creates a passing result.
    pub fn pass(task_id: Uuid, attempt_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            attempt_id,
            passed: true,
            violations: Vec::new(),
            verified_at: chrono::Utc::now(),
        }
    }

    /// Creates a failing result with violations.
    pub fn fail(task_id: Uuid, attempt_id: Uuid, violations: Vec<ScopeViolation>) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            attempt_id,
            passed: false,
            violations,
            verified_at: chrono::Utc::now(),
        }
    }

    /// Returns true if any violations are fatal.
    pub fn has_fatal_violations(&self) -> bool {
        self.violations.iter().any(|v| v.fatal)
    }
}
