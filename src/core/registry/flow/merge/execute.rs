use super::*;

impl Registry {
    pub fn merge_approve(&self, flow_id: &str) -> Result<crate::core::state::MergeState> {
        let flow = self.get_flow(flow_id)?;
        let origin = "registry:merge_approve";

        let _ = self.enforce_constitution_gate(
            flow.project_id,
            "merge_approve",
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            origin,
        )?;

        let state = self.state()?;
        let ms = state.merge_states.get(&flow.id).ok_or_else(|| {
            HivemindError::user(
                "merge_not_prepared",
                "No merge preparation exists for this flow",
                origin,
            )
        })?;

        if ms.status == crate::core::state::MergeStatus::Approved {
            return Ok(ms.clone());
        }

        if !ms.conflicts.is_empty() {
            return Err(HivemindError::user(
                "unresolved_conflicts",
                "Merge has unresolved conflicts",
                origin,
            ));
        }

        let user = env::var("HIVEMIND_USER")
            .or_else(|_| env::var("USER"))
            .ok()
            .filter(|u| !u.trim().is_empty());

        let event = Event::new(
            EventPayload::MergeApproved {
                flow_id: flow.id,
                user,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store
            .append(event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after approve",
                origin,
            )
        })
    }

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

    #[allow(clippy::too_many_lines)]
    pub(crate) fn merge_execute_via_pr(
        &self,
        flow_id: &str,
        options: MergeExecuteOptions,
    ) -> Result<crate::core::state::MergeState> {
        let origin = "registry:merge_execute_via_pr";
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
                origin,
            )
        })?;

        if ms.status != crate::core::state::MergeStatus::Approved {
            return Err(HivemindError::user(
                "merge_not_approved",
                "Merge has not been approved",
                origin,
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
        self.emit_integration_lock_acquired(&flow, "merge_execute_pr", origin)?;

        let managers = Self::worktree_managers_for_flow(&flow, &state, origin)?;
        if managers.len() != 1 {
            return Err(HivemindError::user(
                "pr_merge_multi_repo_unsupported",
                "PR merge mode currently supports exactly one repository",
                origin,
            ));
        }
        let (_repo_name, manager) = managers.into_iter().next().ok_or_else(|| {
            HivemindError::system(
                "repo_not_found",
                "No repository attached to project",
                origin,
            )
        })?;
        let repo_path = manager.repo_path().to_path_buf();
        let merge_branch = format!("integration/{}/prepare", flow.id);
        let merge_ref = format!("refs/heads/{merge_branch}");
        if !Self::git_ref_exists(&repo_path, &merge_ref) {
            return Err(HivemindError::user(
                "merge_branch_not_found",
                "Prepared integration branch not found",
                origin,
            )
            .with_hint("Run 'hivemind merge prepare' again"));
        }

        let target_branch = ms
            .target_branch
            .clone()
            .unwrap_or_else(|| "main".to_string());
        let old_target_head = std::process::Command::new("git")
            .current_dir(&repo_path)
            .args(["rev-parse", &target_branch])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let push = std::process::Command::new("git")
            .current_dir(&repo_path)
            .args(["push", "--set-upstream", "origin", &merge_branch])
            .output()
            .map_err(|e| HivemindError::system("git_push_failed", e.to_string(), origin))?;
        if !push.status.success() {
            return Err(HivemindError::git(
                "git_push_failed",
                String::from_utf8_lossy(&push.stderr).to_string(),
                origin,
            ));
        }

        let title = format!("Hivemind flow {} merge", flow.id);
        let body = format!(
            "Automated merge PR for flow {}\n\nTarget: {}\n\nGenerated by Hivemind.",
            flow.id, target_branch
        );

        let create = std::process::Command::new("gh")
            .current_dir(&repo_path)
            .args([
                "pr",
                "create",
                "--base",
                &target_branch,
                "--head",
                &merge_branch,
                "--title",
                &title,
                "--body",
                &body,
            ])
            .output()
            .map_err(|e| HivemindError::system("gh_not_available", e.to_string(), origin))?;

        if !create.status.success() {
            let list_existing = std::process::Command::new("gh")
                .current_dir(&repo_path)
                .args([
                    "pr",
                    "list",
                    "--state",
                    "open",
                    "--base",
                    &target_branch,
                    "--head",
                    &merge_branch,
                    "--json",
                    "number",
                    "--jq",
                    ".[0].number",
                ])
                .output()
                .map_err(|e| HivemindError::system("gh_pr_list_failed", e.to_string(), origin))?;

            let existing = String::from_utf8_lossy(&list_existing.stdout)
                .trim()
                .to_string();
            if existing.is_empty() {
                return Err(HivemindError::system(
                    "gh_pr_create_failed",
                    String::from_utf8_lossy(&create.stderr).to_string(),
                    origin,
                ));
            }
        }

        let pr_number_out = std::process::Command::new("gh")
            .current_dir(&repo_path)
            .args(["pr", "view", "--json", "number", "--jq", ".number"])
            .output()
            .map_err(|e| HivemindError::system("gh_pr_view_failed", e.to_string(), origin))?;
        if !pr_number_out.status.success() {
            return Err(HivemindError::system(
                "gh_pr_view_failed",
                String::from_utf8_lossy(&pr_number_out.stderr).to_string(),
                origin,
            ));
        }
        let pr_number = String::from_utf8_lossy(&pr_number_out.stdout)
            .trim()
            .to_string();
        if pr_number.is_empty() {
            return Err(HivemindError::system(
                "gh_pr_view_failed",
                "Unable to resolve PR number".to_string(),
                origin,
            ));
        }

        if options.monitor_ci {
            let checks = std::process::Command::new("gh")
                .current_dir(&repo_path)
                .args(["pr", "checks", &pr_number, "--watch", "--required"])
                .output()
                .map_err(|e| HivemindError::system("gh_pr_checks_failed", e.to_string(), origin))?;
            if !checks.status.success() {
                return Err(HivemindError::system(
                    "gh_pr_checks_failed",
                    String::from_utf8_lossy(&checks.stderr).to_string(),
                    origin,
                ));
            }
        }

        let merged_now = if options.auto_merge {
            let mut args = vec!["pr", "merge", &pr_number, "--squash", "--delete-branch"];
            if !options.monitor_ci {
                args.push("--auto");
            }
            let merge = std::process::Command::new("gh")
                .current_dir(&repo_path)
                .args(args)
                .output()
                .map_err(|e| HivemindError::system("gh_pr_merge_failed", e.to_string(), origin))?;
            if !merge.status.success() {
                return Err(HivemindError::system(
                    "gh_pr_merge_failed",
                    String::from_utf8_lossy(&merge.stderr).to_string(),
                    origin,
                ));
            }
            options.monitor_ci
        } else {
            false
        };

        if options.pull_after && merged_now {
            let checkout = std::process::Command::new("git")
                .current_dir(&repo_path)
                .args(["checkout", &target_branch])
                .output()
                .map_err(|e| HivemindError::system("git_checkout_failed", e.to_string(), origin))?;
            if !checkout.status.success() {
                return Err(HivemindError::git(
                    "git_checkout_failed",
                    String::from_utf8_lossy(&checkout.stderr).to_string(),
                    origin,
                ));
            }

            let pull = std::process::Command::new("git")
                .current_dir(&repo_path)
                .args(["pull", "--ff-only", "origin", &target_branch])
                .output()
                .map_err(|e| HivemindError::system("git_pull_failed", e.to_string(), origin))?;
            if !pull.status.success() {
                return Err(HivemindError::git(
                    "git_pull_failed",
                    String::from_utf8_lossy(&pull.stderr).to_string(),
                    origin,
                ));
            }
        }

        if merged_now {
            let new_target_head = std::process::Command::new("git")
                .current_dir(&repo_path)
                .args(["rev-parse", &target_branch])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();

            let mut commits = Vec::new();
            if let Some(old_head) = old_target_head {
                let rev_list = std::process::Command::new("git")
                    .current_dir(&repo_path)
                    .args([
                        "rev-list",
                        "--reverse",
                        &format!("{old_head}..{new_target_head}"),
                    ])
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
            } else if !new_target_head.is_empty() {
                commits.push(new_target_head);
            }

            self.append_event(
                Event::new(
                    EventPayload::MergeCompleted {
                        flow_id: flow.id,
                        commits,
                    },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                ),
                origin,
            )?;

            self.trigger_graph_snapshot_refresh(flow.project_id, "merge_completed", origin);
        }

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after PR execution",
                origin,
            )
        })
    }
}
