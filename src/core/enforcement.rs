//! Scope enforcement - post-execution violation detection.
//!
//! Phase 1 enforcement is detection-based, not prevention-based.
//! Violations are detected after execution and made fatal.

use super::diff::{ChangeType, Diff, FileChange};
use super::scope::{FilePermission, Scope};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

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

/// Scope enforcer for post-execution verification.
pub struct ScopeEnforcer {
    scope: Scope,
}

impl ScopeEnforcer {
    /// Creates a new scope enforcer.
    pub fn new(scope: Scope) -> Self {
        Self { scope }
    }

    /// Verifies that a diff does not violate the scope.
    pub fn verify_diff(&self, diff: &Diff, task_id: Uuid, attempt_id: Uuid) -> VerificationResult {
        let mut violations = Vec::new();

        for change in &diff.changes {
            if let Some(violation) = self.check_file_change(change) {
                violations.push(violation);
            }
        }

        if violations.is_empty() {
            VerificationResult::pass(task_id, attempt_id)
        } else {
            VerificationResult::fail(task_id, attempt_id, violations)
        }
    }

    /// Checks a single file change against the scope.
    fn check_file_change(&self, change: &FileChange) -> Option<ScopeViolation> {
        let path_str = change.path.to_string_lossy();

        match change.change_type {
            ChangeType::Created | ChangeType::Modified => {
                // Write operations require write permission
                if !self.scope.filesystem.can_write(&path_str) {
                    return Some(ScopeViolation::filesystem(
                        path_str.to_string(),
                        format!(
                            "Write to '{}' not allowed by scope (change type: {:?})",
                            path_str, change.change_type
                        ),
                    ));
                }
            }
            ChangeType::Deleted => {
                // Deletion requires write permission
                if !self.scope.filesystem.can_write(&path_str) {
                    return Some(ScopeViolation::filesystem(
                        path_str.to_string(),
                        format!("Delete of '{path_str}' not allowed by scope"),
                    ));
                }
            }
        }

        // Check for denied paths
        if let Some(FilePermission::Deny) = self.scope.filesystem.permission_for(&path_str) {
            return Some(ScopeViolation::filesystem(
                path_str.to_string(),
                format!("Access to '{path_str}' is explicitly denied"),
            ));
        }

        None
    }

    /// Verifies git operations against the scope.
    pub fn verify_git_operations(
        &self,
        commits_created: bool,
        branches_created: bool,
        task_id: Uuid,
        attempt_id: Uuid,
    ) -> VerificationResult {
        let mut violations = Vec::new();

        if commits_created && !self.scope.git.can_commit() {
            violations.push(ScopeViolation::git(
                "Commits were created but git commit permission not granted",
            ));
        }

        if branches_created && !self.scope.git.can_branch() {
            violations.push(ScopeViolation::git(
                "Branches were created but git branch permission not granted",
            ));
        }

        if violations.is_empty() {
            VerificationResult::pass(task_id, attempt_id)
        } else {
            VerificationResult::fail(task_id, attempt_id, violations)
        }
    }

    /// Performs full verification including diff and git operations.
    pub fn verify_all(
        &self,
        diff: &Diff,
        commits_created: bool,
        branches_created: bool,
        task_id: Uuid,
        attempt_id: Uuid,
    ) -> VerificationResult {
        let mut all_violations = Vec::new();

        // Check diff
        let diff_result = self.verify_diff(diff, task_id, attempt_id);
        all_violations.extend(diff_result.violations);

        // Check git
        let git_result =
            self.verify_git_operations(commits_created, branches_created, task_id, attempt_id);
        all_violations.extend(git_result.violations);

        if all_violations.is_empty() {
            VerificationResult::pass(task_id, attempt_id)
        } else {
            VerificationResult::fail(task_id, attempt_id, all_violations)
        }
    }

    /// Returns the scope being enforced.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
}

/// Checks if a path matches any pattern in a list.
pub fn path_matches_any(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    for pattern in patterns {
        if pattern.contains('*') {
            // Simple glob matching
            if pattern == "*" {
                return true;
            }
            if let Some(prefix) = pattern.strip_suffix("/*") {
                if path_str.starts_with(prefix) {
                    return true;
                }
            }
            if let Some(suffix) = pattern.strip_prefix("*/") {
                if path_str.ends_with(suffix) {
                    return true;
                }
            }
        } else if path_str.starts_with(pattern) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::diff::{ChangeType, Diff, FileChange};
    use crate::core::scope::{FilesystemScope, GitScope, PathRule, Scope};
    use std::path::PathBuf;

    fn test_scope() -> Scope {
        Scope::new()
            .with_filesystem(
                FilesystemScope::new()
                    .with_rule(PathRule::write("src/"))
                    .with_rule(PathRule::read("docs/"))
                    .with_rule(PathRule::deny("secrets/")),
            )
            .with_git(GitScope::with_commit())
    }

    fn test_diff_with_changes(changes: Vec<FileChange>) -> Diff {
        Diff {
            id: Uuid::new_v4(),
            task_id: None,
            attempt_id: None,
            baseline_id: Uuid::new_v4(),
            changes,
            computed_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn allowed_write_passes() {
        let enforcer = ScopeEnforcer::new(test_scope());
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        let diff = test_diff_with_changes(vec![FileChange {
            path: PathBuf::from("src/main.rs"),
            change_type: ChangeType::Modified,
            old_hash: Some("old".to_string()),
            new_hash: Some("new".to_string()),
        }]);

        let result = enforcer.verify_diff(&diff, task_id, attempt_id);
        assert!(result.passed);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn disallowed_write_fails() {
        let enforcer = ScopeEnforcer::new(test_scope());
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        let diff = test_diff_with_changes(vec![FileChange {
            path: PathBuf::from("docs/README.md"),
            change_type: ChangeType::Modified,
            old_hash: Some("old".to_string()),
            new_hash: Some("new".to_string()),
        }]);

        let result = enforcer.verify_diff(&diff, task_id, attempt_id);
        assert!(!result.passed);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(
            result.violations[0].violation_type,
            ViolationType::Filesystem
        );
    }

    #[test]
    fn denied_path_fails() {
        let enforcer = ScopeEnforcer::new(test_scope());
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        let diff = test_diff_with_changes(vec![FileChange {
            path: PathBuf::from("secrets/api_key.txt"),
            change_type: ChangeType::Created,
            old_hash: None,
            new_hash: Some("new".to_string()),
        }]);

        let result = enforcer.verify_diff(&diff, task_id, attempt_id);
        assert!(!result.passed);
    }

    #[test]
    fn git_commit_allowed() {
        let enforcer = ScopeEnforcer::new(test_scope());
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        let result = enforcer.verify_git_operations(true, false, task_id, attempt_id);
        assert!(result.passed);
    }

    #[test]
    fn git_commit_disallowed() {
        let scope = Scope::new().with_git(GitScope::new()); // No commit permission
        let enforcer = ScopeEnforcer::new(scope);
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        let result = enforcer.verify_git_operations(true, false, task_id, attempt_id);
        assert!(!result.passed);
        assert_eq!(result.violations[0].violation_type, ViolationType::Git);
    }

    #[test]
    fn git_branch_disallowed() {
        let scope = Scope::new().with_git(GitScope::with_commit()); // Commit but not branch
        let enforcer = ScopeEnforcer::new(scope);
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        let result = enforcer.verify_git_operations(false, true, task_id, attempt_id);
        assert!(!result.passed);
    }

    #[test]
    fn full_verification() {
        let enforcer = ScopeEnforcer::new(test_scope());
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        let diff = test_diff_with_changes(vec![FileChange {
            path: PathBuf::from("src/lib.rs"),
            change_type: ChangeType::Created,
            old_hash: None,
            new_hash: Some("new".to_string()),
        }]);

        let result = enforcer.verify_all(&diff, true, false, task_id, attempt_id);
        assert!(result.passed);
    }

    #[test]
    fn full_verification_with_violations() {
        let enforcer = ScopeEnforcer::new(test_scope());
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();

        let diff = test_diff_with_changes(vec![
            FileChange {
                path: PathBuf::from("src/lib.rs"),
                change_type: ChangeType::Created,
                old_hash: None,
                new_hash: Some("new".to_string()),
            },
            FileChange {
                path: PathBuf::from("config/settings.json"),
                change_type: ChangeType::Modified,
                old_hash: Some("old".to_string()),
                new_hash: Some("new".to_string()),
            },
        ]);

        let result = enforcer.verify_all(&diff, true, true, task_id, attempt_id);
        assert!(!result.passed);
        // Should have violations for: config/ write and branch creation
        assert!(result.violations.len() >= 2);
    }

    #[test]
    fn violation_serialization() {
        let violation = ScopeViolation::filesystem("test.txt", "Write not allowed");
        let json = serde_json::to_string(&violation).unwrap();
        let restored: ScopeViolation = serde_json::from_str(&json).unwrap();

        assert_eq!(violation, restored);
    }

    #[test]
    fn verification_result_serialization() {
        let result = VerificationResult::pass(Uuid::new_v4(), Uuid::new_v4());
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"passed\":true"));
    }

    #[test]
    fn path_matching() {
        let patterns = vec!["src/".to_string(), "tests/*".to_string()];

        assert!(path_matches_any(Path::new("src/main.rs"), &patterns));
        assert!(path_matches_any(Path::new("tests/test1.rs"), &patterns));
        assert!(!path_matches_any(Path::new("docs/README.md"), &patterns));
    }
}
