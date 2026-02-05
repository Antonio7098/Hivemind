//! `EventStore` trait and implementations.
//!
//! Event stores are the persistence layer for events. All state is derived
//! from events, so the event store is the single source of truth.

use crate::core::events::{Event, EventId};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Errors that can occur in the event store.
#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Event not found: {0}")]
    NotFound(EventId),
}

/// Result type for event store operations.
pub type Result<T> = std::result::Result<T, EventStoreError>;

/// Filter for querying events.
#[derive(Debug, Default, Clone)]
pub struct EventFilter {
    /// Filter by project ID.
    pub project_id: Option<Uuid>,
    /// Filter by graph ID.
    pub graph_id: Option<Uuid>,
    /// Filter by task ID.
    pub task_id: Option<Uuid>,
    /// Filter by flow ID.
    pub flow_id: Option<Uuid>,
    /// Maximum number of events to return.
    pub limit: Option<usize>,
}

impl EventFilter {
    /// Creates an empty filter (matches all events).
    #[must_use]
    pub fn all() -> Self {
        Self::default()
    }

    /// Filter by project ID.
    #[must_use]
    pub fn for_project(project_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn for_graph(graph_id: Uuid) -> Self {
        Self {
            graph_id: Some(graph_id),
            ..Default::default()
        }
    }

    /// Checks if an event matches this filter.
    #[must_use]
    pub fn matches(&self, event: &Event) -> bool {
        if let Some(pid) = self.project_id {
            if event.metadata.correlation.project_id != Some(pid) {
                return false;
            }
        }
        if let Some(gid) = self.graph_id {
            if event.metadata.correlation.graph_id != Some(gid) {
                return false;
            }
        }
        if let Some(tid) = self.task_id {
            if event.metadata.correlation.task_id != Some(tid) {
                return false;
            }
        }
        if let Some(fid) = self.flow_id {
            if event.metadata.correlation.flow_id != Some(fid) {
                return false;
            }
        }
        true
    }
}

/// Trait for event storage backends.
pub trait EventStore: Send + Sync {
    /// Appends an event to the store, returning its assigned ID.
    fn append(&self, event: Event) -> Result<EventId>;

    /// Reads events matching the filter.
    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>>;

    /// Reads all events in order.
    fn read_all(&self) -> Result<Vec<Event>>;
}

/// In-memory event store for testing.
#[derive(Debug, Default)]
pub struct InMemoryEventStore {
    events: RwLock<Vec<Event>>,
}

impl InMemoryEventStore {
    /// Creates a new empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: RwLock::new(Vec::new()),
        }
    }
}

#[allow(clippy::significant_drop_tightening)]
impl EventStore for InMemoryEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        let mut events = self.events.write().expect("lock poisoned");
        event.metadata.sequence = Some(events.len() as u64);
        let id = event.id();
        events.push(event);
        Ok(id)
    }

    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        let events = self.events.read().expect("lock poisoned");
        let mut result: Vec<Event> = events
            .iter()
            .filter(|e| filter.matches(e))
            .cloned()
            .collect();
        if let Some(limit) = filter.limit {
            result.truncate(limit);
        }
        Ok(result)
    }

    fn read_all(&self) -> Result<Vec<Event>> {
        let events = self.events.read().expect("lock poisoned");
        Ok(events.clone())
    }
}

/// File-based event store (append-only JSON lines).
#[derive(Debug)]
pub struct FileEventStore {
    path: PathBuf,
    cache: RwLock<Vec<Event>>,
}

impl FileEventStore {
    /// Creates or opens a file-based event store.
    ///
    /// # Errors
    /// Returns an error if the file cannot be created or read.
    pub fn open(path: PathBuf) -> Result<Self> {
        let cache = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            content
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(serde_json::from_str)
                .collect::<std::result::Result<Vec<Event>, _>>()?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Vec::new()
        };

        Ok(Self {
            path,
            cache: RwLock::new(cache),
        })
    }

    /// Returns the path to the event file.
    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[allow(clippy::significant_drop_tightening)]
impl EventStore for FileEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut cache = self.cache.write().expect("lock poisoned");
        event.metadata.sequence = Some(cache.len() as u64);
        let id = event.id();

        let json = serde_json::to_string(&event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{json}")?;

        cache.push(event);
        Ok(id)
    }

    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        let cache = self.cache.read().expect("lock poisoned");
        let mut result: Vec<Event> = cache
            .iter()
            .filter(|e| filter.matches(e))
            .cloned()
            .collect();
        if let Some(limit) = filter.limit {
            result.truncate(limit);
        }
        Ok(result)
    }

    fn read_all(&self) -> Result<Vec<Event>> {
        let cache = self.cache.read().expect("lock poisoned");
        Ok(cache.clone())
    }
}

/// Thread-safe wrapper for any event store.
pub type SharedEventStore = Arc<dyn EventStore>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::{CorrelationIds, EventPayload};

    #[test]
    fn in_memory_store_append_and_read() {
        let store = InMemoryEventStore::new();
        let project_id = Uuid::new_v4();

        let event = Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "test".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        );

        let id = store.append(event).unwrap();
        let events = store.read_all().unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id(), id);
    }

    #[test]
    fn in_memory_store_filter_by_project() {
        let store = InMemoryEventStore::new();
        let project1 = Uuid::new_v4();
        let project2 = Uuid::new_v4();

        store
            .append(Event::new(
                EventPayload::ProjectCreated {
                    id: project1,
                    name: "p1".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project1),
            ))
            .unwrap();

        store
            .append(Event::new(
                EventPayload::ProjectCreated {
                    id: project2,
                    name: "p2".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project2),
            ))
            .unwrap();

        let filter = EventFilter::for_project(project1);
        let events = store.read(&filter).unwrap();

        assert_eq!(events.len(), 1);
    }

    #[test]
    fn in_memory_store_filter_by_graph() {
        let store = InMemoryEventStore::new();
        let project = Uuid::new_v4();
        let graph1 = Uuid::new_v4();
        let graph2 = Uuid::new_v4();

        store
            .append(Event::new(
                EventPayload::TaskGraphCreated {
                    graph_id: graph1,
                    project_id: project,
                    name: "g1".to_string(),
                    description: None,
                },
                CorrelationIds::for_graph(project, graph1),
            ))
            .unwrap();

        store
            .append(Event::new(
                EventPayload::TaskGraphCreated {
                    graph_id: graph2,
                    project_id: project,
                    name: "g2".to_string(),
                    description: None,
                },
                CorrelationIds::for_graph(project, graph2),
            ))
            .unwrap();

        let filter = EventFilter::for_graph(graph1);
        let events = store.read(&filter).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].metadata.correlation.graph_id, Some(graph1));
    }

    #[test]
    fn file_store_persist_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let project_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "persist-test".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        );

        // Write
        {
            let store = FileEventStore::open(path.clone()).unwrap();
            store.append(event.clone()).unwrap();
        }

        // Reload
        {
            let store = FileEventStore::open(path).unwrap();
            let events = store.read_all().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].payload, event.payload);
        }
    }
}
