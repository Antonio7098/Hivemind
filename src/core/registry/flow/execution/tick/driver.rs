use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub fn tick_flow(
        &self,
        flow_id: &str,
        interactive: bool,
        max_parallel: Option<u16>,
    ) -> Result<TaskFlow> {
        if interactive {
            return Err(HivemindError::user(
                "interactive_mode_deprecated",
                "Interactive mode is deprecated and no longer supported",
                "registry:tick_flow",
            )
            .with_hint("Re-run without --interactive"));
        }

        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Running {
            return Err(HivemindError::user(
                "flow_not_running",
                "Flow is not in running state",
                "registry:tick_flow",
            ));
        }

        let state = self.state()?;
        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                "registry:tick_flow",
            )
        })?;

        let flow_defaults = state
            .flow_runtime_defaults
            .get(&flow.id)
            .cloned()
            .unwrap_or_default();
        let configured_limit = flow_defaults
            .worker
            .as_ref()
            .map(|cfg| cfg.max_parallel_tasks.max(1))
            .or_else(|| {
                Self::project_runtime_for_role(project, RuntimeRole::Worker)
                    .map(|cfg| cfg.max_parallel_tasks.max(1))
            })
            .or_else(|| {
                state
                    .global_runtime_defaults
                    .worker
                    .as_ref()
                    .map(|cfg| cfg.max_parallel_tasks.max(1))
            })
            .unwrap_or(1_u16);
        let requested_limit = max_parallel.unwrap_or(configured_limit);
        let global_limit = match env::var("HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL") {
            Ok(raw) => Self::parse_global_parallel_limit(Some(raw))?,
            Err(env::VarError::NotPresent) => Self::parse_global_parallel_limit(None)?,
            Err(env::VarError::NotUnicode(_)) => {
                return Err(HivemindError::user(
                    "invalid_global_parallel_limit",
                    "HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL must be valid UTF-8",
                    "registry:tick_flow",
                ))
            }
        };

        let limit = requested_limit.min(global_limit);
        if limit == 0 {
            return Err(HivemindError::user(
                "invalid_max_parallel",
                "max_parallel must be at least 1",
                "registry:tick_flow",
            )
            .with_hint("Use --max-parallel 1 or higher"));
        }

        let mut started_in_tick: Vec<(Uuid, Scope)> = Vec::new();
        let mut latest_flow = flow;

        for _ in 0..usize::from(limit) {
            let snapshot = self.get_flow(flow_id)?;
            latest_flow = snapshot.clone();
            if snapshot.state != FlowState::Running {
                break;
            }

            let mut verifying = snapshot.tasks_in_state(TaskExecState::Verifying);
            verifying.sort();
            if let Some(task_id) = verifying.first().copied() {
                latest_flow = self.process_verifying_task(flow_id, task_id)?;
                continue;
            }

            let state = self.state()?;
            let graph = state.graphs.get(&snapshot.graph_id).ok_or_else(|| {
                HivemindError::system("graph_not_found", "Graph not found", "registry:tick_flow")
            })?;

            let mut retrying = snapshot.tasks_in_state(TaskExecState::Retry);
            retrying.sort();
            let mut ready = snapshot.tasks_in_state(TaskExecState::Ready);
            ready.sort();

            let mut candidates = retrying;
            for task_id in ready {
                if !candidates.contains(&task_id) {
                    candidates.push(task_id);
                }
            }

            if candidates.is_empty() {
                latest_flow = self.tick_flow_once(flow_id, interactive, None)?;
                break;
            }

            let mut active_scopes = started_in_tick.clone();
            let mut running = snapshot.tasks_in_state(TaskExecState::Running);
            running.sort();
            for running_id in running {
                if active_scopes.iter().any(|(id, _)| *id == running_id) {
                    continue;
                }
                if let Some(task) = graph.tasks.get(&running_id) {
                    active_scopes.push((running_id, task.scope.clone().unwrap_or_default()));
                }
            }

            let mut chosen: Option<(Uuid, Scope)> = None;

            for candidate_id in candidates {
                if !Self::can_auto_run_task(&state, candidate_id) {
                    continue;
                }
                let Some(task) = graph.tasks.get(&candidate_id) else {
                    continue;
                };

                let candidate_scope = task.scope.clone().unwrap_or_default();
                let mut hard_conflict: Option<(Uuid, String)> = None;
                let mut soft_conflict: Option<(Uuid, String)> = None;

                for (other_id, other_scope) in &active_scopes {
                    if *other_id == candidate_id {
                        continue;
                    }

                    match check_compatibility(&candidate_scope, other_scope) {
                        ScopeCompatibility::HardConflict => {
                            hard_conflict = Some((
                                    *other_id,
                                    format!(
                                        "Hard scope conflict with task {other_id}; serialized in this tick"
                                    ),
                                ));
                            break;
                        }
                        ScopeCompatibility::SoftConflict => {
                            if soft_conflict.is_none() {
                                soft_conflict = Some((
                                        *other_id,
                                        format!(
                                            "Soft scope conflict with task {other_id}; allowing parallel attempt with warning"
                                        ),
                                    ));
                            }
                        }
                        ScopeCompatibility::Compatible => {}
                    }
                }

                if let Some((conflicting_task_id, reason)) = hard_conflict {
                    self.append_event(
                        Event::new(
                            EventPayload::ScopeConflictDetected {
                                flow_id: snapshot.id,
                                task_id: candidate_id,
                                conflicting_task_id,
                                severity: "hard_conflict".to_string(),
                                action: "serialized".to_string(),
                                reason: reason.clone(),
                            },
                            CorrelationIds::for_graph_flow_task(
                                snapshot.project_id,
                                snapshot.graph_id,
                                snapshot.id,
                                candidate_id,
                            ),
                        ),
                        "registry:tick_flow",
                    )?;

                    self.append_event(
                        Event::new(
                            EventPayload::TaskSchedulingDeferred {
                                flow_id: snapshot.id,
                                task_id: candidate_id,
                                reason,
                            },
                            CorrelationIds::for_graph_flow_task(
                                snapshot.project_id,
                                snapshot.graph_id,
                                snapshot.id,
                                candidate_id,
                            ),
                        ),
                        "registry:tick_flow",
                    )?;
                    continue;
                }

                if let Some((conflicting_task_id, reason)) = soft_conflict {
                    self.append_event(
                        Event::new(
                            EventPayload::ScopeConflictDetected {
                                flow_id: snapshot.id,
                                task_id: candidate_id,
                                conflicting_task_id,
                                severity: "soft_conflict".to_string(),
                                action: "warn_parallel".to_string(),
                                reason,
                            },
                            CorrelationIds::for_graph_flow_task(
                                snapshot.project_id,
                                snapshot.graph_id,
                                snapshot.id,
                                candidate_id,
                            ),
                        ),
                        "registry:tick_flow",
                    )?;
                }

                chosen = Some((candidate_id, candidate_scope));
                break;
            }

            let Some((task_id, scope)) = chosen else {
                break;
            };

            started_in_tick.push((task_id, scope));
            latest_flow = self.tick_flow_once(flow_id, interactive, Some(task_id))?;
        }

        Ok(latest_flow)
    }
}
