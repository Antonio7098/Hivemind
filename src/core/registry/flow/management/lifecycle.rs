use super::*;

impl Registry {
    pub fn create_flow(&self, graph_id: &str, name: Option<&str>) -> Result<TaskFlow> {
        let graph = self
            .get_graph(graph_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let issues = Self::validate_graph_issues(&graph);

        if graph.state == GraphState::Draft {
            let valid = issues.is_empty();
            let event = Event::new(
                EventPayload::TaskGraphValidated {
                    graph_id: graph.id,
                    project_id: graph.project_id,
                    valid,
                    issues: issues.clone(),
                },
                CorrelationIds::for_graph(graph.project_id, graph.id),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:create_flow")
            })?;

            if valid {
                let event = Event::new(
                    EventPayload::TaskGraphLocked {
                        graph_id: graph.id,
                        project_id: graph.project_id,
                    },
                    CorrelationIds::for_graph(graph.project_id, graph.id),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:create_flow",
                    )
                })?;
            }
        }

        if !issues.is_empty() {
            let err = HivemindError::user(
                "graph_invalid",
                "Graph validation failed",
                "registry:create_flow",
            )
            .with_context("graph_id", graph.id.to_string());
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let state = self.state()?;
        let has_active = state.flows.values().any(|f| {
            f.graph_id == graph.id
                && !matches!(
                    f.state,
                    FlowState::Completed | FlowState::Merged | FlowState::Aborted
                )
        });
        if has_active {
            let err = HivemindError::user(
                "graph_in_use",
                "Graph already used by an active flow",
                "registry:create_flow",
            )
            .with_context("graph_id", graph.id.to_string());
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let flow_id = Uuid::new_v4();
        let task_ids: Vec<Uuid> = graph.tasks.keys().copied().collect();
        let event = Event::new(
            EventPayload::TaskFlowCreated {
                flow_id,
                graph_id: graph.id,
                project_id: graph.project_id,
                name: name.map(String::from),
                task_ids,
            },
            CorrelationIds::for_graph_flow(graph.project_id, graph.id, flow_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:create_flow")
        })?;

        self.get_flow(&flow_id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn start_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self
            .get_flow(flow_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        match flow.state {
            FlowState::Created => {}
            FlowState::Paused => {
                let event = Event::new(
                    EventPayload::TaskFlowResumed { flow_id: flow.id },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:start_flow",
                    )
                })?;
                let resumed = self.get_flow(flow_id)?;
                if resumed.run_mode == RunMode::Auto {
                    return self.auto_progress_flow(flow_id);
                }
                return Ok(resumed);
            }
            FlowState::Running => {
                let err = HivemindError::user(
                    "flow_already_running",
                    "Flow is already running",
                    "registry:start_flow",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
            FlowState::Completed => {
                let err = HivemindError::user(
                    "flow_completed",
                    "Flow has already completed",
                    "registry:start_flow",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
            FlowState::FrozenForMerge => {
                let err = HivemindError::user(
                    "flow_frozen",
                    "Flow is frozen for merge",
                    "registry:start_flow",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
            FlowState::Merged => {
                let err = HivemindError::user(
                    "flow_merged",
                    "Flow has already been merged",
                    "registry:start_flow",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
            FlowState::Aborted => {
                let err =
                    HivemindError::user("flow_aborted", "Flow was aborted", "registry:start_flow");
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
        }

        let state = self.state()?;
        let unmet = Self::unmet_flow_dependencies(&state, &flow);
        if !unmet.is_empty() {
            let preview = unmet
                .iter()
                .take(5)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if unmet.len() > 5 {
                format!(" (+{} more)", unmet.len().saturating_sub(5))
            } else {
                String::new()
            };
            let err = HivemindError::user(
                "flow_dependencies_unmet",
                format!("Flow dependencies are not completed: {preview}{suffix}"),
                "registry:start_flow",
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            );
            return Err(err);
        }

        let base_revision = state
            .projects
            .get(&flow.project_id)
            .and_then(|p| p.repositories.first())
            .and_then(|repo| {
                std::process::Command::new("git")
                    .current_dir(&repo.path)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .filter(|s| !s.is_empty())
            });

        if let Some(project) = state.projects.get(&flow.project_id) {
            for repo in &project.repositories {
                let repo_head = std::process::Command::new("git")
                    .current_dir(&repo.path)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .filter(|s| !s.is_empty());
                if let Some(head) = repo_head {
                    let _ = std::process::Command::new("git")
                        .current_dir(&repo.path)
                        .args(["branch", "-f", &format!("flow/{}", flow.id), &head])
                        .output();
                }
            }
        }

        let event = Event::new(
            EventPayload::TaskFlowStarted {
                flow_id: flow.id,
                base_revision,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:start_flow")
        })?;

        if let Some(graph) = state.graphs.get(&flow.graph_id) {
            let ready = graph.root_tasks();
            for task_id in ready {
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
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:start_flow",
                    )
                })?;
            }
        }

        let started = self.get_flow(flow_id)?;
        if started.run_mode == RunMode::Auto {
            return self.auto_progress_flow(flow_id);
        }
        Ok(started)
    }

    pub fn restart_flow(&self, flow_id: &str, name: Option<&str>, start: bool) -> Result<TaskFlow> {
        let source = self.get_flow(flow_id)?;
        if source.state != FlowState::Aborted {
            return Err(HivemindError::user(
                "flow_not_aborted",
                "Only aborted flows can be restarted",
                "registry:restart_flow",
            )
            .with_hint("Abort the flow first, or create a new flow from the graph"));
        }

        let state = self.state()?;
        let runtime_defaults = state
            .flow_runtime_defaults
            .get(&source.id)
            .cloned()
            .unwrap_or_default();
        let mut dependencies: Vec<_> = source.depends_on_flows.iter().copied().collect();
        dependencies.sort();

        let source_graph_id = source.graph_id;
        let source_run_mode = source.run_mode;
        drop(state);

        let mut restarted = self.create_flow(&source_graph_id.to_string(), name)?;
        let restarted_id = restarted.id.to_string();

        for dep in dependencies {
            restarted = self.flow_add_dependency(&restarted_id, &dep.to_string())?;
        }

        if let Some(config) = runtime_defaults.worker {
            let env_pairs: Vec<String> = config
                .env
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect();
            restarted = self.flow_runtime_set(
                &restarted_id,
                RuntimeRole::Worker,
                &config.adapter_name,
                &config.binary_path,
                config.model,
                &config.args,
                &env_pairs,
                config.timeout_ms,
                config.max_parallel_tasks,
            )?;
        }
        if let Some(config) = runtime_defaults.validator {
            let env_pairs: Vec<String> = config
                .env
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect();
            restarted = self.flow_runtime_set(
                &restarted_id,
                RuntimeRole::Validator,
                &config.adapter_name,
                &config.binary_path,
                config.model,
                &config.args,
                &env_pairs,
                config.timeout_ms,
                config.max_parallel_tasks,
            )?;
        }

        if source_run_mode != RunMode::Manual {
            restarted = self.flow_set_run_mode(&restarted_id, source_run_mode)?;
        }

        if start && restarted.state == FlowState::Created {
            restarted = self.start_flow(&restarted_id)?;
        }

        Ok(restarted)
    }

    pub fn pause_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;

        match flow.state {
            FlowState::Paused => return Ok(flow),
            FlowState::Running => {}
            _ => {
                return Err(HivemindError::user(
                    "flow_not_running",
                    "Flow is not in running state",
                    "registry:pause_flow",
                ));
            }
        }

        let running_tasks: Vec<Uuid> = flow.tasks_in_state(TaskExecState::Running);
        let event = Event::new(
            EventPayload::TaskFlowPaused {
                flow_id: flow.id,
                running_tasks,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:pause_flow")
        })?;

        self.get_flow(flow_id)
    }

    pub fn resume_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Paused {
            return Err(HivemindError::user(
                "flow_not_paused",
                "Flow is not paused",
                "registry:resume_flow",
            ));
        }

        let event = Event::new(
            EventPayload::TaskFlowResumed { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:resume_flow")
        })?;

        let resumed = self.get_flow(flow_id)?;
        if resumed.run_mode == RunMode::Auto {
            return self.auto_progress_flow(flow_id);
        }
        Ok(resumed)
    }

    pub fn abort_flow(
        &self,
        flow_id: &str,
        reason: Option<&str>,
        forced: bool,
    ) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state == FlowState::Aborted {
            return Ok(flow);
        }
        if flow.state == FlowState::Completed {
            return Err(HivemindError::user(
                "flow_already_terminal",
                "Flow is completed",
                "registry:abort_flow",
            ));
        }

        let state = self.state()?;
        let mut latest_attempt_ids: HashMap<Uuid, (chrono::DateTime<Utc>, Uuid)> = HashMap::new();
        for attempt in state.attempts.values().filter(|a| a.flow_id == flow.id) {
            latest_attempt_ids
                .entry(attempt.task_id)
                .and_modify(|slot| {
                    if attempt.started_at > slot.0 {
                        *slot = (attempt.started_at, attempt.id);
                    }
                })
                .or_insert((attempt.started_at, attempt.id));
        }

        for (task_id, exec) in &flow.task_executions {
            if !matches!(
                exec.state,
                TaskExecState::Running | TaskExecState::Verifying
            ) {
                continue;
            }

            let attempt_id = latest_attempt_ids.get(task_id).map(|(_, id)| *id);
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id: flow.id,
                        task_id: *task_id,
                        attempt_id,
                        from: exec.state,
                        to: TaskExecState::Failed,
                    },
                    CorrelationIds::for_graph_flow_task(
                        flow.project_id,
                        flow.graph_id,
                        flow.id,
                        *task_id,
                    ),
                ),
                "registry:abort_flow",
            )?;
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionFailed {
                        flow_id: flow.id,
                        task_id: *task_id,
                        attempt_id,
                        reason: Some("flow_aborted".to_string()),
                    },
                    attempt_id.map_or_else(
                        || {
                            CorrelationIds::for_graph_flow_task(
                                flow.project_id,
                                flow.graph_id,
                                flow.id,
                                *task_id,
                            )
                        },
                        |aid| {
                            CorrelationIds::for_graph_flow_task_attempt(
                                flow.project_id,
                                flow.graph_id,
                                flow.id,
                                *task_id,
                                aid,
                            )
                        },
                    ),
                ),
                "registry:abort_flow",
            )?;
        }

        let event = Event::new(
            EventPayload::TaskFlowAborted {
                flow_id: flow.id,
                reason: reason.map(String::from),
                forced,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:abort_flow")
        })?;

        self.get_flow(flow_id)
    }

    pub fn replay_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let fid = Uuid::parse_str(flow_id).map_err(|_| {
            HivemindError::user(
                "invalid_flow_id",
                format!("'{flow_id}' is not a valid flow ID"),
                "registry:replay_flow",
            )
        })?;

        let filter = EventFilter {
            flow_id: Some(fid),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;
        if events.is_empty() {
            return Err(HivemindError::user(
                "flow_not_found",
                format!("No events found for flow '{flow_id}'"),
                "registry:replay_flow",
            ));
        }

        let all_events = self.store.read_all().map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:replay_flow")
        })?;
        let flow_related: Vec<Event> = all_events
            .into_iter()
            .filter(|e| {
                e.metadata.correlation.flow_id == Some(fid)
                    || match &e.payload {
                        EventPayload::TaskFlowCreated { flow_id: f, .. } => *f == fid,
                        _ => false,
                    }
            })
            .collect();

        let replayed = crate::core::state::AppState::replay(&flow_related);
        replayed.flows.get(&fid).cloned().ok_or_else(|| {
            HivemindError::user(
                "flow_not_found",
                format!("Flow '{flow_id}' not found in replayed state"),
                "registry:replay_flow",
            )
        })
    }
}
