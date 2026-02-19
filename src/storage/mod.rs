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
//! use uuid::Uuid;
//!
//! let temp_dir = tempfile::tempdir()?;
//! let store = SqliteEventStore::open(temp_dir.path())?;
//!
//! // Query events by project
//! let filter = EventFilter {
//!     project_id: Some(Uuid::new_v4()),
//!     ..Default::default()
//! };
//! let events = store.read(&filter)?;
//! # Ok::<(), hivemind::storage::event_store::EventStoreError>(())
//! ```
//!
//! # Modules
//!
//! - [`event_store`] - Event store trait and `SQLite` implementation

pub mod event_store;
