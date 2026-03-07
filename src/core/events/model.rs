use super::*;

/// Event metadata common to all events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Unique event identifier.
    pub id: EventId,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Correlation IDs for tracing.
    pub correlation: CorrelationIds,
    /// Sequence number within the event stream (assigned by store).
    pub sequence: Option<u64>,
}
impl EventMetadata {
    /// Creates new metadata with current timestamp.
    #[must_use]
    pub fn new(correlation: CorrelationIds) -> Self {
        Self {
            id: EventId::new(),
            timestamp: Utc::now(),
            correlation,
            sequence: None,
        }
    }
}
/// A complete event with metadata and payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Event metadata.
    pub metadata: EventMetadata,
    /// Event payload.
    pub payload: EventPayload,
}
impl Event {
    /// Creates a new event with the given payload and correlation.
    #[must_use]
    pub fn new(payload: EventPayload, correlation: CorrelationIds) -> Self {
        Self {
            metadata: EventMetadata::new(correlation),
            payload,
        }
    }

    /// Returns the event ID.
    #[must_use]
    pub fn id(&self) -> EventId {
        self.metadata.id
    }

    /// Returns the event timestamp.
    #[must_use]
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.metadata.timestamp
    }
}
