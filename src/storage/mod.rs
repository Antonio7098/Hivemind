//! Event storage implementations.
//!
//! This module provides the persistence layer for Hivemind's event-sourced
//! architecture. All state is derived from events, making the event store
//! the single source of truth.
//!
//! # Architecture
//!
//! The event store:
//! - Persists events durably (SQLite-backed)
//! - Provides efficient querying by correlation IDs
//! - Supports event filtering and pagination
//! - Ensures atomicity of event appends
//!
//! # Event Sourcing
//!
//! ```text
//! ┌─────────────┐     ┌──────────────┐     ┌─────────────┐
//! │   Events    │ ──▶ │  EventStore  │ ──▶ │   State     │
//! │ (immutable) │     │  (persist)   │     │  (derived)  │
//! └─────────────┘     └──────────────┘     └─────────────┘
//! ```
//!
//! # Example
//!
//! ```no_run
//! use hivemind::storage::event_store::{EventStore, EventFilter, SqliteEventStore};
//!
//! let store = SqliteEventStore::open("/path/to/events.db")?;
//!
//! // Query events by project
//! let filter = EventFilter {
//!     project_id: Some(project_id),
//!     ..Default::default()
//! };
//! let events = store.query(filter)?;
//! # Ok::<(), hivemind::storage::event_store::EventStoreError>(())
//! ```
//!
//! # Modules
//!
//! - [`event_store`] - Event store trait and SQLite implementation

pub mod event_store;
