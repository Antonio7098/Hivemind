use super::*;

pub(super) struct TickRuntimeExecution {
    pub(super) runtime_for_adapter: ProjectRuntimeConfig,
    pub(super) report: crate::adapters::runtime::ExecutionReport,
}

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_runtime_for_tick_attempt(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        worktree_status: &WorktreeStatus,
        repo_worktrees: &[(String, WorktreeStatus)],
        mut runtime_for_adapter: ProjectRuntimeConfig,
        runtime_selection_source: RuntimeSelectionSource,
        task_scope: Option<Scope>,
        attempt_id: Uuid,
        attempt_corr: &CorrelationIds,
        next_attempt_number: u32,
        max_attempts: u32,
        mut runtime_flags: Vec<String>,
        runtime_prompt: String,
        origin: &'static str,
    ) -> Result<Option<ProjectRuntimeConfig>> {
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
        if let Some(scope) = task_scope {
            let scope_json = serde_json::to_string(&scope).map_err(|e| {
                HivemindError::system("scope_serialize_failed", e.to_string(), origin)
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
        for (repo_name, wt) in repo_worktrees {
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
                    origin,
                )
            })?;
            self.set_native_graph_query_runtime_env(project, &mut runtime_for_adapter.env, origin);
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
            match Self::prepare_runtime_environment(&mut runtime_for_adapter, origin) {
                Ok(provenance) => provenance,
                Err(err) => {
                    self.handle_runtime_failure(
                        state,
                        flow,
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
                        origin,
                    )?;
                    return Ok(None);
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
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        self.append_event(
            Event::new(
                EventPayload::RuntimeCapabilitiesEvaluated {
                    adapter_name: runtime_for_adapter.adapter_name.clone(),
                    role: RuntimeRole::Worker,
                    selection_source: runtime_selection_source,
                    capabilities: Self::runtime_capabilities_for_adapter(
                        &runtime_for_adapter.adapter_name,
                    ),
                },
                attempt_corr.clone(),
            ),
            origin,
        )?;

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
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        Ok(Some(runtime_for_adapter))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_tick_attempt(
        &self,
        interactive: bool,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        worktree_status: &WorktreeStatus,
        runtime_for_adapter: ProjectRuntimeConfig,
        input: ExecutionInput,
        attempt_id: Uuid,
        attempt_corr: &CorrelationIds,
        next_attempt_number: u32,
        max_attempts: u32,
        origin: &'static str,
    ) -> Result<Option<TickRuntimeExecution>> {
        let mut adapter = Self::build_runtime_adapter(runtime_for_adapter.clone())?;
        if let Err(e) = adapter.initialize() {
            self.handle_runtime_failure(
                state,
                flow,
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
                origin,
            )?;
            return Ok(None);
        }
        if let Err(e) = adapter.prepare(task_id, &worktree_status.path) {
            self.handle_runtime_failure(
                state,
                flow,
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
                origin,
            )?;
            return Ok(None);
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
                            attempt_corr,
                            runtime_projector.observe_chunk(RuntimeOutputStream::Stdout, &chunk),
                            origin,
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
                        state,
                        flow,
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
                        origin,
                    )?;
                    return Ok(None);
                }
            }
        } else {
            let report = match adapter.execute(input) {
                Ok(r) => r,
                Err(e) => {
                    self.handle_runtime_failure(
                        state,
                        flow,
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
                        origin,
                    )?;
                    return Ok(None);
                }
            };
            (report, None)
        };

        if let Some(native_invocation) = report.native_invocation.as_ref() {
            self.append_native_invocation_events(
                flow,
                task_id,
                attempt_id,
                attempt_corr,
                &runtime_for_adapter.adapter_name,
                native_invocation,
                origin,
            )?;
        }

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
                    HivemindError::system("event_append_failed", e.to_string(), origin)
                })?;

                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    attempt_corr,
                    runtime_projector
                        .observe_chunk(RuntimeOutputStream::Stdout, &format!("{content}\n")),
                    origin,
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
                    HivemindError::system("event_append_failed", e.to_string(), origin)
                })?;

                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    attempt_corr,
                    runtime_projector
                        .observe_chunk(RuntimeOutputStream::Stderr, &format!("{content}\n")),
                    origin,
                );
            }
        }

        let _ = self.append_projected_runtime_observations(
            attempt_id,
            attempt_corr,
            runtime_projector.flush(),
            origin,
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
            attempt_corr.clone(),
        );
        self.store
            .append(exited_event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        Ok(Some(TickRuntimeExecution {
            runtime_for_adapter,
            report,
        }))
    }
}
