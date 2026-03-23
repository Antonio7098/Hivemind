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

    pub(super) fn apply_external_runtime_tool_directives(
        &self,
        attempt_id: Uuid,
        _runtime_for_adapter: &ProjectRuntimeConfig,
        stdout: &str,
        stderr: &str,
        _origin: &'static str,
    ) -> std::result::Result<(), (String, String, bool)> {
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
