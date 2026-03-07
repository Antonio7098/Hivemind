use super::*;

/// File-based event store (append-only JSON lines).
#[derive(Debug)]
pub struct FileEventStore {
    path: PathBuf,
    pub(crate) cache: RwLock<Vec<Event>>,
}
impl FileEventStore {
    /// Creates or opens a file-based event store.
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        file.lock_shared()?;

        let mut content = String::new();
        {
            use std::io::Read;
            let mut reader = std::io::BufReader::new(&file);
            reader.read_to_string(&mut content)?;
        }

        file.unlock()?;

        let cache = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .flat_map(|line| {
                let normalized = normalize_concatenated_json_objects(line);
                serde_json::Deserializer::from_str(&normalized)
                    .into_iter::<Event>()
                    .collect::<Vec<_>>()
            })
            .collect::<std::result::Result<Vec<Event>, _>>()?;

        Ok(Self {
            path,
            cache: RwLock::new(cache),
        })
    }

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

        let mut file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.lock_exclusive()?;

        let mut content = String::new();
        {
            use std::io::{Read, Seek};
            let _ = file.rewind();
            let mut reader = std::io::BufReader::new(&file);
            reader.read_to_string(&mut content)?;
        }

        let disk_events = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .flat_map(|line| {
                let normalized = normalize_concatenated_json_objects(line);
                serde_json::Deserializer::from_str(&normalized)
                    .into_iter::<Event>()
                    .collect::<Vec<_>>()
            })
            .collect::<std::result::Result<Vec<Event>, _>>()?;

        let mut cache = self.cache.write().expect("lock poisoned");
        cache.clone_from(&disk_events);

        let next_seq = cache.len() as u64;
        event.metadata.sequence = Some(next_seq);
        event.metadata.id = EventId::from_ordered_u64(next_seq);
        if let Some(last) = cache.last() {
            if event.metadata.timestamp <= last.metadata.timestamp {
                event.metadata.timestamp = last.metadata.timestamp + ChronoDuration::nanoseconds(1);
            }
        }
        let id = event.id();

        let json = serde_json::to_string(&event)?;
        writeln!(file, "{json}")?;
        let _ = file.flush();
        let _ = file.unlock();

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

    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        let (tx, rx) = mpsc::channel();
        let filter = filter.clone();
        let path = self.path.clone();

        thread::spawn(move || {
            let mut sent = 0usize;
            let mut seen = 0usize;

            loop {
                let Ok(content) = std::fs::read_to_string(&path) else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };

                let Ok(events) = content
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .flat_map(|line| {
                        let normalized = normalize_concatenated_json_objects(line);
                        serde_json::Deserializer::from_str(&normalized)
                            .into_iter::<Event>()
                            .collect::<Vec<_>>()
                    })
                    .collect::<std::result::Result<Vec<Event>, _>>()
                else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };

                for ev in events.iter().skip(seen) {
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

                seen = events.len();
                thread::sleep(Duration::from_millis(200));
            }
        });

        Ok(rx)
    }
}
