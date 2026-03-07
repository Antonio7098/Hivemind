use super::*;

pub(crate) fn normalize_concatenated_json_objects(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(c) = chars.next() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        }

        out.push(c);

        if !in_string && c == '}' && chars.peek().copied() == Some('{') {
            out.push('\n');
        }
    }

    out
}
pub(crate) fn timestamp_to_nanos(timestamp: DateTime<Utc>) -> i64 {
    timestamp
        .timestamp_nanos_opt()
        .unwrap_or_else(|| timestamp.timestamp_micros().saturating_mul(1_000))
}
pub(crate) fn nanos_to_timestamp(nanos: i64) -> DateTime<Utc> {
    let seconds = nanos.div_euclid(1_000_000_000);
    let subsec_nanos = u32::try_from(nanos.rem_euclid(1_000_000_000)).unwrap_or(0);
    DateTime::<Utc>::from_timestamp(seconds, subsec_nanos).unwrap_or_else(Utc::now)
}
/// Errors that can occur in the event store.
#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invariant violation: {0}")]
    Invariant(String),
    #[error("Event not found: {0}")]
    NotFound(EventId),
}
/// Result type for event store operations.
pub type Result<T> = std::result::Result<T, EventStoreError>;
