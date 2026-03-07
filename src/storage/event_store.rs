//! `EventStore` trait and implementations.
//!
//! Event stores are the persistence layer for events. All state is derived
//! from events, so the event store is the single source of truth.

use crate::core::events::{Event, EventId, EventPayload};
use chrono::Duration as ChronoDuration;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

mod sqlite_store;
pub use sqlite_store::*;

mod jsonl_stores;
pub use jsonl_stores::*;
mod helpers;
pub(crate) use helpers::{
    nanos_to_timestamp, normalize_concatenated_json_objects, timestamp_to_nanos,
};
pub use helpers::{EventStoreError, Result};
mod filter;
pub use filter::*;
mod memory;
pub use memory::*;

/// Trait for event storage backends.
pub trait EventStore: Send + Sync {
    /// Appends an event to the store, returning its assigned ID.
    fn append(&self, event: Event) -> Result<EventId>;

    /// Reads events matching the filter.
    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>>;

    /// Streams events matching the filter.
    ///
    /// Implementations should emit matching historical events first, then continue
    /// yielding new events as they are appended.
    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>>;

    /// Reads all events in order.
    fn read_all(&self) -> Result<Vec<Event>>;
}
/// Thread-safe wrapper for any event store.
pub type SharedEventStore = Arc<dyn EventStore>;
