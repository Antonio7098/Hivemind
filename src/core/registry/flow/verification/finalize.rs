use super::*;

impl Registry {
    pub(crate) fn emit_task_execution_completion_events(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt: &AttemptState,
        completion: CompletionArtifacts<'_>,
        origin: &'static str,
    ) -> Result<()> {
        let state = self.state()?;
        let corr_task = Self::correlation_for_flow_task_event(&state, flow, task_id);
        let corr_attempt =
            Self::correlation_for_flow_task_attempt_event(&state, flow, task_id, attempt.id);

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt.id),
                    from: TaskExecState::Running,
                    to: TaskExecState::Verifying,
                },
                corr_task,
            ),
            origin,
        )?;

        if let Some(commit_sha) = completion.checkpoint_commit_sha {
            self.append_event(
                Event::new(
                    EventPayload::CheckpointCommitCreated {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        commit_sha,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        for change in &completion.artifact.diff.changes {
            self.append_event(
                Event::new(
                    EventPayload::FileModified {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        path: change.path.to_string_lossy().to_string(),
                        change_type: change.change_type,
                        old_hash: change.old_hash.clone(),
                        new_hash: change.new_hash.clone(),
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::DiffComputed {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: attempt.id,
                    diff_id: completion.artifact.diff.id,
                    baseline_id: completion.baseline_id,
                    change_count: completion.artifact.diff.change_count(),
                },
                corr_attempt,
            ),
            origin,
        )?;

        Ok(())
    }

    pub(crate) fn capture_and_store_baseline(
        &self,
        worktree_path: &Path,
        origin: &'static str,
    ) -> Result<Baseline> {
        let baseline = Baseline::capture(worktree_path)
            .map_err(|e| HivemindError::system("baseline_capture_failed", e.to_string(), origin))?;
        self.write_baseline_artifact(&baseline)?;
        Ok(baseline)
    }

    pub(crate) fn compute_and_store_diff(
        &self,
        baseline_id: Uuid,
        worktree_path: &Path,
        task_id: Uuid,
        attempt_id: Uuid,
        origin: &'static str,
    ) -> Result<DiffArtifact> {
        let baseline = self.read_baseline_artifact(baseline_id)?;
        let diff = Diff::compute(&baseline, worktree_path)
            .map_err(|e| HivemindError::system("diff_compute_failed", e.to_string(), origin))?
            .for_task(task_id)
            .for_attempt(attempt_id);

        let mut unified = String::new();
        for change in &diff.changes {
            if let Ok(chunk) = self.unified_diff_for_change(baseline_id, worktree_path, change) {
                unified.push_str(&chunk);
                if !chunk.ends_with('\n') {
                    unified.push('\n');
                }
            }
        }

        let artifact = DiffArtifact { diff, unified };
        self.write_diff_artifact(&artifact)?;
        Ok(artifact)
    }
}
