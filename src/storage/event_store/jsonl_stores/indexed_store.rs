use super::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct RegistryIndexDisk {
    projects: HashMap<String, String>,
}
/// Event store that maintains per-project and per-flow append-only logs plus an index.
#[derive(Debug)]
pub struct IndexedEventStore {
    index_path: PathBuf,
    projects_dir: PathBuf,
    flows_dir: PathBuf,
    global: FileEventStore,
}
impl IndexedEventStore {
    /// Opens (or creates) an indexed store rooted at `base_dir`.
    pub fn open(base_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_dir)?;

        let index_path = base_dir.join("index.json");
        let projects_dir = base_dir.join("projects");
        let flows_dir = base_dir.join("flows");
        std::fs::create_dir_all(&projects_dir)?;
        std::fs::create_dir_all(&flows_dir)?;

        if !index_path.exists() {
            let disk = RegistryIndexDisk::default();
            std::fs::write(&index_path, serde_json::to_string_pretty(&disk)?)?;
        }

        let global = FileEventStore::open(base_dir.join("events.jsonl"))?;

        Ok(Self {
            index_path,
            projects_dir,
            flows_dir,
            global,
        })
    }

    fn project_log_rel(project_id: Uuid) -> String {
        format!("projects/{project_id}/events.jsonl")
    }

    fn flow_log_path(&self, flow_id: Uuid) -> PathBuf {
        self.flows_dir
            .join(flow_id.to_string())
            .join("events.jsonl")
    }

    fn ensure_project_index(&self, project_id: Uuid) -> Result<PathBuf> {
        use std::io::{Read, Seek, Write};

        let rel = Self::project_log_rel(project_id);
        let abs = self
            .projects_dir
            .join(project_id.to_string())
            .join("events.jsonl");

        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.index_path)?;
        file.lock_exclusive()?;

        let mut content = String::new();
        {
            let _ = file.rewind();
            let mut reader = std::io::BufReader::new(&file);
            reader.read_to_string(&mut content)?;
        }

        let mut disk: RegistryIndexDisk = if content.trim().is_empty() {
            RegistryIndexDisk::default()
        } else {
            serde_json::from_str(&content).unwrap_or_default()
        };

        disk.projects
            .entry(project_id.to_string())
            .or_insert_with(|| rel.clone());

        let json = serde_json::to_string_pretty(&disk)?;
        {
            let _ = file.rewind();
            file.set_len(0)?;
            file.write_all(json.as_bytes())?;
            let _ = file.flush();
        }
        let _ = file.unlock();

        Ok(abs)
    }

    fn append_mirror(path: &PathBuf, event: &Event) -> Result<()> {
        use std::io::Write;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(path)?;
        file.lock_exclusive()?;
        let json = serde_json::to_string(event)?;
        writeln!(file, "{json}")?;
        let _ = file.flush();
        let _ = file.unlock();
        Ok(())
    }
}
#[allow(clippy::significant_drop_tightening)]
impl EventStore for IndexedEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(self.global.path())?;
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

        let mut cache = self.global.cache.write().expect("lock poisoned");
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

        cache.push(event.clone());
        drop(cache);

        if let Some(project_id) = event.metadata.correlation.project_id {
            let project_path = self.ensure_project_index(project_id)?;
            Self::append_mirror(&project_path, &event)?;
        }
        if let Some(flow_id) = event.metadata.correlation.flow_id {
            let flow_path = self.flow_log_path(flow_id);
            Self::append_mirror(&flow_path, &event)?;
        }

        Ok(id)
    }

    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        self.global.read(filter)
    }

    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        self.global.stream(filter)
    }

    fn read_all(&self) -> Result<Vec<Event>> {
        self.global.read_all()
    }
}
