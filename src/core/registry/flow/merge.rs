use super::*;

impl Registry {
    pub(crate) fn emit_merge_conflict(
        &self,
        flow: &TaskFlow,
        task_id: Option<Uuid>,
        details: String,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::MergeConflictDetected {
                    flow_id: flow.id,
                    task_id,
                    details,
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            origin,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub fn merge_prepare(
        &self,
        flow_id: &str,
        target_branch: Option<&str>,
    ) -> Result<crate::core::state::MergeState> {
        let origin = "registry:merge_prepare";
        let mut flow = self.get_flow(flow_id)?;

        if !matches!(flow.state, FlowState::Completed | FlowState::FrozenForMerge) {
            let err = HivemindError::user(
                "flow_not_completed",
                "Flow has not completed successfully",
                origin,
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            );
            return Err(err);
        }

        let _ = self.enforce_constitution_gate(
            flow.project_id,
            "merge_prepare",
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            origin,
        )?;

        let mut state = self.state()?;
        if let Some(ms) = state.merge_states.get(&flow.id) {
            if ms.status == crate::core::state::MergeStatus::Prepared && ms.conflicts.is_empty() {
                return Ok(ms.clone());
            }
        }

        let _integration_lock = self.acquire_flow_integration_lock(flow.id, origin)?;
        self.emit_integration_lock_acquired(&flow, "merge_prepare", origin)?;

        if flow.state == FlowState::Completed {
            self.append_event(
                Event::new(
                    EventPayload::FlowFrozenForMerge { flow_id: flow.id },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                ),
                origin,
            )?;
            flow = self.get_flow(flow_id)?;
            state = self.state()?;
        }

        let graph = state.graphs.get(&flow.graph_id).ok_or_else(|| {
            HivemindError::system(
                "graph_not_found",
                "Graph not found",
                "registry:merge_prepare",
            )
        })?;

        let mut conflicts = Vec::new();
        let mut integrated_tasks: Vec<(Uuid, Option<String>)> = Vec::new();
        let mut managers =
            Self::worktree_managers_for_flow(&flow, &state, "registry:merge_prepare")?;

        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                "Project not found",
                "registry:merge_prepare",
            )
        })?;
        if project.repositories.is_empty() {
            return Err(HivemindError::user(
                "project_has_no_repo",
                "Project has no attached repository",
                "registry:merge_prepare",
            ));
        }
        let (_primary_repo_name, manager) = managers.drain(..1).next().ok_or_else(|| {
            HivemindError::user(
                "project_has_no_repo",
                "Project has no attached repository",
                "registry:merge_prepare",
            )
        })?;

        let prepared_target_branch = {
            let repo_path = manager.repo_path();
            let current_branch = std::process::Command::new("git")
                .current_dir(repo_path)
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map_or_else(
                    || "HEAD".to_string(),
                    |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                );

            let main_exists = std::process::Command::new("git")
                .current_dir(repo_path)
                .args(["show-ref", "--verify", "--quiet", "refs/heads/main"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            let target = target_branch.map_or_else(
                || {
                    if main_exists {
                        "main".to_string()
                    } else {
                        current_branch
                    }
                },
                ToString::to_string,
            );
            if target == "HEAD" {
                return Err(HivemindError::user(
                    "detached_head",
                    "Cannot prepare merge from detached HEAD",
                    "registry:merge_prepare",
                )
                .with_hint("Re-run with --target <branch> or checkout a branch"));
            }
            let base_ref = target.as_str();

            let merge_branch = format!("integration/{}/prepare", flow.id);
            let merge_path = manager
                .config()
                .base_dir
                .join(flow.id.to_string())
                .join("_integration_prepare");

            if merge_path.exists() {
                let _ = std::process::Command::new("git")
                    .current_dir(manager.repo_path())
                    .args([
                        "worktree",
                        "remove",
                        "--force",
                        merge_path.to_str().unwrap_or(""),
                    ])
                    .output();
                let _ = fs::remove_dir_all(&merge_path);
            }

            if let Some(parent) = merge_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    HivemindError::system(
                        "create_dir_failed",
                        e.to_string(),
                        "registry:merge_prepare",
                    )
                })?;
            }

            let add = std::process::Command::new("git")
                .current_dir(manager.repo_path())
                .args([
                    "worktree",
                    "add",
                    "-B",
                    &merge_branch,
                    merge_path.to_str().unwrap_or(""),
                    base_ref,
                ])
                .output()
                .map_err(|e| {
                    HivemindError::system(
                        "git_worktree_add_failed",
                        e.to_string(),
                        "registry:merge_prepare",
                    )
                })?;
            if !add.status.success() {
                return Err(HivemindError::git(
                    "git_worktree_add_failed",
                    String::from_utf8_lossy(&add.stderr).to_string(),
                    "registry:merge_prepare",
                ));
            }

            for task_id in graph.topological_order() {
                if flow
                    .task_executions
                    .get(&task_id)
                    .is_none_or(|e| e.state != TaskExecState::Success)
                {
                    continue;
                }

                let task_branch = format!("exec/{}/{task_id}", flow.id);
                let task_ref = format!("refs/heads/{task_branch}");

                let ref_exists = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["show-ref", "--verify", "--quiet", &task_ref])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if !ref_exists {
                    let details = format!("task {task_id}: missing branch '{task_branch}'");
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                    break;
                }

                let _ = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", &merge_branch])
                    .output();

                if let Some(deps) = graph.dependencies.get(&task_id) {
                    for dep in deps {
                        let dep_branch = format!("exec/{}/{dep}", flow.id);
                        let Some(dep_sha) = Self::resolve_git_ref(&merge_path, &dep_branch) else {
                            let details = format!(
                                "task {task_id}: dependency branch missing for {dep_branch}"
                            );
                            conflicts.push(details.clone());
                            self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                            break;
                        };

                        let contains_dependency = std::process::Command::new("git")
                            .current_dir(&merge_path)
                            .args(["merge-base", "--is-ancestor", &dep_sha, &task_branch])
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false);
                        if !contains_dependency {
                            let details = format!(
                                "task {task_id}: drift detected (missing prerequisite integrated changes)"
                            );
                            conflicts.push(details.clone());
                            self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                            break;
                        }
                    }

                    if !conflicts.is_empty() {
                        break;
                    }
                }

                let sandbox_branch = format!("integration/{}/{task_id}", flow.id);
                let checkout = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", "-B", &sandbox_branch, &merge_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_checkout_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;
                if !checkout.status.success() {
                    return Err(HivemindError::git(
                        "git_checkout_failed",
                        String::from_utf8_lossy(&checkout.stderr).to_string(),
                        "registry:merge_prepare",
                    ));
                }

                let merge = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .env("GIT_AUTHOR_NAME", "Hivemind")
                    .env("GIT_AUTHOR_EMAIL", "hivemind@example.com")
                    .env("GIT_COMMITTER_NAME", "Hivemind")
                    .env("GIT_COMMITTER_EMAIL", "hivemind@example.com")
                    .args([
                        "-c",
                        "user.name=Hivemind",
                        "-c",
                        "user.email=hivemind@example.com",
                        "-c",
                        "commit.gpgsign=false",
                        "merge",
                        "--no-commit",
                        "--no-ff",
                        &task_branch,
                    ])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_merge_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;

                if !merge.status.success() {
                    let unmerged = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["diff", "--name-only", "--diff-filter=U"])
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_default();

                    let details = if unmerged.is_empty() {
                        String::from_utf8_lossy(&merge.stderr).to_string()
                    } else {
                        format!("conflicts in: {unmerged}")
                    };
                    conflicts.push(format!("task {task_id}: {details}"));
                    self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;

                    let _ = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["merge", "--abort"])
                        .output();
                    let _ = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["checkout", &merge_branch])
                        .output();
                    break;
                }

                let merge_in_progress = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !merge_in_progress {
                    continue;
                }

                let commit_msg = format!(
                    "Integrate task {task_id}\n\nFlow: {}\nTask: {task_id}\nTarget: {target}\nVerification-Summary: task_checks_passed\nTimestamp: {}\nHivemind-Version: {}",
                    flow.id,
                    Utc::now().to_rfc3339(),
                    env!("CARGO_PKG_VERSION")
                );
                let commit = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .env("GIT_AUTHOR_NAME", "Hivemind")
                    .env("GIT_AUTHOR_EMAIL", "hivemind@example.com")
                    .env("GIT_COMMITTER_NAME", "Hivemind")
                    .env("GIT_COMMITTER_EMAIL", "hivemind@example.com")
                    .args([
                        "-c",
                        "user.name=Hivemind",
                        "-c",
                        "user.email=hivemind@example.com",
                        "-c",
                        "commit.gpgsign=false",
                        "commit",
                        "-m",
                        &commit_msg,
                    ])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_commit_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;
                if !commit.status.success() {
                    return Err(HivemindError::git(
                        "git_commit_failed",
                        String::from_utf8_lossy(&commit.stderr).to_string(),
                        "registry:merge_prepare",
                    ));
                }

                let _ = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", &merge_branch])
                    .output();
                let promote = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["merge", "--ff-only", &sandbox_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_merge_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;
                if !promote.status.success() {
                    let details = String::from_utf8_lossy(&promote.stderr).trim().to_string();
                    conflicts.push(format!("task {task_id}: {details}"));
                    self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                    break;
                }

                let integrated_sha = Self::resolve_git_ref(&merge_path, "HEAD");
                integrated_tasks.push((task_id, integrated_sha));
            }

            if conflicts.is_empty() {
                let target_dir = self
                    .config
                    .data_dir
                    .join("cargo-target")
                    .join(flow.id.to_string())
                    .join("_integration_prepare")
                    .join("checks");
                let _ = fs::create_dir_all(&target_dir);

                let mut unique_checks: Vec<crate::core::verification::CheckConfig> = Vec::new();
                for task_id in graph.topological_order() {
                    if flow
                        .task_executions
                        .get(&task_id)
                        .is_none_or(|e| e.state != TaskExecState::Success)
                    {
                        continue;
                    }
                    if let Some(task) = graph.tasks.get(&task_id) {
                        for check in &task.criteria.checks {
                            if let Some(existing) = unique_checks
                                .iter_mut()
                                .find(|c| c.name == check.name && c.command == check.command)
                            {
                                existing.required = existing.required || check.required;
                                if existing.timeout_ms.is_none() {
                                    existing.timeout_ms = check.timeout_ms;
                                }
                            } else {
                                unique_checks.push(check.clone());
                            }
                        }
                    }
                }

                for check in &unique_checks {
                    self.append_event(
                        Event::new(
                            EventPayload::MergeCheckStarted {
                                flow_id: flow.id,
                                task_id: None,
                                check_name: check.name.clone(),
                                required: check.required,
                            },
                            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                        ),
                        "registry:merge_prepare",
                    )?;

                    let started = Instant::now();
                    let (exit_code, combined) = match Self::run_check_command(
                        &merge_path,
                        &target_dir,
                        &check.command,
                        check.timeout_ms,
                    ) {
                        Ok((exit_code, output, _timed_out)) => (exit_code, output),
                        Err(e) => (127, e.to_string()),
                    };
                    let duration_ms =
                        u64::try_from(started.elapsed().as_millis().min(u128::from(u64::MAX)))
                            .unwrap_or(u64::MAX);
                    let passed = exit_code == 0;

                    let safe_name = check
                        .name
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                        .collect::<String>();
                    let out_path = target_dir.join(format!("merge_check_{safe_name}.log"));
                    if let Err(e) = fs::write(&out_path, &combined) {
                        let details = format!(
                            "failed to write check output for {} to {}: {}",
                            check.name,
                            out_path.display(),
                            e
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, None, details, origin)?;
                        break;
                    }

                    self.append_event(
                        Event::new(
                            EventPayload::MergeCheckCompleted {
                                flow_id: flow.id,
                                task_id: None,
                                check_name: check.name.clone(),
                                passed,
                                exit_code,
                                output: combined.clone(),
                                duration_ms,
                                required: check.required,
                            },
                            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                        ),
                        "registry:merge_prepare",
                    )?;

                    if check.required && !passed {
                        let details = format!(
                            "required check failed: {} (exit={exit_code}, duration={}ms)",
                            check.name, duration_ms
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, None, details, origin)?;
                        if !combined.trim().is_empty() {
                            let snippet = combined.lines().take(10).collect::<Vec<_>>().join("\n");
                            conflicts.push(format!("check output (first lines): {snippet}"));
                        }
                        break;
                    }
                }
            }

            if conflicts.is_empty() {
                let flow_branch = format!("flow/{}", flow.id);
                let update = std::process::Command::new("git")
                    .current_dir(manager.repo_path())
                    .args(["branch", "-f", &flow_branch, &merge_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_branch_update_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;
                if !update.status.success() {
                    return Err(HivemindError::git(
                        "git_branch_update_failed",
                        String::from_utf8_lossy(&update.stderr).to_string(),
                        "registry:merge_prepare",
                    ));
                }

                for (task_id, commit_sha) in &integrated_tasks {
                    self.append_event(
                        Event::new(
                            EventPayload::TaskIntegratedIntoFlow {
                                flow_id: flow.id,
                                task_id: *task_id,
                                commit_sha: commit_sha.clone(),
                            },
                            CorrelationIds::for_graph_flow_task(
                                flow.project_id,
                                flow.graph_id,
                                flow.id,
                                *task_id,
                            ),
                        ),
                        origin,
                    )?;
                }
            }

            target
        };

        if conflicts.is_empty() {
            for (repo_name, manager) in managers {
                let merge_branch = format!("integration/{}/prepare", flow.id);
                let merge_path = manager
                    .config()
                    .base_dir
                    .join(flow.id.to_string())
                    .join("_integration_prepare");

                if merge_path.exists() {
                    let _ = std::process::Command::new("git")
                        .current_dir(manager.repo_path())
                        .args([
                            "worktree",
                            "remove",
                            "--force",
                            merge_path.to_str().unwrap_or(""),
                        ])
                        .output();
                    let _ = fs::remove_dir_all(&merge_path);
                }

                if let Some(parent) = merge_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        HivemindError::system("create_dir_failed", e.to_string(), origin)
                    })?;
                }

                let add = std::process::Command::new("git")
                    .current_dir(manager.repo_path())
                    .args([
                        "worktree",
                        "add",
                        "-B",
                        &merge_branch,
                        merge_path.to_str().unwrap_or(""),
                        &prepared_target_branch,
                    ])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_worktree_add_failed", e.to_string(), origin)
                    })?;
                if !add.status.success() {
                    let details = format!(
                        "repo {repo_name}: {}",
                        String::from_utf8_lossy(&add.stderr).trim()
                    );
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(&flow, None, details, origin)?;
                    continue;
                }

                for task_id in graph.topological_order() {
                    if flow
                        .task_executions
                        .get(&task_id)
                        .is_none_or(|e| e.state != TaskExecState::Success)
                    {
                        continue;
                    }
                    let task_branch = format!("exec/{}/{task_id}", flow.id);
                    let task_ref = format!("refs/heads/{task_branch}");
                    if !Self::git_ref_exists(&merge_path, &task_ref) {
                        let details = format!("repo {repo_name}: task {task_id}: missing branch");
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        break;
                    }

                    let sandbox_branch = format!("integration/{}/{task_id}", flow.id);
                    let checkout = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["checkout", "-B", &sandbox_branch, &merge_branch])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_checkout_failed", e.to_string(), origin)
                        })?;
                    if !checkout.status.success() {
                        let details = format!(
                            "repo {repo_name}: task {task_id}: {}",
                            String::from_utf8_lossy(&checkout.stderr).trim()
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        break;
                    }

                    let merge = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args([
                            "-c",
                            "user.name=Hivemind",
                            "-c",
                            "user.email=hivemind@example.com",
                            "-c",
                            "commit.gpgsign=false",
                            "merge",
                            "--no-commit",
                            "--no-ff",
                            &task_branch,
                        ])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_merge_failed", e.to_string(), origin)
                        })?;
                    if !merge.status.success() {
                        let details = format!(
                            "repo {repo_name}: task {task_id}: {}",
                            String::from_utf8_lossy(&merge.stderr).trim()
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        let _ = std::process::Command::new("git")
                            .current_dir(&merge_path)
                            .args(["merge", "--abort"])
                            .output();
                        break;
                    }

                    let merge_in_progress = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if !merge_in_progress {
                        let _ = std::process::Command::new("git")
                            .current_dir(&merge_path)
                            .args(["checkout", &merge_branch])
                            .output();
                        continue;
                    }

                    let commit_msg = format!(
                        "Integrate task {task_id}\n\nFlow: {}\nTask: {task_id}\nTarget: {}\nRepository: {}\nTimestamp: {}\nHivemind-Version: {}",
                        flow.id,
                        prepared_target_branch,
                        repo_name,
                        Utc::now().to_rfc3339(),
                        env!("CARGO_PKG_VERSION")
                    );
                    let commit = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args([
                            "-c",
                            "user.name=Hivemind",
                            "-c",
                            "user.email=hivemind@example.com",
                            "-c",
                            "commit.gpgsign=false",
                            "commit",
                            "-m",
                            &commit_msg,
                        ])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_commit_failed", e.to_string(), origin)
                        })?;
                    if !commit.status.success() {
                        let details = format!(
                            "repo {repo_name}: task {task_id}: {}",
                            String::from_utf8_lossy(&commit.stderr).trim()
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        break;
                    }

                    let _ = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["checkout", &merge_branch])
                        .output();
                    let promote = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["merge", "--ff-only", &sandbox_branch])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_merge_failed", e.to_string(), origin)
                        })?;
                    if !promote.status.success() {
                        let details = format!(
                            "repo {repo_name}: task {task_id}: {}",
                            String::from_utf8_lossy(&promote.stderr).trim()
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        break;
                    }
                }

                if conflicts.is_empty() {
                    let flow_branch = format!("flow/{}", flow.id);
                    let _ = std::process::Command::new("git")
                        .current_dir(manager.repo_path())
                        .args(["branch", "-f", &flow_branch, &merge_branch])
                        .output();
                }
            }
        }

        let event = Event::new(
            EventPayload::MergePrepared {
                flow_id: flow.id,
                target_branch: Some(prepared_target_branch),
                conflicts,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:merge_prepare",
            )
        })?;

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after prepare",
                "registry:merge_prepare",
            )
        })
    }

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
