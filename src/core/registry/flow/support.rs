use super::*;

impl Registry {
    pub(crate) fn checkout_and_clean_worktree(
        worktree_path: &Path,
        branch: &str,
        base: &str,
        origin: &'static str,
    ) -> Result<()> {
        let checkout = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["checkout", "-f", "-B", branch, base])
            .output();
        if !checkout.as_ref().is_ok_and(|o| o.status.success()) {
            return Err(HivemindError::git(
                "git_checkout_failed",
                checkout.map_or_else(
                    |e| e.to_string(),
                    |o| String::from_utf8_lossy(&o.stderr).to_string(),
                ),
                origin,
            ));
        }

        let clean = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["clean", "-fdx"])
            .output();
        if !clean.as_ref().is_ok_and(|o| o.status.success()) {
            return Err(HivemindError::git(
                "git_clean_failed",
                clean.map_or_else(
                    |e| e.to_string(),
                    |o| String::from_utf8_lossy(&o.stderr).to_string(),
                ),
                origin,
            ));
        }

        Ok(())
    }

    pub(crate) fn acquire_flow_integration_lock(
        &self,
        flow_id: Uuid,
        origin: &'static str,
    ) -> Result<std::fs::File> {
        let lock_dir = self.config.data_dir.join("locks");
        fs::create_dir_all(&lock_dir)
            .map_err(|e| HivemindError::system("create_dir_failed", e.to_string(), origin))?;

        let lock_path = lock_dir.join(format!("flow_integration_{flow_id}.lock"));
        for attempt in 0..5 {
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(|e| HivemindError::system("lock_open_failed", e.to_string(), origin))?;

            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(err) => {
                    if attempt < 4 {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    return Err(HivemindError::user(
                        "integration_in_progress",
                        "Another integration operation is already in progress for this flow",
                        origin,
                    )
                    .with_context("flow_id", flow_id.to_string())
                    .with_context("lock_error", err.to_string()));
                }
            }
        }

        Err(HivemindError::user(
            "integration_in_progress",
            "Another integration operation is already in progress for this flow",
            origin,
        )
        .with_context("flow_id", flow_id.to_string())
        .with_context("lock_error", "retry_exhausted"))
    }

    pub(crate) fn resolve_git_ref(repo_path: &Path, reference: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .current_dir(repo_path)
            .args(["rev-parse", reference])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if sha.is_empty() {
            return None;
        }
        Some(sha)
    }

    pub(crate) fn resolve_task_frozen_commit_sha(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
    ) -> Option<String> {
        let manager = Self::worktree_manager_for_flow(flow, state).ok()?;
        let branch_ref = format!("refs/heads/exec/{}/{task_id}", flow.id);
        Self::resolve_git_ref(manager.repo_path(), &branch_ref).or_else(|| {
            let status = manager.inspect(flow.id, task_id).ok()?;
            if !status.is_worktree {
                return None;
            }
            status
                .head_commit
                .or_else(|| Self::resolve_git_ref(&status.path, "HEAD"))
        })
    }

    pub(crate) fn emit_integration_lock_acquired(
        &self,
        flow: &TaskFlow,
        operation: &str,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::FlowIntegrationLockAcquired {
                    flow_id: flow.id,
                    operation: operation.to_string(),
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            origin,
        )
    }

    pub(crate) fn attempt_runtime_outcome(
        &self,
        attempt_id: Uuid,
    ) -> Result<(Option<i32>, Option<String>)> {
        let filter = EventFilter {
            attempt_id: Some(attempt_id),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;

        let mut exit_code: Option<i32> = None;
        let mut terminated: Option<String> = None;
        for ev in events {
            match ev.payload {
                EventPayload::RuntimeExited { exit_code: ec, .. } => {
                    exit_code = Some(ec);
                }
                EventPayload::RuntimeTerminated { reason, .. } => {
                    terminated = Some(reason);
                }
                _ => {}
            }
        }

        if exit_code.is_none() && terminated.is_none() {
            return Ok((None, None));
        }
        Ok((exit_code, terminated))
    }
    #[allow(
        clippy::type_complexity,
        clippy::too_many_lines,
        clippy::unnecessary_wraps
    )]
    pub(crate) fn build_retry_context(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_number: u32,
        max_attempts: u32,
        _origin: &'static str,
    ) -> Result<(
        String,
        Vec<AttemptSummary>,
        Vec<Uuid>,
        Vec<String>,
        Vec<String>,
        Option<i32>,
        Option<String>,
    )> {
        let mut attempts: Vec<AttemptState> = state
            .attempts
            .values()
            .filter(|a| a.flow_id == flow.id && a.task_id == task_id)
            .filter(|a| a.attempt_number < attempt_number)
            .cloned()
            .collect();
        attempts.sort_by_key(|a| a.attempt_number);

        let prior_attempt_ids: Vec<Uuid> = attempts.iter().map(|a| a.id).collect();

        let mut prior_attempts: Vec<AttemptSummary> = Vec::new();
        for prior in &attempts {
            let (exit_code, terminated_reason) = self
                .attempt_runtime_outcome(prior.id)
                .unwrap_or((None, None));

            let required_failed: Vec<String> = prior
                .check_results
                .iter()
                .filter(|r| r.required && !r.passed)
                .map(|r| r.name.clone())
                .collect();
            let optional_failed: Vec<String> = prior
                .check_results
                .iter()
                .filter(|r| !r.required && !r.passed)
                .map(|r| r.name.clone())
                .collect();
            let incomplete_checkpoints: Vec<String> = prior
                .checkpoints
                .iter()
                .filter(|cp| cp.state != AttemptCheckpointState::Completed)
                .map(|cp| cp.checkpoint_id.clone())
                .collect();

            let mut summary = String::new();
            if let Some(diff_id) = prior.diff_id {
                if let Ok(artifact) = self.read_diff_artifact(diff_id) {
                    let _ = write!(summary, "change_count={} ", artifact.diff.change_count());
                }
            }

            if required_failed.is_empty() && optional_failed.is_empty() {
                if let Some(ec) = exit_code {
                    let _ = write!(summary, "runtime_exit_code={ec} ");
                }
                if let Some(ref reason) = terminated_reason {
                    let _ = write!(summary, "runtime_terminated={reason} ");
                }
            }

            if !required_failed.is_empty() {
                let _ = write!(
                    summary,
                    "required_checks_failed={} ",
                    required_failed.join(", ")
                );
            }
            if !optional_failed.is_empty() {
                let _ = write!(
                    summary,
                    "optional_checks_failed={} ",
                    optional_failed.join(", ")
                );
            }
            if !incomplete_checkpoints.is_empty() {
                let _ = write!(
                    summary,
                    "pending_checkpoints={} ",
                    incomplete_checkpoints.join(", ")
                );
            }

            if summary.trim().is_empty() {
                summary = "no recorded outcomes".to_string();
            }

            let failure_reason = if !required_failed.is_empty() {
                Some(format!(
                    "Required checks failed: {}",
                    required_failed.join(", ")
                ))
            } else if !optional_failed.is_empty() {
                Some(format!(
                    "Optional checks failed: {}",
                    optional_failed.join(", ")
                ))
            } else if !incomplete_checkpoints.is_empty() {
                Some(format!(
                    "Checkpoints incomplete: {}",
                    incomplete_checkpoints.join(", ")
                ))
            } else if let Some(ec) = exit_code {
                if ec != 0 {
                    Some(format!("Runtime exited with code {ec}"))
                } else {
                    None
                }
            } else if terminated_reason.is_some() {
                Some("Runtime terminated".to_string())
            } else {
                None
            };

            prior_attempts.push(AttemptSummary {
                attempt_number: prior.attempt_number,
                summary: summary.trim().to_string(),
                failure_reason,
            });
        }

        let latest = attempts.last();
        let mut required_failures: Vec<String> = Vec::new();
        let mut optional_failures: Vec<String> = Vec::new();
        let mut incomplete_checkpoints: Vec<String> = Vec::new();
        let mut exit_code: Option<i32> = None;
        #[allow(clippy::useless_let_if_seq, clippy::option_if_let_else)]
        let terminated_reason = if let Some(last) = latest {
            required_failures = last
                .check_results
                .iter()
                .filter(|r| r.required && !r.passed)
                .map(|r| r.name.clone())
                .collect();
            optional_failures = last
                .check_results
                .iter()
                .filter(|r| !r.required && !r.passed)
                .map(|r| r.name.clone())
                .collect();
            incomplete_checkpoints = last
                .checkpoints
                .iter()
                .filter(|cp| cp.state != AttemptCheckpointState::Completed)
                .map(|cp| cp.checkpoint_id.clone())
                .collect();

            let (ec, term) = self
                .attempt_runtime_outcome(last.id)
                .unwrap_or((None, None));
            exit_code = ec;
            term
        } else {
            None
        };

        #[allow(clippy::items_after_statements)]
        fn truncate(s: &str, max_len: usize) -> String {
            if s.len() <= max_len {
                return s.to_string();
            }
            s.chars().take(max_len).collect()
        }

        let mut ctx = String::new();
        let _ = writeln!(
            ctx,
            "Retry attempt {attempt_number}/{max_attempts} for task {task_id}"
        );

        if let Some(last) = latest {
            let _ = writeln!(ctx, "Previous attempt: {}", last.id);

            if !required_failures.is_empty() {
                let _ = writeln!(
                    ctx,
                    "Required check failures (must fix): {}",
                    required_failures.join(", ")
                );
                for r in last
                    .check_results
                    .iter()
                    .filter(|r| r.required && !r.passed)
                {
                    let _ = writeln!(ctx, "--- Check: {}", r.name);
                    let _ = writeln!(ctx, "exit_code={}", r.exit_code);
                    let _ = writeln!(ctx, "output:\n{}", truncate(&r.output, 2000));
                }
            }

            if !optional_failures.is_empty() {
                let _ = writeln!(
                    ctx,
                    "Optional check failures: {}",
                    optional_failures.join(", ")
                );
            }

            if let Some(ec) = exit_code {
                let _ = writeln!(ctx, "Runtime exit code: {ec}");
            }
            if let Some(ref reason) = terminated_reason {
                let _ = writeln!(ctx, "Runtime terminated: {reason}");
            }
            if !incomplete_checkpoints.is_empty() {
                let _ = writeln!(
                    ctx,
                    "Incomplete checkpoints from prior attempt: {}",
                    incomplete_checkpoints.join(", ")
                );
            }

            if let Some(diff_id) = last.diff_id {
                if let Ok(artifact) = self.read_diff_artifact(diff_id) {
                    let created: Vec<String> = artifact
                        .diff
                        .changes
                        .iter()
                        .filter(|c| c.change_type == ChangeType::Created)
                        .map(|c| c.path.to_string_lossy().to_string())
                        .collect();
                    let modified: Vec<String> = artifact
                        .diff
                        .changes
                        .iter()
                        .filter(|c| c.change_type == ChangeType::Modified)
                        .map(|c| c.path.to_string_lossy().to_string())
                        .collect();
                    let deleted: Vec<String> = artifact
                        .diff
                        .changes
                        .iter()
                        .filter(|c| c.change_type == ChangeType::Deleted)
                        .map(|c| c.path.to_string_lossy().to_string())
                        .collect();

                    let _ = writeln!(
                            ctx,
                            "Filesystem changes observed (from diff): change_count={} created={} modified={} deleted={}",
                            artifact.diff.change_count(),
                            created.len(),
                            modified.len(),
                            deleted.len()
                        );
                    if !created.is_empty() {
                        let _ = writeln!(ctx, "Created:\n{}", created.join("\n"));
                    }
                    if !modified.is_empty() {
                        let _ = writeln!(ctx, "Modified:\n{}", modified.join("\n"));
                    }
                    if !deleted.is_empty() {
                        let _ = writeln!(ctx, "Deleted:\n{}", deleted.join("\n"));
                    }
                }
            }
        }

        Ok((
            ctx,
            prior_attempts,
            prior_attempt_ids,
            required_failures,
            optional_failures,
            exit_code,
            terminated_reason,
        ))
    }

    pub(crate) fn flow_for_task(
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<TaskFlow> {
        state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&task_id))
            .max_by_key(|f| (f.updated_at, f.id))
            .cloned()
            .ok_or_else(|| {
                HivemindError::user("task_not_in_flow", "Task is not part of any flow", origin)
            })
    }

    pub(crate) fn inspect_task_worktree(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<WorktreeStatus> {
        let manager = Self::worktree_manager_for_flow(flow, state)?;
        let status = manager
            .inspect(flow.id, task_id)
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        if !status.is_worktree {
            return Err(HivemindError::user(
                "worktree_not_found",
                "Worktree not found for task",
                origin,
            ));
        }
        Ok(status)
    }

    pub(crate) fn resolve_latest_attempt_without_diff(
        state: &AppState,
        flow_id: Uuid,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<AttemptState> {
        state
            .attempts
            .values()
            .filter(|a| a.flow_id == flow_id && a.task_id == task_id)
            .filter(|a| a.diff_id.is_none())
            .max_by_key(|a| a.started_at)
            .cloned()
            .ok_or_else(|| {
                HivemindError::system(
                    "attempt_not_found",
                    "Attempt not found for running task",
                    origin,
                )
            })
    }

    pub(crate) fn fail_running_attempt(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        reason: &str,
        origin: &'static str,
    ) -> Result<()> {
        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, task_id);
        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt_id),
                    from: TaskExecState::Running,
                    to: TaskExecState::Failed,
                },
                corr_task,
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionFailed {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt_id),
                    reason: Some(reason.to_string()),
                },
                corr_attempt,
            ),
            origin,
        )
    }

    pub(crate) fn resolve_latest_attempt_with_diff(
        state: &AppState,
        flow_id: Uuid,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<AttemptState> {
        state
            .attempts
            .values()
            .filter(|a| a.flow_id == flow_id && a.task_id == task_id)
            .filter(|a| a.diff_id.is_some())
            .max_by_key(|a| a.started_at)
            .cloned()
            .ok_or_else(|| {
                HivemindError::system(
                    "attempt_not_found",
                    "Attempt not found for verifying task",
                    origin,
                )
            })
    }

    pub(crate) fn project_for_flow<'a>(
        flow: &TaskFlow,
        state: &'a AppState,
    ) -> Result<&'a Project> {
        state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                "registry:worktree_manager_for_flow",
            )
        })
    }

    pub(crate) fn git_ref_exists(repo_path: &Path, reference: &str) -> bool {
        std::process::Command::new("git")
            .current_dir(repo_path)
            .args(["show-ref", "--verify", "--quiet", reference])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    pub(crate) fn default_base_ref_for_repo(
        flow: &TaskFlow,
        manager: &WorktreeManager,
        is_primary: bool,
    ) -> String {
        let flow_ref = format!("refs/heads/flow/{}", flow.id);
        if Self::git_ref_exists(manager.repo_path(), &flow_ref) {
            return format!("flow/{}", flow.id);
        }
        if is_primary {
            return flow
                .base_revision
                .clone()
                .unwrap_or_else(|| "HEAD".to_string());
        }
        "HEAD".to_string()
    }

    pub(crate) fn ensure_task_worktree_status(
        manager: &WorktreeManager,
        flow: &TaskFlow,
        task_id: Uuid,
        base_ref: &str,
        origin: &'static str,
    ) -> Result<WorktreeStatus> {
        let status = manager
            .inspect(flow.id, task_id)
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        if status.is_worktree {
            return Ok(status);
        }

        manager
            .create(flow.id, task_id, Some(base_ref))
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        let status = manager
            .inspect(flow.id, task_id)
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        if !status.is_worktree {
            return Err(HivemindError::git(
                "worktree_create_failed",
                format!(
                    "Worktree path exists but is not a git worktree: {}",
                    status.path.display()
                ),
                origin,
            ));
        }
        Ok(status)
    }

    pub(crate) fn ensure_task_worktree(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<WorktreeStatus> {
        let managers = Self::worktree_managers_for_flow(flow, state, origin)?;
        let mut primary_status: Option<WorktreeStatus> = None;
        for (idx, (_repo_name, manager)) in managers.iter().enumerate() {
            let base_ref = Self::default_base_ref_for_repo(flow, manager, idx == 0);
            let status =
                Self::ensure_task_worktree_status(manager, flow, task_id, &base_ref, origin)?;
            if idx == 0 {
                primary_status = Some(status);
            }
        }
        primary_status.ok_or_else(|| {
            HivemindError::user(
                "project_has_no_repo",
                "Project has no repository attached",
                origin,
            )
        })
    }

    pub(crate) fn inspect_task_worktrees(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<Vec<(String, WorktreeStatus)>> {
        let managers = Self::worktree_managers_for_flow(flow, state, origin)?;
        let mut statuses = Vec::new();
        for (repo_name, manager) in managers {
            let status = manager
                .inspect(flow.id, task_id)
                .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
            if !status.is_worktree {
                return Err(HivemindError::user(
                    "worktree_not_found",
                    format!("Worktree not found for task in repo '{repo_name}'"),
                    origin,
                ));
            }
            statuses.push((repo_name, status));
        }
        Ok(statuses)
    }
}
