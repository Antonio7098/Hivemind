use super::*;

mod attempt;
mod chat;
mod flow;
mod graph;
mod merge;
mod support;
mod workflow;

impl AppState {
    /// Applies an event to the state in place.
    pub fn apply_mut(&mut self, event: &Event) {
        let timestamp = event.timestamp();

        if self.apply_catalog_event(&event.payload, timestamp)
            || self.apply_graph_event(&event.payload, timestamp)
            || self.apply_flow_event(event, timestamp)
            || self.apply_workflow_event(&event.payload, timestamp)
            || self.apply_attempt_event(&event.payload, timestamp)
            || self.apply_chat_event(&event.payload, timestamp)
            || self.apply_merge_event(&event.payload, timestamp)
            || Self::is_ignored_event(&event.payload)
        {}
    }

    /// Replays a sequence of events to produce state.
    /// Deterministic: same events → same state.
    #[must_use]
    pub fn replay(events: &[Event]) -> Self {
        let mut state = Self::new();
        for event in events {
            state.apply_mut(event);
        }
        state
    }
}
