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
        let state = self.state()?;
        self.append_event(
            Event::new(
                EventPayload::FlowIntegrationLockAcquired {
                    flow_id: flow.id,
                    operation: operation.to_string(),
                },
                Self::correlation_for_flow_event(&state, flow),
            ),
            origin,
        )
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
