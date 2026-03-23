use super::*;

mod lifecycle;
mod runtime;

impl AppState {
    pub(super) fn apply_flow_event(&mut self, event: &Event, timestamp: DateTime<Utc>) -> bool {
        self.apply_flow_lifecycle_event(&event.payload, timestamp)
            || self.apply_flow_runtime_event(&event.payload, timestamp)
    }
}
