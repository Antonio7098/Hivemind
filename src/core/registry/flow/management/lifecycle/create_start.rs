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
}
