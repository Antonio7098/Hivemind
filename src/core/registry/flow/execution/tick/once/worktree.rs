use super::*;

pub(super) struct TickLaunchPrereqs {
    pub(super) runtime: ProjectRuntimeConfig,
    pub(super) runtime_selection_source: RuntimeSelectionSource,
    pub(super) worktree_status: WorktreeStatus,
    pub(super) repo_worktrees: Vec<(String, WorktreeStatus)>,
    pub(super) next_attempt_number: u32,
}

impl Registry {
    pub(super) fn prepare_tick_launch_prereqs(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<TickLaunchPrereqs> {
        let (runtime, runtime_selection_source) = Self::effective_runtime_for_task_with_source(
            state,
            flow,
            task_id,
            RuntimeRole::Worker,
            origin,
        )?;

        let worktree_status = Self::ensure_task_worktree(flow, state, task_id, origin)?;
        let repo_worktrees = Self::inspect_task_worktrees(flow, state, task_id, origin)?;

        let exec = flow.task_executions.get(&task_id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;

        if exec.state == TaskExecState::Retry && exec.retry_mode == RetryMode::Clean {
            let branch = format!("exec/{}/{task_id}", flow.id);
            let managers = Self::worktree_managers_for_flow(flow, state, origin)?;
            for (idx, (_repo_name, repo_worktree)) in repo_worktrees.iter().enumerate() {
                let (_, manager) = &managers[idx];
                let base = Self::default_base_ref_for_repo(flow, manager, idx == 0);
                Self::checkout_and_clean_worktree(&repo_worktree.path, &branch, &base, origin)?;
            }
        }

        Ok(TickLaunchPrereqs {
            runtime,
            runtime_selection_source,
            worktree_status,
            repo_worktrees,
            next_attempt_number: exec.attempt_count.saturating_add(1),
        })
    }

    pub(super) fn merge_task_dependency_branches(
        _state: &AppState,
        flow: &TaskFlow,
        graph: &TaskGraph,
        task_id: Uuid,
        repo_worktrees: &[(String, WorktreeStatus)],
        origin: &'static str,
    ) -> Result<()> {
        if let Some(deps) = graph.dependencies.get(&task_id) {
            let mut dep_ids: Vec<Uuid> = deps.iter().copied().collect();
            dep_ids.sort();

            for (_repo_name, repo_worktree) in repo_worktrees {
                for &dep_task_id in &dep_ids {
                    let dep_branch = format!("exec/{}/{dep_task_id}", flow.id);
                    let dep_ref = format!("refs/heads/{dep_branch}");

                    let ref_exists = Command::new("git")
                        .current_dir(&repo_worktree.path)
                        .args(["show-ref", "--verify", "--quiet", &dep_ref])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if !ref_exists {
                        continue;
                    }

                    let already_contains = Command::new("git")
                        .current_dir(&repo_worktree.path)
                        .args(["merge-base", "--is-ancestor", &dep_branch, "HEAD"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if already_contains {
                        continue;
                    }

                    let merge = Command::new("git")
                        .current_dir(&repo_worktree.path)
                        .args([
                            "-c",
                            "user.name=Hivemind",
                            "-c",
                            "user.email=hivemind@example.com",
                            "merge",
                            "--no-edit",
                            &dep_branch,
                        ])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_merge_failed", e.to_string(), origin)
                        })?;

                    if !merge.status.success() {
                        let _ = Command::new("git")
                            .current_dir(&repo_worktree.path)
                            .args(["merge", "--abort"])
                            .output();
                        return Err(HivemindError::git(
                            "merge_failed",
                            String::from_utf8_lossy(&merge.stderr).to_string(),
                            origin,
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}
