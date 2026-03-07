use super::*;

impl RegistryConfig {
    /// Creates a new config with default data directory.
    #[must_use]
    pub fn default_dir() -> Self {
        if let Ok(data_dir) = env::var("HIVEMIND_DATA_DIR") {
            return Self {
                data_dir: PathBuf::from(data_dir),
            };
        }

        let data_dir =
            dirs::home_dir().map_or_else(|| PathBuf::from(".hivemind"), |h| h.join(".hivemind"));
        Self { data_dir }
    }

    /// Creates a config with custom data directory.
    #[must_use]
    pub fn with_dir(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Returns the path to the legacy events JSONL mirror file.
    #[must_use]
    pub fn events_path(&self) -> PathBuf {
        self.data_dir.join("events.jsonl")
    }

    /// Returns the path to the canonical `SQLite` database file.
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("db.sqlite")
    }
}
