use super::*;

impl AppState {
    pub(super) fn apply_project_deleted(&mut self, project_id: Uuid) {
        self.projects.remove(&project_id);

        let flow_ids: std::collections::HashSet<Uuid> = self
            .flows
            .values()
            .filter(|flow| flow.project_id == project_id)
            .map(|flow| flow.id)
            .collect();

        self.tasks.retain(|_, task| task.project_id != project_id);
        self.graphs
            .retain(|_, graph| graph.project_id != project_id);
        self.flows.retain(|_, flow| flow.project_id != project_id);
        self.flow_runtime_defaults
            .retain(|flow_id, _| !flow_ids.contains(flow_id));
        self.merge_states
            .retain(|flow_id, _| !flow_ids.contains(flow_id));
        self.attempts
            .retain(|_, attempt| !flow_ids.contains(&attempt.flow_id));
    }
}

pub(crate) fn governance_artifact_key(
    project_id: Option<Uuid>,
    scope: &str,
    artifact_kind: &str,
    artifact_key: &str,
) -> String {
    let project_key = project_id.map_or_else(|| "global".to_string(), |id| id.to_string());
    format!("{project_key}::{scope}::{artifact_kind}::{artifact_key}")
}

pub(crate) fn project_runtime_config(
    adapter_name: &str,
    binary_path: &str,
    model: &Option<String>,
    args: &[String],
    env: &HashMap<String, String>,
    timeout_ms: u64,
    max_parallel_tasks: u16,
) -> ProjectRuntimeConfig {
    ProjectRuntimeConfig {
        adapter_name: adapter_name.to_string(),
        binary_path: binary_path.to_string(),
        model: model.clone(),
        args: args.to_vec(),
        env: env.clone(),
        timeout_ms,
        max_parallel_tasks,
    }
}

pub(crate) fn task_runtime_config(
    adapter_name: &str,
    binary_path: &str,
    model: &Option<String>,
    args: &[String],
    env: &HashMap<String, String>,
    timeout_ms: u64,
) -> TaskRuntimeConfig {
    TaskRuntimeConfig {
        adapter_name: adapter_name.to_string(),
        binary_path: binary_path.to_string(),
        model: model.clone(),
        args: args.to_vec(),
        env: env.clone(),
        timeout_ms,
    }
}
