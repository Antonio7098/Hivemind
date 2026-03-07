use super::*;

mod governance;
mod project;
mod runtime;
mod support;
mod task;

#[cfg(test)]
mod tests;

pub(super) use support::{governance_artifact_key, project_runtime_config, task_runtime_config};

impl AppState {
    pub(super) fn apply_catalog_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        self.apply_project_catalog_event(payload, timestamp)
            || self.apply_runtime_catalog_event(payload, timestamp)
            || self.apply_task_catalog_event(payload, timestamp)
            || self.apply_governance_catalog_event(payload, timestamp)
    }
}
