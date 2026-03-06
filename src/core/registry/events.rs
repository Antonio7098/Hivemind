//! Event registry support.

#![allow(clippy::wildcard_imports, unused_imports)]

use crate::core::registry::shared_prelude::*;
use crate::core::registry::shared_types::*;
use crate::core::registry::types::*;
use crate::core::registry::{
    Registry, ATTEMPT_CONTEXT_MAX_EXPAND_DEPTH, ATTEMPT_CONTEXT_SCHEMA_VERSION,
    ATTEMPT_CONTEXT_SECTION_BUDGET_BYTES, ATTEMPT_CONTEXT_TOTAL_BUDGET_BYTES,
    ATTEMPT_CONTEXT_TRUNCATION_POLICY, ATTEMPT_CONTEXT_VERSION, CONSTITUTION_SCHEMA_VERSION,
    CONSTITUTION_VERSION, GOVERNANCE_EXPORT_IMPORT_BOUNDARY, GOVERNANCE_FROM_LAYOUT,
    GOVERNANCE_PROJECTION_VERSION, GOVERNANCE_RECOVERY_SNAPSHOT_SCHEMA_VERSION,
    GOVERNANCE_SCHEMA_VERSION, GOVERNANCE_TO_LAYOUT, GRAPH_SNAPSHOT_SCHEMA_VERSION,
    GRAPH_SNAPSHOT_VERSION,
};

mod mirror;
mod recover;
mod verify;

impl Registry {
    pub(crate) fn append_event(&self, event: Event, origin: &'static str) -> Result<()> {
        self.store
            .append(event)
            .map(|_| ())
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))
    }

    pub(crate) fn record_error_event(&self, err: &HivemindError, correlation: CorrelationIds) {
        let _ = self.store.append(Event::new(
            EventPayload::ErrorOccurred { error: err.clone() },
            correlation,
        ));
    }

    pub fn read_events(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        self.store.read(filter).map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:read_events")
        })
    }

    pub fn stream_events(&self, filter: &EventFilter) -> Result<std::sync::mpsc::Receiver<Event>> {
        self.store.stream(filter).map_err(|e| {
            HivemindError::system(
                "event_stream_failed",
                e.to_string(),
                "registry:stream_events",
            )
        })
    }

    /// Gets a specific event by ID.
    ///
    /// # Errors
    /// Returns an error if the event cannot be read or is not found.
    pub fn get_event(&self, event_id: &str) -> Result<Event> {
        let id = Uuid::parse_str(event_id).map_err(|_| {
            HivemindError::user(
                "invalid_event_id",
                format!("'{event_id}' is not a valid event ID"),
                "registry:get_event",
            )
        })?;

        let events = self.store.read_all().map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:get_event")
        })?;

        events
            .into_iter()
            .find(|e| e.metadata.id.as_uuid() == id)
            .ok_or_else(|| {
                HivemindError::user(
                    "event_not_found",
                    format!("Event '{event_id}' not found"),
                    "registry:get_event",
                )
            })
    }
}
