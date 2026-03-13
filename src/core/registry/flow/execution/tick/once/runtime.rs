#![allow(clippy::too_many_lines)]

use super::*;
use crate::adapters::runtime::StructuredRuntimeObservation;

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
        attach_resume_session_if_supported(
            state,
            flow,
            task_id,
            attempt_id,
            &mut runtime_for_adapter,
        );
        runtime_for_adapter.env.insert(
            "HIVEMIND_DATA_DIR".to_string(),
            self.config.data_dir.to_string_lossy().to_string(),
        );
        runtime_for_adapter.env.insert(
            "HIVEMIND_DATA_DIR".to_string(),
            self.config.data_dir.to_string_lossy().to_string(),
        );
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
        let stream_live_output =
            interactive || matches!(&adapter, SelectedRuntimeAdapter::Native(_));

        let (report, terminated_reason) = if stream_live_output {
            let mut stdout = if interactive {
                Some(std::io::stdout())
            } else {
                None
            };
            let res = adapter.execute_interactive(&input, |evt| {
                match evt {
                    InteractiveAdapterEvent::Output { content } => {
                        let chunk = content;
                        if let Some(stdout) = stdout.as_mut() {
                            let _ = stdout.write_all(chunk.as_bytes());
                            let _ = stdout.flush();
                        }
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
                    InteractiveAdapterEvent::FilesystemObserved {
                        files_created,
                        files_modified,
                        files_deleted,
                    } => {
                        let event = Event::new(
                            EventPayload::RuntimeFilesystemObserved {
                                attempt_id,
                                files_created,
                                files_modified,
                                files_deleted,
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
        let has_structured_command_events = !report.structured_runtime_observations.is_empty();

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
                            let dirty_paths = diff
                                .changes
                                .iter()
                                .map(|change| change.path.clone())
                                .collect::<Vec<_>>();
                            if !dirty_paths.is_empty() {
                                let _ = self.mark_attempt_graph_snapshot_dirty_paths(
                                    flow.project_id,
                                    attempt_id,
                                    &dirty_paths,
                                    "runtime_filesystem_observed",
                                    origin,
                                );
                            }
                        }
                    }
                }
            }
        }

        if !stream_live_output {
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

                let observations = runtime_projector
                    .observe_chunk(RuntimeOutputStream::Stdout, &format!("{content}\n"));
                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    attempt_corr,
                    filter_projected_runtime_observations(
                        observations,
                        has_structured_command_events,
                    ),
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

                let observations = runtime_projector
                    .observe_chunk(RuntimeOutputStream::Stderr, &format!("{content}\n"));
                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    attempt_corr,
                    filter_projected_runtime_observations(
                        observations,
                        has_structured_command_events,
                    ),
                    origin,
                );
            }

            let _ = self.append_structured_runtime_observations(
                attempt_id,
                attempt_corr,
                report.structured_runtime_observations.clone(),
                origin,
            );
        }

        let _ = self.append_projected_runtime_observations(
            attempt_id,
            attempt_corr,
            filter_projected_runtime_observations(
                runtime_projector.flush(),
                has_structured_command_events,
            ),
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

fn filter_projected_runtime_observations(
    observations: Vec<ProjectedRuntimeObservation>,
    suppress_command_observed: bool,
) -> Vec<ProjectedRuntimeObservation> {
    if !suppress_command_observed {
        return observations;
    }

    observations
        .into_iter()
        .filter(|observation| {
            !matches!(
                observation,
                ProjectedRuntimeObservation::CommandObserved { .. }
            )
        })
        .collect()
}

impl Registry {
    pub(super) fn apply_external_runtime_tool_directives(
        &self,
        attempt_id: Uuid,
        runtime_for_adapter: &ProjectRuntimeConfig,
        stdout: &str,
        stderr: &str,
        origin: &'static str,
    ) -> std::result::Result<(), (String, String, bool)> {
        if runtime_for_adapter.adapter_name == "native" {
            return Ok(());
        }

        let mut completed_checkpoint_ids = std::collections::BTreeSet::new();
        for line in stdout.lines().chain(stderr.lines()) {
            let trimmed = line.trim();
            let Some(action) = trimmed.strip_prefix("ACT:") else {
                continue;
            };

            let tool_action = match crate::native::tool_engine::NativeToolAction::parse(action) {
                Ok(Some(action)) => action,
                Ok(None) => {
                    return Err((
                        "external_runtime_directive_invalid".to_string(),
                        format!("Unsupported external runtime directive line: {trimmed}"),
                        false,
                    ));
                }
                Err(error) => {
                    return Err((
                        "external_runtime_directive_invalid".to_string(),
                        format!("Failed to parse external runtime directive '{trimmed}': {}", error.message),
                        false,
                    ));
                }
            };

            match tool_action.name.as_str() {
                "checkpoint_complete" => {
                    let checkpoint_id = tool_action
                        .input
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| {
                            (
                                "external_runtime_directive_invalid".to_string(),
                                "checkpoint_complete directive requires a non-empty 'id'"
                                    .to_string(),
                                false,
                            )
                        })?;
                    if !completed_checkpoint_ids.insert(checkpoint_id.to_string()) {
                        continue;
                    }
                    let summary = tool_action
                        .input
                        .get("summary")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|summary| !summary.is_empty());
                    match self.checkpoint_complete(&attempt_id.to_string(), checkpoint_id, summary) {
                        Ok(_) => {}
                        Err(err)
                            if err.code == "checkpoint_already_completed"
                                || err.code == "all_checkpoints_completed" => {}
                        Err(err) => {
                            self.record_error_event(&err, CorrelationIds::none());
                            return Err((
                                "external_runtime_directive_execution_failed".to_string(),
                                format!(
                                    "External runtime directive checkpoint_complete failed: {}",
                                    err.message
                                ),
                                false,
                            ));
                        }
                    }
                }
                tool_name => {
                    return Err((
                        "external_runtime_directive_unsupported".to_string(),
                        format!(
                            "External runtime requested unsupported built-in tool '{tool_name}'"
                        ),
                        false,
                    ));
                }
            }
        }

        let _ = origin;
        Ok(())
    }
}

fn attach_resume_session_if_supported(
    state: &AppState,
    flow: &TaskFlow,
    task_id: Uuid,
    attempt_id: Uuid,
    runtime_for_adapter: &mut ProjectRuntimeConfig,
) {
    if !matches!(
        runtime_for_adapter.adapter_name.as_str(),
        "opencode" | "codex" | "kilo"
    ) {
        return;
    }

    let Some(exec) = flow.task_executions.get(&task_id) else {
        return;
    };
    if exec.retry_mode != RetryMode::Continue {
        return;
    }

    let Some(current_attempt) = state.attempts.get(&attempt_id) else {
        return;
    };

    let previous = state
        .attempts
        .values()
        .filter(|attempt| {
            attempt.flow_id == flow.id
                && attempt.task_id == task_id
                && attempt.attempt_number < current_attempt.attempt_number
        })
        .filter_map(|attempt| {
            attempt.runtime_session.as_ref().and_then(|session| {
                (session.adapter_name == runtime_for_adapter.adapter_name)
                    .then_some((attempt, session))
            })
        })
        .max_by_key(|(attempt, _)| attempt.attempt_number);

    let Some((previous_attempt, session)) = previous else {
        return;
    };

    runtime_for_adapter.env.insert(
        "HIVEMIND_RUNTIME_RESUME_SESSION_ID".to_string(),
        session.session_id.clone(),
    );
    runtime_for_adapter.env.insert(
        "HIVEMIND_RUNTIME_RESUME_PARENT_ATTEMPT_ID".to_string(),
        previous_attempt.id.to_string(),
    );
}
