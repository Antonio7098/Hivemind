use super::*;

impl Registry {
    pub(crate) fn prepare_runtime_environment(
        runtime: &mut ProjectRuntimeConfig,
        origin: &'static str,
    ) -> Result<RuntimeEnvironmentProvenance> {
        let built = build_protected_runtime_environment(&runtime.env)
            .map_err(|error| Self::runtime_env_build_error(error, origin))?;
        runtime.env = built.env;
        Ok(built.provenance)
    }

    pub(crate) fn has_model_flag(args: &[String], short_alias: bool) -> bool {
        args.iter().any(|arg| {
            arg == "--model" || arg.starts_with("--model=") || (short_alias && arg == "-m")
        })
    }

    /// Opens or creates a registry at the default location.
    ///
    /// # Errors
    /// Returns an error if the event store cannot be opened.
    pub fn open() -> Result<Self> {
        Self::open_with_config(RegistryConfig::default_dir())
    }

    /// Opens or creates a registry with custom config.
    ///
    /// # Errors
    /// Returns an error if the event store cannot be opened.
    pub fn open_with_config(config: RegistryConfig) -> Result<Self> {
        let store = SqliteEventStore::open(&config.data_dir).map_err(|e| {
            HivemindError::system("store_open_failed", e.to_string(), "registry:open")
        })?;

        Ok(Self {
            store: Arc::new(store),
            config,
        })
    }

    /// Creates a registry with a custom event store (for testing).
    #[must_use]
    pub fn with_store(store: Arc<dyn EventStore>, config: RegistryConfig) -> Self {
        Self { store, config }
    }

    /// Gets the current state by replaying all events.
    ///
    /// # Errors
    /// Returns an error if events cannot be read.
    pub fn state(&self) -> Result<AppState> {
        let events = self.store.read_all().map_err(|e| {
            HivemindError::system("state_read_failed", e.to_string(), "registry:state")
        })?;
        Ok(AppState::replay(&events))
    }

    /// Lists events in the store.
    ///
    /// # Errors
    /// Returns an error if events cannot be read.
    pub fn list_events(&self, project_id: Option<Uuid>, limit: usize) -> Result<Vec<Event>> {
        let mut filter = EventFilter::all();
        filter.project_id = project_id;
        filter.limit = Some(limit);

        self.store.read(&filter).map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:list_events")
        })
    }

    /// Returns the registry configuration.
    #[must_use]
    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }
}
