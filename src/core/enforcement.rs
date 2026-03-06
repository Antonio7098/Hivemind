//! Scope enforcement - post-execution violation detection.
//!
//! Sprint 1 enforcement is detection-based, not prevention-based.
//! Violations are detected after execution and made fatal.

use super::diff::{ChangeType, Diff, FileChange};
use super::scope::{FilePermission, Scope};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

mod enforcer;
mod matchers;
mod result;

#[cfg(test)]
mod tests;

/// A scope violation detected post-execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeViolation {
    /// Type of violation.
    pub violation_type: ViolationType,
    /// Path that was violated (if applicable).
    pub path: Option<String>,
    /// Description of the violation.
    pub description: String,
    /// Whether this violation is fatal.
    pub fatal: bool,
}

/// Type of scope violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    /// Filesystem access violation.
    Filesystem,
    /// Git operation violation.
    Git,
    /// Command execution violation.
    Execution,
}

/// Result of scope verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Unique verification ID.
    pub id: Uuid,
    /// Task this verification is for.
    pub task_id: Uuid,
    /// Attempt this verification is for.
    pub attempt_id: Uuid,
    /// Whether verification passed.
    pub passed: bool,
    /// Violations detected.
    pub violations: Vec<ScopeViolation>,
    /// Verification timestamp.
    pub verified_at: chrono::DateTime<chrono::Utc>,
}

/// Scope enforcer for post-execution verification.
pub struct ScopeEnforcer {
    scope: Scope,
}

pub use matchers::path_matches_any;
