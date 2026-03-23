use super::*;
use crate::core::enforcement::ScopeEnforcer;
use crate::core::graph::GraphTask;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn verify_task_scope_artifacts(
        &self,
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        task: &GraphTask,
        attempt: &crate::core::state::AttemptState,
        worktree_status: &WorktreeStatus,
        origin: &'static str,
    ) -> Result<VerificationResult> {
        let diff_id = attempt.diff_id.ok_or_else(|| {
            HivemindError::system("diff_not_found", "Diff not found for attempt", origin)
        })?;
        let artifact = self.read_diff_artifact(diff_id)?;

        let baseline_id = attempt.baseline_id.ok_or_else(|| {
            HivemindError::system(
                "baseline_not_found",
                "Baseline not found for attempt",
                origin,
            )
        })?;
        let baseline = self.read_baseline_artifact(baseline_id)?;

        let mut verification = if let Some(scope) = &task.scope {
            let (commits_created, branches_created) =
                Self::detect_git_operations(&worktree_status.path, &baseline, attempt.id);

            ScopeEnforcer::new(scope.clone()).verify_all(
                &artifact.diff,
                commits_created,
                branches_created,
                task_id,
                attempt.id,
            )
        } else {
            VerificationResult::pass(task_id, attempt.id)
        };

        if let Some(scope) = &task.scope {
            let repo_violations =
                Self::verify_repository_scope(scope, flow, state, task_id, origin);
            if !repo_violations.is_empty() {
                verification.passed = false;
                verification.violations.extend(repo_violations);
            }

            let ambient_violations =
                self.verify_scope_environment_baseline(flow, state, task_id, attempt.id, origin);
            if !ambient_violations.is_empty() {
                verification.passed = false;
                verification.violations.extend(ambient_violations);
            }

            let traced_violations =
                self.verify_scope_trace_writes(flow, state, task_id, attempt.id, origin);
            if !traced_violations.is_empty() {
                verification.passed = false;
                verification.violations.extend(traced_violations);
            }
        }

        Ok(verification)
    }
}
