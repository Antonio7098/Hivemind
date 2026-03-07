use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub fn merge_execute(&self, flow_id: &str) -> Result<crate::core::state::MergeState> {
        let origin = "registry:merge_execute";
        let flow = self.get_flow(flow_id)?;

        let _ = self.enforce_constitution_gate(
            flow.project_id,
            "merge_execute",
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            origin,
        )?;

        let state = self.state()?;
        let ms = state.merge_states.get(&flow.id).ok_or_else(|| {
            HivemindError::user(
                "merge_not_prepared",
                "No merge preparation exists for this flow",
                "registry:merge_execute",
            )
        })?;

        if ms.status != crate::core::state::MergeStatus::Approved {
            return Err(HivemindError::user(
                "merge_not_approved",
                "Merge has not been approved",
                "registry:merge_execute",
            ));
        }

        if flow.state != FlowState::FrozenForMerge {
            return Err(HivemindError::user(
                "flow_not_frozen_for_merge",
                "Flow must be frozen for merge before execution",
                origin,
            ));
        }

        let _integration_lock = self.acquire_flow_integration_lock(flow.id, origin)?;
        self.emit_integration_lock_acquired(&flow, "merge_execute", origin)?;

        let mut commits = Vec::new();
        if state.projects.contains_key(&flow.project_id) {
            let managers = Self::worktree_managers_for_flow(&flow, &state, origin)?;
            let merge_branch = format!("integration/{}/prepare", flow.id);
            let merge_ref = format!("refs/heads/{merge_branch}");

            let mut repo_merge_meta: Vec<(String, PathBuf, String, String, WorktreeManager)> =
                Vec::new();
            for (repo_name, manager) in managers {
                let repo_path = manager.repo_path().to_path_buf();
                let dirty = std::process::Command::new("git")
                    .current_dir(&repo_path)
                    .args(["status", "--porcelain"])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_status_failed", e.to_string(), origin)
                    })?;
                if !dirty.status.success() {
                    return Err(HivemindError::git(
                        "git_status_failed",
                        String::from_utf8_lossy(&dirty.stderr).to_string(),
                        origin,
                    ));
                }
                let has_dirty_files = String::from_utf8_lossy(&dirty.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .any(|l| {
                        let path = l
                            .strip_prefix("?? ")
                            .or_else(|| l.get(3..))
                            .unwrap_or("")
                            .trim();
                        !path.starts_with(".hivemind/")
                    });
                if has_dirty_files {
                    return Err(HivemindError::user(
                        "repo_dirty",
                        format!("Repository '{repo_name}' has uncommitted changes"),
                        origin,
                    ));
                }

                let current_branch = std::process::Command::new("git")
                    .current_dir(&repo_path)
                    .args(["rev-parse", "--abbrev-ref", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map_or_else(
                        || "HEAD".to_string(),
                        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    );
                let target = ms
                    .target_branch
                    .clone()
                    .unwrap_or_else(|| current_branch.clone());
                if target == "HEAD" {
                    return Err(HivemindError::user(
                        "detached_head",
                        format!("Repository '{repo_name}' is in detached HEAD"),
                        origin,
                    ));
                }
                if !Self::git_ref_exists(&repo_path, &merge_ref) {
                    return Err(HivemindError::user(
                        "merge_branch_not_found",
                        format!("Prepared integration branch not found in repo '{repo_name}'"),
                        origin,
                    )
                    .with_hint("Run 'hivemind merge prepare' again"));
                }

                let ff_possible = std::process::Command::new("git")
                    .current_dir(&repo_path)
                    .args(["merge-base", "--is-ancestor", &target, &merge_branch])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !ff_possible {
                    return Err(HivemindError::git(
                        "git_merge_failed",
                        format!("Fast-forward is not possible in repo '{repo_name}'"),
                        origin,
                    ));
                }

                repo_merge_meta.push((repo_name, repo_path, current_branch, target, manager));
            }

            let mut merged: Vec<(PathBuf, String, String)> = Vec::new();
            for (repo_name, repo_path, current_branch, target, manager) in &repo_merge_meta {
                if current_branch != target {
                    let checkout = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args(["checkout", target])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_checkout_failed", e.to_string(), origin)
                        })?;
                    if !checkout.status.success() {
                        return Err(HivemindError::git(
                            "git_checkout_failed",
                            String::from_utf8_lossy(&checkout.stderr).to_string(),
                            origin,
                        ));
                    }
                }

                let old_head = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map_or_else(
                        || "HEAD".to_string(),
                        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    );
                let merge_out = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["merge", "--ff-only", &merge_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_merge_failed", e.to_string(), origin)
                    })?;
                if !merge_out.status.success() {
                    for (merged_repo, rollback_head, checkout_back) in merged.iter().rev() {
                        let _ = std::process::Command::new("git")
                            .current_dir(merged_repo)
                            .args(["reset", "--hard", rollback_head])
                            .output();
                        let _ = std::process::Command::new("git")
                            .current_dir(merged_repo)
                            .args(["checkout", checkout_back])
                            .output();
                    }
                    return Err(HivemindError::git(
                        "git_merge_failed",
                        format!(
                            "Merge failed in repo '{repo_name}': {}",
                            String::from_utf8_lossy(&merge_out.stderr).trim()
                        ),
                        origin,
                    ));
                }

                let new_head = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map_or_else(
                        || "HEAD".to_string(),
                        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    );
                let rev_list = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["rev-list", "--reverse", &format!("{old_head}..{new_head}")])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();
                if !rev_list.is_empty() {
                    commits.extend(
                        rev_list
                            .lines()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(String::from),
                    );
                }
                merged.push((repo_path.clone(), old_head, current_branch.clone()));

                if current_branch != target {
                    let _ = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args(["checkout", current_branch])
                        .output();
                }

                let merge_path = manager
                    .config()
                    .base_dir
                    .join(flow.id.to_string())
                    .join("_merge");
                if merge_path.exists() {
                    let _ = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args([
                            "worktree",
                            "remove",
                            "--force",
                            merge_path.to_str().unwrap_or(""),
                        ])
                        .output();
                    let _ = fs::remove_dir_all(&merge_path);
                }
                let prepare_path = manager
                    .config()
                    .base_dir
                    .join(flow.id.to_string())
                    .join("_integration_prepare");
                if prepare_path.exists() {
                    let _ = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args([
                            "worktree",
                            "remove",
                            "--force",
                            prepare_path.to_str().unwrap_or(""),
                        ])
                        .output();
                    let _ = fs::remove_dir_all(&prepare_path);
                }
                let prepare_branch = format!("integration/{}/prepare", flow.id);
                let _ = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["branch", "-D", &prepare_branch])
                    .output();
                if manager.config().cleanup_on_success {
                    for task_id in flow.task_executions.keys() {
                        let branch = format!("exec/{}/{task_id}", flow.id);
                        let _ = std::process::Command::new("git")
                            .current_dir(repo_path)
                            .args(["branch", "-D", &branch])
                            .output();
                        let integration_branch = format!("integration/{}/{task_id}", flow.id);
                        let _ = std::process::Command::new("git")
                            .current_dir(repo_path)
                            .args(["branch", "-D", &integration_branch])
                            .output();
                    }
                    let flow_branch = format!("flow/{}", flow.id);
                    if current_branch != &flow_branch {
                        let _ = std::process::Command::new("git")
                            .current_dir(repo_path)
                            .args(["branch", "-D", &flow_branch])
                            .output();
                    }
                }
            }
        }

        let event = Event::new(
            EventPayload::MergeCompleted {
                flow_id: flow.id,
                commits,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:merge_execute",
            )
        })?;

        self.trigger_graph_snapshot_refresh(flow.project_id, "merge_completed", origin);

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after execute",
                "registry:merge_execute",
            )
        })
    }

    pub fn merge_execute_with_options(
        &self,
        flow_id: &str,
        options: MergeExecuteOptions,
    ) -> Result<crate::core::state::MergeState> {
        match options.mode {
            MergeExecuteMode::Local => self.merge_execute(flow_id),
            MergeExecuteMode::Pr => self.merge_execute_via_pr(flow_id, options),
        }
    }
}
