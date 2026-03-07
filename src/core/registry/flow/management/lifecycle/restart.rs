use super::*;

impl Registry {
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
}
