use super::*;

/// In-memory event store for testing.
#[derive(Debug, Default)]
pub struct InMemoryEventStore {
    events: Arc<RwLock<Vec<Event>>>,
}
impl InMemoryEventStore {
    /// Creates a new empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }
}
#[allow(clippy::significant_drop_tightening)]
impl EventStore for InMemoryEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        let mut events = self.events.write().expect("lock poisoned");
        let next_seq = events.len() as u64;
        event.metadata.sequence = Some(next_seq);
        event.metadata.id = EventId::from_ordered_u64(next_seq);
        if let Some(last) = events.last() {
            if event.metadata.timestamp <= last.metadata.timestamp {
                event.metadata.timestamp = last.metadata.timestamp + ChronoDuration::nanoseconds(1);
            }
        }
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

    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        let (tx, rx) = mpsc::channel();
        let filter = filter.clone();
        let events = Arc::clone(&self.events);

        thread::spawn(move || {
            let mut sent = 0usize;
            let mut seen = 0usize;

            loop {
                let snapshot = {
                    let guard = events.read().expect("lock poisoned");
                    guard.clone()
                };

                for ev in snapshot.iter().skip(seen) {
                    if !filter.matches(ev) {
                        continue;
                    }

                    if tx.send(ev.clone()).is_err() {
                        return;
                    }

                    sent += 1;
                    if let Some(limit) = filter.limit {
                        if sent >= limit {
                            return;
                        }
                    }
                }

                seen = snapshot.len();
                thread::sleep(Duration::from_millis(200));
            }
        });

        Ok(rx)
    }
}
