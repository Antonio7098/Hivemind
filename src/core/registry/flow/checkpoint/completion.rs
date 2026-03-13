use super::*;

impl Registry {
    fn project_repository_heads_changed_since_graph_snapshot(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<bool> {
        let project = self.get_project(&project_id.to_string())?;
        if project.repositories.is_empty() {
            return Ok(false);
        }
        let Some(snapshot) = self.read_graph_snapshot_artifact(project_id, origin)? else {
            return Ok(true);
        };

        let recorded_heads: BTreeMap<(String, String), String> = snapshot
            .provenance
            .head_commits
            .into_iter()
            .map(|entry| ((entry.repo_name, entry.repo_path), entry.commit_hash))
            .collect();

        if recorded_heads.len() != project.repositories.len() {
            return Ok(true);
        }

        for repo in &project.repositories {
            let current_head = Self::resolve_repo_head_commit(Path::new(&repo.path), origin)?;
            match recorded_heads.get(&(repo.name.clone(), repo.path.clone())) {
                Some(recorded_head) if recorded_head == &current_head => {}
                _ => return Ok(true),
            }
        }

        Ok(false)
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

        let corr_attempt = Self::correlation_for_flow_task_attempt_event(
            &state,
            flow,
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

        if let Err(err) = self.mark_attempt_graph_snapshot_registry_freshness(
            flow.project_id,
            attempt.id,
            "stale",
            origin,
        ) {
            self.record_error_event(&err, corr_attempt.clone());
        }

        match self.project_repository_heads_changed_since_graph_snapshot(flow.project_id, origin) {
            Ok(true) => {
                self.trigger_graph_snapshot_refresh(flow.project_id, "checkpoint_complete", origin);
            }
            Ok(false) => {}
            Err(err) => {
                self.record_error_event(&err, corr_attempt.clone());
                self.trigger_graph_snapshot_refresh(flow.project_id, "checkpoint_complete", origin);
            }
        }

        let _ = self.enforce_constitution_gate(
            flow.project_id,
            "checkpoint_complete",
            corr_attempt,
            origin,
        )?;

        if flow.run_mode == RunMode::Auto && flow.state == FlowState::Running {
            let _ = self.auto_progress_flow(&flow.id.to_string());
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::registry::RegistryConfig;
    use crate::core::scope::RepoAccessMode;
    use tempfile::tempdir;

    #[test]
    fn checkpoint_completion_skips_authoritative_refresh_when_repo_heads_are_unchanged() {
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let repo_dir = temp.path().join("repo");
        std::fs::create_dir_all(&repo_dir).expect("create repo dir");
        std::fs::write(repo_dir.join("README.md"), "seed\n").expect("write seed file");
        init_git_repo(&repo_dir);

        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(data_dir)).expect("registry");
        let project = registry
            .create_project("checkpoint-test", None)
            .expect("project");
        let project_id = project.id.to_string();
        registry
            .attach_repo(
                &project_id,
                repo_dir.to_str().expect("repo path"),
                Some("main"),
                RepoAccessMode::ReadWrite,
            )
            .expect("attach repo");
        registry
            .graph_snapshot_refresh(&project_id, "test")
            .expect("initial graph snapshot refresh");

        let task = registry
            .create_task(&project_id, "Task A", None, None)
            .expect("task");
        let graph = registry
            .create_graph(&project_id, "graph", &[task.id])
            .expect("graph");
        let flow = registry
            .create_flow(&graph.id.to_string(), Some("flow"))
            .expect("flow");
        registry
            .start_flow(&flow.id.to_string())
            .expect("start flow");
        registry
            .start_task_execution(&task.id.to_string())
            .expect("start task execution");

        let attempts = registry
            .list_attempts(Some(&flow.id.to_string()), None, 10)
            .expect("attempts");
        let attempt_id = attempts[0].attempt_id.to_string();
        let checkpoints = registry
            .list_checkpoints(&attempt_id)
            .expect("list checkpoints");
        let worktree_readme = registry
            .worktree_list(&flow.id.to_string())
            .expect("worktree list")
            .into_iter()
            .find(|status| status.task_id == task.id)
            .expect("task worktree status")
            .path
            .join("README.md");
        std::fs::write(worktree_readme, "changed in worktree\n").expect("update worktree readme");

        registry
            .checkpoint_complete(
                &attempt_id,
                &checkpoints[0].checkpoint_id,
                Some("checkpoint"),
            )
            .expect("complete checkpoint");

        let events = registry
            .list_events(Some(flow.project_id), 200)
            .expect("list events");
        assert!(!events.iter().any(|event| {
            matches!(
                event.payload,
                EventPayload::GraphSnapshotStarted { ref trigger, .. }
                    if trigger == "checkpoint_complete"
            )
        }));
    }

    fn init_git_repo(repo_dir: &std::path::Path) {
        run_git(repo_dir, &["init", "-q"]);
        run_git(repo_dir, &["config", "user.name", "Augment Agent"]);
        run_git(repo_dir, &["config", "user.email", "augment@example.com"]);
        run_git(repo_dir, &["add", "README.md"]);
        run_git(repo_dir, &["commit", "-qm", "seed"]);
    }

    fn run_git(repo_dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(repo_dir)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }
}
