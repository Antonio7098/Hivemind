// ARCH_DEBT: legacy oversized function
#![allow(clippy::too_many_lines)]

use super::*;
use crate::adapters::runtime::StructuredRuntimeObservation;
use serde_json::Value;

mod adapter_lifecycle;
mod environment;
mod filesystem;
mod interactive;
mod observations;

pub(super) struct TickRuntimeExecution {
    pub(super) runtime_for_adapter: ProjectRuntimeConfig,
    pub(super) report: crate::adapters::runtime::ExecutionReport,
}

const MAX_EXTERNAL_CHECKPOINT_RECOVERY_ATTEMPTS: u8 = 3;

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
        self.apply_tick_runtime_environment(
            state,
            flow,
            task_id,
            worktree_status,
            repo_worktrees,
            &mut runtime_for_adapter,
            task_scope,
            attempt_id,
            origin,
        )?;

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
            return self.handle_tick_runtime_adapter_error(
                state,
                flow,
                task_id,
                attempt_id,
                &runtime_for_adapter,
                next_attempt_number,
                max_attempts,
                &e,
                origin,
            );
        }
        if let Err(e) = adapter.prepare(task_id, &worktree_status.path) {
            return self.handle_tick_runtime_adapter_error(
                state,
                flow,
                task_id,
                attempt_id,
                &runtime_for_adapter,
                next_attempt_number,
                max_attempts,
                &e,
                origin,
            );
        }

        let mut runtime_projector = RuntimeEventProjector::new();

        let (report, terminated_reason) = if interactive {
            let mut stdout = std::io::stdout();
            let res = adapter.execute_interactive(&input, |evt| {
                self.handle_interactive_adapter_event(
                    attempt_id,
                    attempt_corr,
                    &mut runtime_projector,
                    &mut stdout,
                    evt,
                    origin,
                )
            });

            match res {
                Ok(r) => (r.report, r.terminated_reason),
                Err(e) => {
                    return self.handle_tick_runtime_adapter_error(
                        state,
                        flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &e,
                        origin,
                    );
                }
            }
        } else {
            let report = match adapter.execute(input) {
                Ok(r) => r,
                Err(e) => {
                    return self.handle_tick_runtime_adapter_error(
                        state,
                        flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &e,
                        origin,
                    );
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

        self.append_runtime_filesystem_observation(attempt_id, attempt_corr, worktree_status);

        let has_structured_command_events =
            report
                .structured_runtime_observations
                .iter()
                .any(|observation| {
                    matches!(
                        observation,
                        StructuredRuntimeObservation::CommandCompleted { .. }
                    )
                });

        if !interactive {
            self.append_runtime_output_stream(
                attempt_id,
                attempt_corr,
                &mut runtime_projector,
                RuntimeOutputStream::Stdout,
                &report.stdout,
                has_structured_command_events,
                origin,
            )?;
            self.append_runtime_output_stream(
                attempt_id,
                attempt_corr,
                &mut runtime_projector,
                RuntimeOutputStream::Stderr,
                &report.stderr,
                has_structured_command_events,
                origin,
            )?;

            if let Err(e) = self.append_structured_runtime_observations(
                attempt_id,
                attempt_corr,
                report.structured_runtime_observations.clone(),
                origin,
            ) {
                eprintln!(
                    "ERROR: Failed to append structured runtime observations: {} (attempt_id={}, origin={})",
                    e, attempt_id, origin
                );
            } else {
                eprintln!("DEBUG: Successfully appended {} structured runtime observations for attempt {}",
                    report.structured_runtime_observations.len(), attempt_id);
            }
        }

        let mut all_projected = runtime_projector.flush();
        all_projected.extend(report.projected_runtime_observations.clone());
        let projected =
            filter_projected_runtime_observations(all_projected, has_structured_command_events);
        if let Err(e) = self.append_projected_runtime_observations(
            attempt_id,
            attempt_corr,
            projected.clone(),
            origin,
        ) {
            eprintln!(
                "ERROR: Failed to append projected runtime observations: {} (attempt_id={}, origin={}, count={})",
                e, attempt_id, origin, projected.len()
            );
        } else if !projected.is_empty() {
            eprintln!(
                "DEBUG: Successfully appended {} projected runtime observations for attempt {}",
                projected.len(),
                attempt_id
            );
        }

        if let Some(reason) = terminated_reason {
            if let Err(e) = self.store.append(Event::new(
                EventPayload::RuntimeTerminated { attempt_id, reason },
                attempt_corr.clone(),
            )) {
                eprintln!(
                    "ERROR: Failed to append RuntimeTerminated event: {} (attempt_id={})",
                    e, attempt_id
                );
            }
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

    fn runtime_supports_checkpoint_session_repair(adapter_name: &str) -> bool {
        matches!(adapter_name, "opencode" | "codex" | "kilo")
    }

    fn truncate_checkpoint_repair_excerpt(value: &str, max_chars: usize) -> String {
        let truncated = value.chars().take(max_chars).collect::<String>();
        if value.chars().count() > max_chars {
            format!("{truncated}...")
        } else {
            truncated
        }
    }

    fn checkpoint_repair_input(
        &self,
        state: &AppState,
        attempt_id: Uuid,
        adapter_name: &str,
        original_input: &ExecutionInput,
        stdout: &str,
        stderr: &str,
        repair_attempt: u8,
    ) -> Option<ExecutionInput> {
        let attempt = state.attempts.get(&attempt_id)?;
        if attempt.all_checkpoints_completed || attempt.runtime_session.is_none() {
            return None;
        }

        let pending = attempt
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.state != AttemptCheckpointState::Completed)
            .map(|checkpoint| checkpoint.checkpoint_id.clone())
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return None;
        }

        let active_checkpoint = attempt
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.state == AttemptCheckpointState::Active)
            .or_else(|| {
                attempt
                    .checkpoints
                    .iter()
                    .find(|checkpoint| checkpoint.state != AttemptCheckpointState::Completed)
            })?
            .checkpoint_id
            .clone();

        let mut context_parts = Vec::new();
        if let Some(context) = original_input.context.as_deref() {
            context_parts.push(context.to_string());
        }
        let completion_instruction = if adapter_name == "native" {
            format!(
                "Checkpoint completion repair attempt {repair_attempt}/{MAX_EXTERNAL_CHECKPOINT_RECOVERY_ATTEMPTS}: the previous runtime invocation ended before the task's remaining checkpoints were completed. Continue in the same runtime session and complete all remaining checkpoints in order before ending. Active checkpoint: {active_checkpoint}. Pending checkpoints: {}. Emit one directive line per completed checkpoint using the built-in format ACT:tool:checkpoint_complete:{{\"id\":\"<checkpoint-id>\",\"summary\":\"short progress summary\"}}.",
                pending.join(", ")
            )
        } else {
            format!(
                "Checkpoint completion repair attempt {repair_attempt}/{MAX_EXTERNAL_CHECKPOINT_RECOVERY_ATTEMPTS}: the previous runtime invocation ended before the task's remaining checkpoints were completed. Continue in the same runtime session and complete all remaining checkpoints in order before ending. Active checkpoint: {active_checkpoint}. Pending checkpoints: {}. Complete checkpoints by running: hivemind checkpoint complete --attempt-id {attempt_id} --id <checkpoint-id> --summary \"short progress summary\".",
                pending.join(", ")
            )
        };
        context_parts.push(completion_instruction);
        if !stdout.trim().is_empty() {
            context_parts.push(format!(
                "Previous runtime stdout:\n{}",
                Self::truncate_checkpoint_repair_excerpt(stdout, 4000)
            ));
        }
        if !stderr.trim().is_empty() {
            context_parts.push(format!(
                "Previous runtime stderr:\n{}",
                Self::truncate_checkpoint_repair_excerpt(stderr, 2000)
            ));
        }

        let mut repair_input = original_input.clone();
        repair_input.context = Some(context_parts.join("\n\n"));
        Some(repair_input)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_recover_checkpoint_completion_after_runtime_exit(
        &self,
        flow_id: &str,
        interactive: bool,
        task_id: Uuid,
        worktree_status: &WorktreeStatus,
        repo_worktrees: &[(String, WorktreeStatus)],
        runtime_for_adapter: &ProjectRuntimeConfig,
        runtime_selection_source: RuntimeSelectionSource,
        task_scope: Option<Scope>,
        original_input: &ExecutionInput,
        attempt_id: Uuid,
        attempt_corr: &CorrelationIds,
        next_attempt_number: u32,
        max_attempts: u32,
        repair_attempt: u8,
        stdout: &str,
        stderr: &str,
        origin: &'static str,
    ) -> Result<Option<TickRuntimeExecution>> {
        if !Self::runtime_supports_checkpoint_session_repair(&runtime_for_adapter.adapter_name) {
            return Ok(None);
        }

        let latest_state = self.state()?;
        let latest_flow = self.get_flow(flow_id)?;
        let Some(repair_input) = self.checkpoint_repair_input(
            &latest_state,
            attempt_id,
            &runtime_for_adapter.adapter_name,
            original_input,
            stdout,
            stderr,
            repair_attempt,
        ) else {
            return Ok(None);
        };

        let runtime_prompt = format_execution_prompt(&repair_input);
        let mut runtime_flags = Self::runtime_start_flags(runtime_for_adapter);
        runtime_flags.push(format!("checkpoint-repair-attempt={repair_attempt}"));
        let Some(prepared_runtime) = self.prepare_runtime_for_tick_attempt(
            &latest_state,
            &latest_flow,
            task_id,
            worktree_status,
            repo_worktrees,
            runtime_for_adapter.clone(),
            runtime_selection_source,
            task_scope,
            attempt_id,
            attempt_corr,
            next_attempt_number,
            max_attempts,
            runtime_flags,
            runtime_prompt,
            origin,
        )?
        else {
            return Ok(None);
        };

        self.execute_tick_attempt(
            interactive,
            &latest_state,
            &latest_flow,
            task_id,
            worktree_status,
            prepared_runtime,
            repair_input,
            attempt_id,
            attempt_corr,
            next_attempt_number,
            max_attempts,
            origin,
        )
    }

    pub(super) fn apply_external_runtime_tool_directives(
        &self,
        attempt_id: Uuid,
        _runtime_for_adapter: &ProjectRuntimeConfig,
        stdout: &str,
        stderr: &str,
        _origin: &'static str,
    ) -> std::result::Result<(), (String, String, bool)> {
        let state = self
            .state()
            .map_err(|error| (error.code, error.message, error.recoverable))?;
        let checkpoints_enabled = state
            .attempts
            .get(&attempt_id)
            .and_then(|attempt| {
                state.flows.get(&attempt.flow_id).and_then(|flow| {
                    state
                        .graphs
                        .get(&flow.graph_id)
                        .and_then(|graph| graph.tasks.get(&attempt.task_id))
                        .map(|task| task.checkpoints_required)
                })
            })
            .unwrap_or(true);
        if !checkpoints_enabled {
            return Ok(());
        }

        for stream in [stdout, stderr] {
            for line in stream.lines() {
                let trimmed = line.trim();
                let Some(payload) = trimmed
                    .strip_prefix("ACT:tool:checkpoint_complete:")
                    .or_else(|| trimmed.strip_prefix("tool:checkpoint_complete:"))
                else {
                    continue;
                };
                let parsed: Value = serde_json::from_str(payload).map_err(|error| {
                    (
                        "runtime_tool_directive_invalid".to_string(),
                        format!("Invalid checkpoint_complete directive payload: {error}"),
                        true,
                    )
                })?;
                let checkpoint_id = parsed
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        (
                            "runtime_tool_directive_invalid".to_string(),
                            "checkpoint_complete directive is missing a non-empty 'id'".to_string(),
                            true,
                        )
                    })?;
                let summary = parsed.get("summary").and_then(Value::as_str);
                self.checkpoint_complete(&attempt_id.to_string(), checkpoint_id, summary)
                    .map_err(|error| (error.code, error.message, error.recoverable))?;
            }
        }
        Ok(())
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
