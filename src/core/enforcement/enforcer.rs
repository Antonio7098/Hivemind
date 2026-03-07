use super::*;

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
                if !self.scope.filesystem.can_write(&path_str) {
                    return Some(ScopeViolation::filesystem(
                        path_str.to_string(),
                        format!("Delete of '{path_str}' not allowed by scope"),
                    ));
                }
            }
        }

        if self.scope.filesystem.permission_for(&path_str) == Some(FilePermission::Deny) {
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

        let diff_result = self.verify_diff(diff, task_id, attempt_id);
        all_violations.extend(diff_result.violations);

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
