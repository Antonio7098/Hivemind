use super::*;

impl Registry {
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
        let flow_corr = Self::correlation_for_flow_event(&state, &flow);
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
                    Self::correlation_for_flow_task_event(&state, &flow, *task_id),
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
                        || Self::correlation_for_flow_task_event(&state, &flow, *task_id),
                        |aid| {
                            Self::correlation_for_flow_task_attempt_event(
                                &state, &flow, *task_id, aid,
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
            flow_corr,
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
