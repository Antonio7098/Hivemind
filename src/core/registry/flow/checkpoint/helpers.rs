use super::*;

impl Registry {
    pub(crate) fn format_checkpoint_commit_message(spec: &CheckpointCommitSpec<'_>) -> String {
        let mut message = String::new();
        let _ = writeln!(message, "hivemind(checkpoint): {}", spec.checkpoint_id);
        let _ = writeln!(message);
        let _ = writeln!(message, "Flow: {}", spec.flow_id);
        let _ = writeln!(message, "Task: {}", spec.task_id);
        let _ = writeln!(message, "Attempt: {}", spec.attempt_id);
        let _ = writeln!(message, "Checkpoint: {}/{}", spec.order, spec.total);
        let _ = writeln!(message, "Schema: checkpoint-v1");
        if let Some(summary) = spec.summary {
            let _ = writeln!(message);
            let _ = writeln!(message, "Summary:");
            let _ = writeln!(message, "{summary}");
        }
        let _ = writeln!(message);
        let _ = writeln!(message, "---");
        let _ = writeln!(
            message,
            "Generated-by: Hivemind {}",
            env!("CARGO_PKG_VERSION")
        );
        message
    }

    pub(crate) fn create_checkpoint_commit(
        worktree_path: &Path,
        spec: &CheckpointCommitSpec<'_>,
        origin: &'static str,
    ) -> Result<String> {
        let add = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["add", "-A"])
            .output()
            .map_err(|e| HivemindError::git("git_add_failed", e.to_string(), origin))?;
        if !add.status.success() {
            return Err(HivemindError::git(
                "git_add_failed",
                String::from_utf8_lossy(&add.stderr).to_string(),
                origin,
            ));
        }

        let message = Self::format_checkpoint_commit_message(spec);
        let commit = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit",
                "--allow-empty",
                "-m",
                &message,
            ])
            .output()
            .map_err(|e| HivemindError::git("git_commit_failed", e.to_string(), origin))?;
        if !commit.status.success() {
            return Err(HivemindError::git(
                "git_commit_failed",
                String::from_utf8_lossy(&commit.stderr).to_string(),
                origin,
            ));
        }

        let head = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| HivemindError::git("git_rev_parse_failed", e.to_string(), origin))?;
        if !head.status.success() {
            return Err(HivemindError::git(
                "git_rev_parse_failed",
                String::from_utf8_lossy(&head.stderr).to_string(),
                origin,
            ));
        }
        Ok(String::from_utf8_lossy(&head.stdout).trim().to_string())
    }

    pub(crate) fn normalized_checkpoint_ids(raw: &[String]) -> Vec<String> {
        let mut ids = Vec::new();
        let mut seen = HashSet::new();

        for candidate in raw {
            let trimmed = candidate.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_string()) {
                ids.push(trimmed.to_string());
            }
        }

        if ids.is_empty() {
            ids.push("checkpoint-1".to_string());
        }

        ids
    }

    pub(crate) fn checkpoint_order(
        checkpoint_ids: &[String],
        checkpoint_id: &str,
    ) -> Option<(u32, u32)> {
        let idx = checkpoint_ids.iter().position(|id| id == checkpoint_id)?;
        let order = u32::try_from(idx.saturating_add(1)).ok()?;
        let total = u32::try_from(checkpoint_ids.len()).ok()?;
        Some((order, total))
    }

    pub(crate) fn emit_task_execution_frozen(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        commit_sha: Option<String>,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::TaskExecutionFrozen {
                    flow_id: flow.id,
                    task_id,
                    commit_sha,
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            ),
            origin,
        )
    }
}
