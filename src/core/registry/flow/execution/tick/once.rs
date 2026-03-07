use super::*;

impl Registry {
    pub(crate) fn tick_flow_once(
        &self,
        flow_id: &str,
        interactive: bool,
        preferred_task: Option<Uuid>,
    ) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Running {
            return Err(HivemindError::user(
                "flow_not_running",
                "Flow is not in running state",
                "registry:tick_flow",
            ));
        }

        let state = self.state()?;
        let graph = state.graphs.get(&flow.graph_id).ok_or_else(|| {
            HivemindError::system("graph_not_found", "Graph not found", "registry:tick_flow")
        })?;

        let mut verifying = flow.tasks_in_state(TaskExecState::Verifying);
        verifying.sort();
        if let Some(task_id) = verifying.first().copied() {
            return self.process_verifying_task(flow_id, task_id);
        }

        let mut newly_ready = Vec::new();
        let mut newly_blocked: Vec<(Uuid, String)> = Vec::new();
        for task_id in graph.tasks.keys() {
            let Some(exec) = flow.task_executions.get(task_id) else {
                continue;
            };
            if exec.state != TaskExecState::Pending {
                continue;
            }

            let deps_satisfied = graph.dependencies.get(task_id).is_none_or(|deps| {
                deps.iter().all(|dep| {
                    flow.task_executions
                        .get(dep)
                        .is_some_and(|e| e.state == TaskExecState::Success)
                })
            });

            if deps_satisfied {
                newly_ready.push(*task_id);
            } else {
                let mut missing: Vec<Uuid> = graph
                    .dependencies
                    .get(task_id)
                    .map(|deps| {
                        deps.iter()
                            .filter(|dep| {
                                flow.task_executions
                                    .get(dep)
                                    .is_none_or(|e| e.state != TaskExecState::Success)
                            })
                            .copied()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                missing.sort();

                let preview = missing
                    .iter()
                    .take(5)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let reason = if missing.len() <= 5 {
                    format!("Waiting on dependencies: {preview}")
                } else {
                    format!(
                        "Waiting on dependencies: {preview} (+{} more)",
                        missing.len().saturating_sub(5)
                    )
                };

                if exec.blocked_reason.as_deref() != Some(reason.as_str()) {
                    newly_blocked.push((*task_id, reason));
                }
            }
        }

        for (task_id, reason) in newly_blocked {
            let event = Event::new(
                EventPayload::TaskBlocked {
                    flow_id: flow.id,
                    task_id,
                    reason: Some(reason),
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
            })?;
        }

        for task_id in newly_ready {
            let event = Event::new(
                EventPayload::TaskReady {
                    flow_id: flow.id,
                    task_id,
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
            })?;
        }

        let flow = self.get_flow(flow_id)?;
        let mut retrying = flow.tasks_in_state(TaskExecState::Retry);
        retrying.sort();
        let mut ready = flow.tasks_in_state(TaskExecState::Ready);
        ready.sort();

        let preferred = preferred_task.filter(|task_id| {
            (retrying.contains(task_id) || ready.contains(task_id))
                && Self::can_auto_run_task(&state, *task_id)
        });
        let task_to_run = preferred.or_else(|| {
            retrying
                .iter()
                .chain(ready.iter())
                .find(|task_id| Self::can_auto_run_task(&state, **task_id))
                .copied()
        });

        let Some(task_id) = task_to_run else {
            let all_success = flow
                .task_executions
                .values()
                .all(|e| e.state == TaskExecState::Success);
            if all_success {
                let event = Event::new(
                    EventPayload::TaskFlowCompleted { flow_id: flow.id },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:tick_flow",
                    )
                })?;
                self.maybe_autostart_dependent_flows(flow.id)?;
                return self.get_flow(flow_id);
            }

            return Ok(flow);
        };

        let (runtime, runtime_selection_source) = Self::effective_runtime_for_task_with_source(
            &state,
            &flow,
            task_id,
            RuntimeRole::Worker,
            "registry:tick_flow",
        )?;

        let worktree_status =
            Self::ensure_task_worktree(&flow, &state, task_id, "registry:tick_flow")?;
        let repo_worktrees =
            Self::inspect_task_worktrees(&flow, &state, task_id, "registry:tick_flow")?;

        let exec = flow.task_executions.get(&task_id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:tick_flow",
            )
        })?;

        if exec.state == TaskExecState::Retry && exec.retry_mode == RetryMode::Clean {
            let branch = format!("exec/{}/{task_id}", flow.id);
            for (idx, (_repo_name, repo_worktree)) in repo_worktrees.iter().enumerate() {
                let managers =
                    Self::worktree_managers_for_flow(&flow, &state, "registry:tick_flow")?;
                let (_, manager) = &managers[idx];
                let base = Self::default_base_ref_for_repo(&flow, manager, idx == 0);
                Self::checkout_and_clean_worktree(
                    &repo_worktree.path,
                    &branch,
                    &base,
                    "registry:tick_flow",
                )?;
            }
        }

        let next_attempt_number = exec.attempt_count.saturating_add(1);

        // Ensure this worktree contains the latest changes from dependency tasks.
        // Each task runs in its own worktree/branch (`exec/<flow>/<task>`), so dependent
        // tasks must merge dependency branch heads to see upstream work.
        if let Some(deps) = graph.dependencies.get(&task_id) {
            let mut dep_ids: Vec<Uuid> = deps.iter().copied().collect();
            dep_ids.sort();

            for (_repo_name, repo_worktree) in &repo_worktrees {
                for &dep_task_id in &dep_ids {
                    let dep_branch = format!("exec/{}/{dep_task_id}", flow.id);
                    let dep_ref = format!("refs/heads/{dep_branch}");

                    let ref_exists = std::process::Command::new("git")
                        .current_dir(&repo_worktree.path)
                        .args(["show-ref", "--verify", "--quiet", &dep_ref])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);

                    if !ref_exists {
                        continue;
                    }

                    let already_contains = std::process::Command::new("git")
                        .current_dir(&repo_worktree.path)
                        .args(["merge-base", "--is-ancestor", &dep_branch, "HEAD"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);

                    if already_contains {
                        continue;
                    }

                    let merge = std::process::Command::new("git")
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
                            HivemindError::system(
                                "git_merge_failed",
                                e.to_string(),
                                "registry:tick_flow",
                            )
                        })?;

                    if !merge.status.success() {
                        let _ = std::process::Command::new("git")
                            .current_dir(&repo_worktree.path)
                            .args(["merge", "--abort"])
                            .output();
                        return Err(HivemindError::git(
                            "merge_failed",
                            String::from_utf8_lossy(&merge.stderr).to_string(),
                            "registry:tick_flow",
                        ));
                    }
                }
            }
        }

        // Use the canonical task lifecycle so we persist baselines, compute diffs, and create
        // checkpoint commits. This is required for dependency propagation.
        let attempt_id = self.start_task_execution(&task_id.to_string())?;
        let attempt_corr = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        let task = graph.tasks.get(&task_id).ok_or_else(|| {
            HivemindError::system(
                "task_not_found",
                "Task not found in graph",
                "registry:tick_flow",
            )
        })?;

        let max_attempts = task.retry_policy.max_retries.saturating_add(1);
        let checkpoint_ids = Self::normalized_checkpoint_ids(&task.checkpoints);

        let checkpoint_help = if checkpoint_ids.is_empty() {
            None
        } else {
            Some(format!(
                    "Execution checkpoints (in order): {}\nComplete checkpoints from the runtime using: \"$HIVEMIND_BIN\" checkpoint complete --id <checkpoint-id> [--summary \"...\"]\n(If available, \"$HIVEMIND_AGENT_BIN\" may be used equivalently.)\nAttempt ID for this run: {attempt_id}",
                    checkpoint_ids.join(", ")
                ))
        };
        let repo_context = format!(
            "Multi-repo worktrees for this attempt:\n{}",
            repo_worktrees
                .iter()
                .map(|(name, wt)| format!("- {name}: {}", wt.path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let mut retry_prior_attempt_ids: Vec<Uuid> = Vec::new();
        let (retry_context_text, prior_attempts) = if next_attempt_number > 1 {
            let (ctx, priors, ids, req, opt, ec, term) = self.build_retry_context(
                &state,
                &flow,
                task_id,
                next_attempt_number,
                max_attempts,
                "registry:tick_flow",
            )?;
            retry_prior_attempt_ids.clone_from(&ids);

            self.append_event(
                Event::new(
                    EventPayload::RetryContextAssembled {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        attempt_number: next_attempt_number,
                        max_attempts,
                        prior_attempt_ids: ids,
                        required_check_failures: req,
                        optional_check_failures: opt,
                        runtime_exit_code: ec,
                        runtime_terminated_reason: term,
                        context: ctx.clone(),
                    },
                    attempt_corr.clone(),
                ),
                "registry:tick_flow",
            )?;

            (Some(ctx), priors)
        } else {
            (None, Vec::new())
        };

        let context_build = self.assemble_attempt_context(
            &state,
            &flow,
            task_id,
            attempt_id,
            &runtime.adapter_name,
            &retry_prior_attempt_ids,
            "registry:tick_flow",
        )?;

        if !context_build.included_document_ids.is_empty()
            || !context_build.excluded_document_ids.is_empty()
        {
            self.append_event(
                Event::new(
                    EventPayload::AttemptContextOverridesApplied {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        template_document_ids: context_build.template_document_ids.clone(),
                        included_document_ids: context_build.included_document_ids.clone(),
                        excluded_document_ids: context_build.excluded_document_ids.clone(),
                        resolved_document_ids: context_build.resolved_document_ids.clone(),
                    },
                    attempt_corr.clone(),
                ),
                "registry:tick_flow",
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::ContextWindowCreated {
                    flow_id: flow.id,
                    task_id,
                    attempt_id,
                    window_id: context_build.context_window_id.clone(),
                    policy: ATTEMPT_CONTEXT_TRUNCATION_POLICY.to_string(),
                    state_hash: context_build.context_window_state_hash.clone(),
                },
                attempt_corr.clone(),
            ),
            "registry:tick_flow",
        )?;

        for op in &context_build.context_window_ops {
            self.append_event(
                Event::new(
                    EventPayload::ContextOpApplied {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        window_id: context_build.context_window_id.clone(),
                        op: op.op.clone(),
                        actor: op.actor.actor.clone(),
                        runtime: op.actor.runtime.clone(),
                        tool: op.actor.tool.clone(),
                        reason: op.reason.clone(),
                        before_hash: op.before_hash.clone(),
                        after_hash: op.after_hash.clone(),
                        added_ids: op.added_ids.clone(),
                        removed_ids: op.removed_ids.clone(),
                        truncated_sections: op.truncated_sections.clone(),
                        section_reasons: op.section_reasons.clone(),
                    },
                    attempt_corr.clone(),
                ),
                "registry:tick_flow",
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::AttemptContextAssembled {
                    flow_id: flow.id,
                    task_id,
                    attempt_id,
                    attempt_number: next_attempt_number,
                    manifest_hash: context_build.manifest_hash.clone(),
                    inputs_hash: context_build.inputs_hash.clone(),
                    context_hash: context_build.context_hash.clone(),
                    context_size_bytes: context_build.context_size_bytes,
                    truncated_sections: context_build.truncated_sections.clone(),
                    manifest_json: context_build.manifest_json.clone(),
                },
                attempt_corr.clone(),
            ),
            "registry:tick_flow",
        )?;

        if !context_build.truncated_sections.is_empty() {
            self.append_event(
                Event::new(
                    EventPayload::AttemptContextTruncated {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        budget_bytes: ATTEMPT_CONTEXT_TOTAL_BUDGET_BYTES,
                        original_size_bytes: context_build.original_size_bytes,
                        truncated_size_bytes: context_build.context_size_bytes,
                        sections: context_build.truncated_sections.clone(),
                        section_reasons: context_build.truncation_reasons.clone(),
                        policy: ATTEMPT_CONTEXT_TRUNCATION_POLICY.to_string(),
                    },
                    attempt_corr.clone(),
                ),
                "registry:tick_flow",
            )?;
        }

        let execution_context_base = format!("{repo_context}\n\n{}", context_build.context);
        let runtime_context = match (&retry_context_text, &checkpoint_help) {
            (Some(retry), Some(checkpoint_text)) => {
                format!("{retry}\n\n{execution_context_base}\n\n{checkpoint_text}")
            }
            (Some(retry), None) => format!("{retry}\n\n{execution_context_base}"),
            (None, Some(checkpoint_text)) => {
                format!("{execution_context_base}\n\n{checkpoint_text}")
            }
            (None, None) => execution_context_base,
        };
        let runtime_context_hash = Self::constitution_digest(runtime_context.as_bytes());

        self.append_event(
            Event::new(
                EventPayload::ContextWindowSnapshotCreated {
                    flow_id: flow.id,
                    task_id,
                    attempt_id,
                    window_id: context_build.context_window_id.clone(),
                    state_hash: context_build.context_window_state_hash.clone(),
                    rendered_prompt_hash: context_build.context_hash.clone(),
                    delivered_input_hash: runtime_context_hash.clone(),
                    snapshot_json: context_build.context_window_snapshot_json.clone(),
                },
                attempt_corr.clone(),
            ),
            "registry:tick_flow",
        )?;

        self.append_event(
            Event::new(
                EventPayload::AttemptContextDelivered {
                    flow_id: flow.id,
                    task_id,
                    attempt_id,
                    manifest_hash: context_build.manifest_hash,
                    inputs_hash: context_build.inputs_hash,
                    context_hash: runtime_context_hash,
                    delivery_target: "runtime_execution_input".to_string(),
                    prior_attempt_ids: retry_prior_attempt_ids,
                    prior_manifest_hashes: context_build.prior_manifest_hashes,
                },
                attempt_corr.clone(),
            ),
            "registry:tick_flow",
        )?;

        self.append_event(
            Event::new(
                EventPayload::RuntimeCapabilitiesEvaluated {
                    adapter_name: runtime.adapter_name.clone(),
                    role: RuntimeRole::Worker,
                    selection_source: runtime_selection_source,
                    capabilities: Self::runtime_capabilities_for_adapter(&runtime.adapter_name),
                },
                attempt_corr.clone(),
            ),
            "registry:tick_flow",
        )?;

        let input = ExecutionInput {
            task_description: task
                .description
                .clone()
                .unwrap_or_else(|| task.title.clone()),
            success_criteria: task.criteria.description.clone(),
            context: Some(runtime_context),
            prior_attempts,
            verifier_feedback: None,
        };
        let runtime_prompt = format_execution_prompt(&input);
        let mut runtime_flags = Self::runtime_start_flags(&runtime);

        let mut runtime_for_adapter = runtime;

        let target_dir = self
            .config
            .data_dir
            .join("cargo-target")
            .join(flow.id.to_string())
            .join(task_id.to_string())
            .join(attempt_id.to_string());
        let _ = fs::create_dir_all(&target_dir);
        runtime_for_adapter
            .env
            .entry("CARGO_TARGET_DIR".to_string())
            .or_insert_with(|| target_dir.to_string_lossy().to_string());
        runtime_for_adapter
            .env
            .insert("HIVEMIND_ATTEMPT_ID".to_string(), attempt_id.to_string());
        if let Some(scope) = &task.scope {
            let scope_json = serde_json::to_string(scope).map_err(|e| {
                HivemindError::system(
                    "scope_serialize_failed",
                    e.to_string(),
                    "registry:tick_flow",
                )
            })?;
            runtime_for_adapter
                .env
                .insert("HIVEMIND_TASK_SCOPE_JSON".to_string(), scope_json);

            let trace_path = self.scope_trace_path(attempt_id);
            let _ = fs::create_dir_all(self.scope_traces_dir());
            runtime_for_adapter.env.insert(
                "HIVEMIND_SCOPE_TRACE_LOG".to_string(),
                trace_path.to_string_lossy().to_string(),
            );
        }
        runtime_for_adapter
            .env
            .insert("HIVEMIND_TASK_ID".to_string(), task_id.to_string());
        runtime_for_adapter
            .env
            .insert("HIVEMIND_FLOW_ID".to_string(), flow.id.to_string());
        runtime_for_adapter.env.insert(
            "HIVEMIND_PRIMARY_WORKTREE".to_string(),
            worktree_status.path.to_string_lossy().to_string(),
        );
        runtime_for_adapter.env.insert(
            "HIVEMIND_ALL_WORKTREES".to_string(),
            repo_worktrees
                .iter()
                .map(|(name, wt)| format!("{name}={}", wt.path.display()))
                .collect::<Vec<_>>()
                .join(";"),
        );
        for (repo_name, wt) in &repo_worktrees {
            let env_key = format!(
                "HIVEMIND_REPO_{}_WORKTREE",
                repo_name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() {
                        c.to_ascii_uppercase()
                    } else {
                        '_'
                    })
                    .collect::<String>()
            );
            runtime_for_adapter
                .env
                .insert(env_key, wt.path.to_string_lossy().to_string());
        }
        if runtime_for_adapter.adapter_name == "native" {
            runtime_for_adapter
                .env
                .entry("HIVEMIND_NATIVE_STATE_DIR".to_string())
                .or_insert_with(|| {
                    let native_state_dir = self.config.data_dir.join("native-runtime");
                    let _ = fs::create_dir_all(&native_state_dir);
                    native_state_dir.to_string_lossy().to_string()
                });
            let project = state.projects.get(&flow.project_id).ok_or_else(|| {
                HivemindError::system(
                    "project_not_found",
                    "Project missing while preparing native runtime graph query context",
                    "registry:tick_flow",
                )
            })?;
            self.set_native_graph_query_runtime_env(
                project,
                &mut runtime_for_adapter.env,
                "registry:tick_flow",
            );
        }
        if let Ok(bin) = std::env::current_exe() {
            let hivemind_bin = bin.to_string_lossy().to_string();
            runtime_for_adapter
                .env
                .insert("HIVEMIND_BIN".to_string(), hivemind_bin);

            let agent_path = bin
                .parent()
                .map(|p| p.join("hivemind-agent"))
                .filter(|p| p.exists())
                .unwrap_or(bin);
            runtime_for_adapter.env.insert(
                "HIVEMIND_AGENT_BIN".to_string(),
                agent_path.to_string_lossy().to_string(),
            );
        }

        let env_provenance =
            match Self::prepare_runtime_environment(&mut runtime_for_adapter, "registry:tick_flow")
            {
                Ok(provenance) => provenance,
                Err(err) => {
                    self.handle_runtime_failure(
                        &state,
                        &flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &err.code,
                        &err.message,
                        err.recoverable,
                        "",
                        "",
                        "registry:tick_flow",
                    )?;
                    return self.get_flow(flow_id);
                }
            };

        runtime_flags.push(format!(
            "env_inherit_mode={}",
            env_provenance.inherit_mode.as_str()
        ));
        runtime_flags.push(format!(
            "env_inherited={}",
            env_provenance.inherited_keys.len()
        ));
        runtime_flags.push(format!("env_overlay={}", env_provenance.overlay_keys.len()));
        runtime_flags.push(format!(
            "env_dropped_sensitive_inherited={}",
            env_provenance.dropped_sensitive_inherited_keys.len()
        ));
        runtime_flags.push(format!(
            "env_dropped_reserved_inherited={}",
            env_provenance.dropped_reserved_inherited_keys.len()
        ));
        let RuntimeEnvironmentProvenance {
            inherit_mode,
            inherited_keys,
            overlay_keys,
            explicit_sensitive_overlay_keys,
            dropped_sensitive_inherited_keys,
            dropped_reserved_inherited_keys,
        } = env_provenance;

        self.store
            .append(Event::new(
                EventPayload::RuntimeEnvironmentPrepared {
                    attempt_id,
                    adapter_name: runtime_for_adapter.adapter_name.clone(),
                    inherit_mode: inherit_mode.as_str().to_string(),
                    inherited_keys,
                    overlay_keys,
                    explicit_sensitive_overlay_keys,
                    dropped_sensitive_inherited_keys,
                    dropped_reserved_inherited_keys,
                },
                attempt_corr.clone(),
            ))
            .map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
            })?;

        self.store
            .append(Event::new(
                EventPayload::RuntimeStarted {
                    adapter_name: runtime_for_adapter.adapter_name.clone(),
                    role: RuntimeRole::Worker,
                    task_id,
                    attempt_id,
                    prompt: runtime_prompt,
                    flags: runtime_flags,
                },
                attempt_corr.clone(),
            ))
            .map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
            })?;

        let mut adapter = Self::build_runtime_adapter(runtime_for_adapter.clone())?;
        if let Err(e) = adapter.initialize() {
            self.handle_runtime_failure(
                &state,
                &flow,
                task_id,
                attempt_id,
                &runtime_for_adapter,
                next_attempt_number,
                max_attempts,
                &e.code,
                &e.message,
                e.recoverable,
                "",
                "",
                "registry:tick_flow",
            )?;
            return self.get_flow(flow_id);
        }
        if let Err(e) = adapter.prepare(task_id, &worktree_status.path) {
            self.handle_runtime_failure(
                &state,
                &flow,
                task_id,
                attempt_id,
                &runtime_for_adapter,
                next_attempt_number,
                max_attempts,
                &e.code,
                &e.message,
                e.recoverable,
                "",
                "",
                "registry:tick_flow",
            )?;
            return self.get_flow(flow_id);
        }

        let mut runtime_projector = RuntimeEventProjector::new();

        let (report, terminated_reason) = if interactive {
            let mut stdout = std::io::stdout();

            let res = adapter.execute_interactive(&input, |evt| {
                match evt {
                    InteractiveAdapterEvent::Output { content } => {
                        let chunk = content;
                        let _ = stdout.write_all(chunk.as_bytes());
                        let _ = stdout.flush();
                        let event = Event::new(
                            EventPayload::RuntimeOutputChunk {
                                attempt_id,
                                stream: RuntimeOutputStream::Stdout,
                                content: chunk.clone(),
                            },
                            attempt_corr.clone(),
                        );
                        self.store.append(event).map_err(|e| e.to_string())?;
                        let _ = self.append_projected_runtime_observations(
                            attempt_id,
                            &attempt_corr,
                            runtime_projector.observe_chunk(RuntimeOutputStream::Stdout, &chunk),
                            "registry:tick_flow",
                        );
                    }
                    InteractiveAdapterEvent::Input { content } => {
                        let event = Event::new(
                            EventPayload::RuntimeInputProvided {
                                attempt_id,
                                content,
                            },
                            attempt_corr.clone(),
                        );
                        self.store.append(event).map_err(|e| e.to_string())?;
                    }
                    InteractiveAdapterEvent::Interrupted => {
                        let event = Event::new(
                            EventPayload::RuntimeInterrupted { attempt_id },
                            attempt_corr.clone(),
                        );
                        self.store.append(event).map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            });

            match res {
                Ok(r) => (r.report, r.terminated_reason),
                Err(e) => {
                    self.handle_runtime_failure(
                        &state,
                        &flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &e.code,
                        &e.message,
                        e.recoverable,
                        "",
                        "",
                        "registry:tick_flow",
                    )?;
                    return self.get_flow(flow_id);
                }
            }
        } else {
            let report = match adapter.execute(input) {
                Ok(r) => r,
                Err(e) => {
                    self.handle_runtime_failure(
                        &state,
                        &flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &e.code,
                        &e.message,
                        e.recoverable,
                        "",
                        "",
                        "registry:tick_flow",
                    )?;
                    return self.get_flow(flow_id);
                }
            };
            (report, None)
        };

        if let Some(native_invocation) = report.native_invocation.as_ref() {
            self.append_native_invocation_events(
                &flow,
                task_id,
                attempt_id,
                &attempt_corr,
                &runtime_for_adapter.adapter_name,
                native_invocation,
                "registry:tick_flow",
            )?;
        }

        // Best-effort filesystem observed based on the persisted baseline.
        if let Ok(state) = self.state() {
            if let Some(attempt) = state.attempts.get(&attempt_id) {
                if let Some(baseline_id) = attempt.baseline_id {
                    if let Ok(baseline) = self.read_baseline_artifact(baseline_id) {
                        if let Ok(diff) = Diff::compute(&baseline, &worktree_status.path) {
                            let created = diff
                                .changes
                                .iter()
                                .filter(|c| c.change_type == ChangeType::Created)
                                .map(|c| c.path.clone())
                                .collect();
                            let modified = diff
                                .changes
                                .iter()
                                .filter(|c| c.change_type == ChangeType::Modified)
                                .map(|c| c.path.clone())
                                .collect();
                            let deleted = diff
                                .changes
                                .iter()
                                .filter(|c| c.change_type == ChangeType::Deleted)
                                .map(|c| c.path.clone())
                                .collect();

                            let fs_event = Event::new(
                                EventPayload::RuntimeFilesystemObserved {
                                    attempt_id,
                                    files_created: created,
                                    files_modified: modified,
                                    files_deleted: deleted,
                                },
                                attempt_corr.clone(),
                            );
                            let _ = self.store.append(fs_event);
                        }
                    }
                }
            }
        }

        if !interactive {
            for chunk in report.stdout.lines() {
                let content = chunk.to_string();
                let event = Event::new(
                    EventPayload::RuntimeOutputChunk {
                        attempt_id,
                        stream: RuntimeOutputStream::Stdout,
                        content: content.clone(),
                    },
                    attempt_corr.clone(),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:tick_flow",
                    )
                })?;

                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    &attempt_corr,
                    runtime_projector
                        .observe_chunk(RuntimeOutputStream::Stdout, &format!("{content}\n")),
                    "registry:tick_flow",
                );
            }
            for chunk in report.stderr.lines() {
                let content = chunk.to_string();
                let event = Event::new(
                    EventPayload::RuntimeOutputChunk {
                        attempt_id,
                        stream: RuntimeOutputStream::Stderr,
                        content: content.clone(),
                    },
                    attempt_corr.clone(),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:tick_flow",
                    )
                })?;

                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    &attempt_corr,
                    runtime_projector
                        .observe_chunk(RuntimeOutputStream::Stderr, &format!("{content}\n")),
                    "registry:tick_flow",
                );
            }
        }

        let _ = self.append_projected_runtime_observations(
            attempt_id,
            &attempt_corr,
            runtime_projector.flush(),
            "registry:tick_flow",
        );

        if let Some(reason) = terminated_reason {
            let _ = self.store.append(Event::new(
                EventPayload::RuntimeTerminated { attempt_id, reason },
                attempt_corr.clone(),
            ));
        }

        let duration_ms = u64::try_from(report.duration.as_millis().min(u128::from(u64::MAX)))
            .unwrap_or(u64::MAX);
        let exited_event = Event::new(
            EventPayload::RuntimeExited {
                attempt_id,
                exit_code: report.exit_code,
                duration_ms,
            },
            attempt_corr,
        );
        self.store.append(exited_event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
        })?;

        if report.exit_code != 0 {
            let (failure_code, failure_message, recoverable) = report
                .errors
                .first()
                .filter(|error| error.code.starts_with("native_"))
                .map_or_else(
                    || {
                        (
                            "runtime_nonzero_exit".to_string(),
                            format!("Runtime exited with code {}", report.exit_code),
                            true,
                        )
                    },
                    |error| (error.code.clone(), error.message.clone(), error.recoverable),
                );
            self.handle_runtime_failure(
                &state,
                &flow,
                task_id,
                attempt_id,
                &runtime_for_adapter,
                next_attempt_number,
                max_attempts,
                &failure_code,
                &failure_message,
                recoverable,
                &report.stdout,
                &report.stderr,
                "registry:tick_flow",
            )?;
            return self.get_flow(flow_id);
        }

        if let Err(err) = self.complete_task_execution(&task_id.to_string()) {
            if err.code == "checkpoints_incomplete" {
                if let Some((failure_code, failure_message, recoverable)) =
                    Self::detect_runtime_output_failure(&report.stdout, &report.stderr)
                {
                    self.handle_runtime_failure(
                        &state,
                        &flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &failure_code,
                        &failure_message,
                        recoverable,
                        &report.stdout,
                        &report.stderr,
                        "registry:tick_flow",
                    )?;
                    return self.get_flow(flow_id);
                }

                self.handle_runtime_failure(
                    &state,
                    &flow,
                    task_id,
                    attempt_id,
                    &runtime_for_adapter,
                    next_attempt_number,
                    max_attempts,
                    "checkpoints_incomplete",
                    &err.message,
                    true,
                    &report.stdout,
                    &report.stderr,
                    "registry:tick_flow",
                )?;
                return Err(err);
            }
            return Err(err);
        }

        self.process_verifying_task(flow_id, task_id)
    }
}
