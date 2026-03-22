use super::*;

impl Registry {
    pub fn list_flows(&self, project_id_or_name: Option<&str>) -> Result<Vec<TaskFlow>> {
        let project_filter = match project_id_or_name {
            Some(id_or_name) => Some(self.get_project(id_or_name)?.id),
            None => None,
        };

        let state = self.state()?;
        let mut flows: Vec<_> = state
            .flows
            .into_values()
            .filter(|flow| project_filter.is_none_or(|pid| flow.project_id == pid))
            .collect();
        flows.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
        flows.reverse();
        Ok(flows)
    }

    pub fn get_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let id = Uuid::parse_str(flow_id).map_err(|_| {
            HivemindError::user(
                "invalid_flow_id",
                format!("'{flow_id}' is not a valid flow ID"),
                "registry:get_flow",
            )
        })?;

        let state = self.state()?;
        state.flows.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "flow_not_found",
                format!("Flow '{flow_id}' not found"),
                "registry:get_flow",
            )
        })
    }

    /// Deletes a flow.
    ///
    /// # Errors
    /// Returns an error if the flow is currently active.
    pub fn delete_flow(&self, flow_id: &str) -> Result<Uuid> {
        let flow = self
            .get_flow(flow_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        if matches!(
            flow.state,
            FlowState::Running | FlowState::Paused | FlowState::FrozenForMerge
        ) {
            let err = HivemindError::user(
                "flow_active",
                "Cannot delete an active flow",
                "registry:delete_flow",
            )
            .with_hint("Abort or complete the flow before deleting it");
            self.record_error_event(&err, Self::correlation_for_flow_event(&state, &flow));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::TaskFlowDeleted {
                flow_id: flow.id,
                graph_id: flow.graph_id,
                project_id: flow.project_id,
            },
            Self::correlation_for_flow_event(&state, &flow),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:delete_flow")
        })?;

        Ok(flow.id)
    }

    pub fn list_attempts(
        &self,
        flow_id: Option<&str>,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AttemptListItem>> {
        let origin = "registry:list_attempts";
        let flow_filter = match flow_id {
            Some(raw) => Some(Uuid::parse_str(raw).map_err(|_| {
                HivemindError::user(
                    "invalid_flow_id",
                    format!("'{raw}' is not a valid flow ID"),
                    origin,
                )
            })?),
            None => None,
        };
        let task_filter = match task_id {
            Some(raw) => Some(Uuid::parse_str(raw).map_err(|_| {
                HivemindError::user(
                    "invalid_task_id",
                    format!("'{raw}' is not a valid task ID"),
                    origin,
                )
            })?),
            None => None,
        };

        let state = self.state()?;
        let mut attempts: Vec<_> = state
            .attempts
            .values()
            .filter(|attempt| flow_filter.is_none_or(|id| attempt.flow_id == id))
            .filter(|attempt| task_filter.is_none_or(|id| attempt.task_id == id))
            .cloned()
            .collect();
        attempts.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        Ok(attempts
            .into_iter()
            .take(limit)
            .map(|attempt| AttemptListItem {
                attempt_id: attempt.id,
                flow_id: attempt.flow_id,
                task_id: attempt.task_id,
                attempt_number: attempt.attempt_number,
                started_at: attempt.started_at,
                all_checkpoints_completed: attempt.all_checkpoints_completed,
            })
            .collect())
    }

    pub fn list_checkpoints(&self, attempt_id: &str) -> Result<Vec<AttemptCheckpoint>> {
        let attempt_uuid = Uuid::parse_str(attempt_id).map_err(|_| {
            HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                "registry:list_checkpoints",
            )
        })?;

        let state = self.state()?;
        let attempt = state
            .attempts
            .get(&attempt_uuid)
            .ok_or_else(|| {
                HivemindError::user(
                    "attempt_not_found",
                    "Attempt not found",
                    "registry:list_checkpoints",
                )
            })?
            .clone();

        Ok(attempt.checkpoints)
    }

    pub fn get_attempt(&self, attempt_id: &str) -> Result<AttemptState> {
        let id = Uuid::parse_str(attempt_id).map_err(|_| {
            HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                "registry:get_attempt",
            )
        })?;

        let state = self.state()?;
        state.attempts.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "attempt_not_found",
                format!("Attempt '{attempt_id}' not found"),
                "registry:get_attempt",
            )
        })
    }

    pub fn get_attempt_diff(&self, attempt_id: &str) -> Result<Option<String>> {
        let attempt = self.get_attempt(attempt_id)?;
        let Some(diff_id) = attempt.diff_id else {
            return Ok(None);
        };
        let artifact = self.read_diff_artifact(diff_id)?;
        Ok(Some(artifact.unified))
    }
}
