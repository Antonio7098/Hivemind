// AUTO-GENERATED non-Registry impl blocks

// RegistryConfig (95-127)
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

// ConstitutionRule (687-695)
impl ConstitutionRule {
    fn id(&self) -> &str {
        match self {
            Self::ForbiddenDependency { id, .. }
            | Self::AllowedDependency { id, .. }
            | Self::CoverageRequirement { id, .. } => id,
        }
    }
}

// SelectedRuntimeAdapter (1114-1168)
impl SelectedRuntimeAdapter {
    fn initialize(&mut self) -> std::result::Result<(), RuntimeError> {
        match self {
            Self::OpenCode(a) => a.initialize(),
            Self::Codex(a) => a.initialize(),
            Self::ClaudeCode(a) => a.initialize(),
            Self::Kilo(a) => a.initialize(),
            Self::Native(a) => a.initialize(),
        }
    }

    fn prepare(
        &mut self,
        task_id: Uuid,
        worktree: &std::path::Path,
    ) -> std::result::Result<(), RuntimeError> {
        match self {
            Self::OpenCode(a) => a.prepare(task_id, worktree),
            Self::Codex(a) => a.prepare(task_id, worktree),
            Self::ClaudeCode(a) => a.prepare(task_id, worktree),
            Self::Kilo(a) => a.prepare(task_id, worktree),
            Self::Native(a) => a.prepare(task_id, worktree),
        }
    }

    fn execute(
        &mut self,
        input: ExecutionInput,
    ) -> std::result::Result<crate::adapters::runtime::ExecutionReport, RuntimeError> {
        match self {
            Self::OpenCode(a) => a.execute(input),
            Self::Codex(a) => a.execute(input),
            Self::ClaudeCode(a) => a.execute(input),
            Self::Kilo(a) => a.execute(input),
            Self::Native(a) => a.execute(input),
        }
    }

    fn execute_interactive<F>(
        &mut self,
        input: &ExecutionInput,
        on_event: F,
    ) -> std::result::Result<InteractiveExecutionResult, RuntimeError>
    where
        F: FnMut(InteractiveAdapterEvent) -> std::result::Result<(), String>,
    {
        match self {
            Self::OpenCode(a) => a.execute_interactive(input, on_event),
            Self::Codex(a) => a.execute_interactive(input, on_event),
            Self::ClaudeCode(a) => a.execute_interactive(input, on_event),
            Self::Kilo(a) => a.execute_interactive(input, on_event),
            Self::Native(a) => a.execute_interactive(input, on_event),
        }
    }
}
