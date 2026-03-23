use super::*;

impl AppState {
    pub(super) fn apply_flow_runtime_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::TaskFlowRuntimeConfigured {
                flow_id,
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                let configured = ProjectRuntimeConfig {
                    adapter_name: adapter_name.clone(),
                    binary_path: binary_path.clone(),
                    model: model.clone(),
                    args: args.clone(),
                    env: env.clone(),
                    timeout_ms: *timeout_ms,
                    max_parallel_tasks: *max_parallel_tasks,
                };
                self.flow_runtime_defaults
                    .entry(*flow_id)
                    .or_default()
                    .set(*role, Some(configured));
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowRuntimeCleared { flow_id, role } => {
                self.flow_runtime_defaults
                    .entry(*flow_id)
                    .or_default()
                    .set(*role, None);
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowRunModeSet { flow_id, mode } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.run_mode = *mode;
                    flow.updated_at = timestamp;
                }
                true
            }
            _ => false,
        }
    }
}
