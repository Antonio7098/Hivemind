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

    #[allow(clippy::too_many_lines)]
    pub fn checkpoint_complete(
        &self,
        attempt_id: &str,
        checkpoint_id: &str,
        summary: Option<&str>,
    ) -> Result<CheckpointCompletionResult> {
        let origin = "registry:checkpoint_complete";
        let attempt_uuid = Uuid::parse_str(attempt_id).map_err(|_| {
            HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                origin,
            )
        })?;

        let checkpoint_id = checkpoint_id.trim();
        if checkpoint_id.is_empty() {
            return Err(HivemindError::user(
                "invalid_checkpoint_id",
                "Checkpoint ID cannot be empty",
                origin,
            ));
        }

        let state = self.state()?;
        let attempt = state
            .attempts
            .get(&attempt_uuid)
            .ok_or_else(|| HivemindError::user("attempt_not_found", "Attempt not found", origin))?;

        let flow = state.flows.get(&attempt.flow_id).ok_or_else(|| {
            HivemindError::system("flow_not_found", "Flow not found for attempt", origin)
        })?;

        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            attempt.task_id,
            attempt.id,
        );

        if flow.state != FlowState::Running {
            let err = HivemindError::user(
                "flow_not_running",
                "Flow is not running; checkpoint completion rejected",
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        }

        let exec = flow.task_executions.get(&attempt.task_id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Running {
            let err = HivemindError::user(
                "attempt_not_running",
                "Attempt is not in RUNNING state",
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        }

        let graph = state
            .graphs
            .get(&flow.graph_id)
            .ok_or_else(|| HivemindError::system("graph_not_found", "Graph not found", origin))?;
        let graph_task = graph.tasks.get(&attempt.task_id).ok_or_else(|| {
            HivemindError::system("task_not_found", "Task not found in graph", origin)
        })?;

        let checkpoint_ids = Self::normalized_checkpoint_ids(&graph_task.checkpoints);
        let Some((order, total)) = Self::checkpoint_order(&checkpoint_ids, checkpoint_id) else {
            let err = HivemindError::user(
                "checkpoint_not_found",
                format!("Checkpoint '{checkpoint_id}' is not declared for this task"),
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        };

        let Some(current) = attempt
            .checkpoints
            .iter()
            .find(|cp| cp.checkpoint_id == checkpoint_id)
        else {
            let err = HivemindError::user(
                "checkpoint_not_declared",
                format!("Checkpoint '{checkpoint_id}' has not been declared for this attempt"),
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        };

        if current.state == AttemptCheckpointState::Completed {
            let err = HivemindError::user(
                "checkpoint_already_completed",
                format!("Checkpoint '{checkpoint_id}' is already completed"),
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        }

        if current.state != AttemptCheckpointState::Active {
            let err = HivemindError::user(
                "checkpoint_not_active",
                format!("Checkpoint '{checkpoint_id}' is not ACTIVE"),
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        }

        let idx = usize::try_from(order.saturating_sub(1)).map_err(|_| {
            HivemindError::system(
                "checkpoint_order_invalid",
                "Checkpoint order conversion failed",
                origin,
            )
        })?;

        let completed_ids: HashSet<&str> = attempt
            .checkpoints
            .iter()
            .filter(|cp| cp.state == AttemptCheckpointState::Completed)
            .map(|cp| cp.checkpoint_id.as_str())
            .collect();
        for prev in checkpoint_ids.iter().take(idx) {
            if !completed_ids.contains(prev.as_str()) {
                let err = HivemindError::user(
                    "checkpoint_order_violation",
                    format!("Cannot complete '{checkpoint_id}' before '{prev}'"),
                    origin,
                );
                self.record_error_event(&err, corr_attempt);
                return Err(err);
            }
        }

        let worktree = Self::inspect_task_worktree(flow, &state, attempt.task_id, origin)?;
        let commit_hash = match Self::create_checkpoint_commit(
            &worktree.path,
            &CheckpointCommitSpec {
                flow_id: flow.id,
                task_id: attempt.task_id,
                attempt_id: attempt.id,
                checkpoint_id,
                order,
                total,
                summary,
            },
            origin,
        ) {
            Ok(hash) => hash,
            Err(err) => {
                self.record_error_event(&err, corr_attempt);
                return Err(err);
            }
        };

        let completed_at = Utc::now();
        let summary_owned = summary.map(str::to_string);
        self.append_event(
            Event::new(
                EventPayload::CheckpointCompleted {
                    flow_id: flow.id,
                    task_id: attempt.task_id,
                    attempt_id: attempt.id,
                    checkpoint_id: checkpoint_id.to_string(),
                    order,
                    commit_hash: commit_hash.clone(),
                    timestamp: completed_at,
                    summary: summary_owned,
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::CheckpointCommitCreated {
                    flow_id: flow.id,
                    task_id: attempt.task_id,
                    attempt_id: attempt.id,
                    commit_sha: commit_hash.clone(),
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        let next_checkpoint_id = checkpoint_ids.get(idx.saturating_add(1)).cloned();
        if let Some(next_id) = next_checkpoint_id.as_ref() {
            let next_order = order.saturating_add(1);
            self.append_event(
                Event::new(
                    EventPayload::CheckpointActivated {
                        flow_id: flow.id,
                        task_id: attempt.task_id,
                        attempt_id: attempt.id,
                        checkpoint_id: next_id.clone(),
                        order: next_order,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        } else {
            self.append_event(
                Event::new(
                    EventPayload::AllCheckpointsCompleted {
                        flow_id: flow.id,
                        task_id: attempt.task_id,
                        attempt_id: attempt.id,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        self.trigger_graph_snapshot_refresh(flow.project_id, "checkpoint_complete", origin);

        let _ = self.enforce_constitution_gate(
            flow.project_id,
            "checkpoint_complete",
            corr_attempt,
            origin,
        )?;

        Ok(CheckpointCompletionResult {
            flow_id: flow.id,
            task_id: attempt.task_id,
            attempt_id: attempt.id,
            checkpoint_id: checkpoint_id.to_string(),
            order,
            total,
            next_checkpoint_id,
            all_completed: order == total,
            commit_hash,
        })
    }
}
