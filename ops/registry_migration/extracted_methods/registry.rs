// AUTO-GENERATED from src/core/registry_original.rs
// Module bucket: registry

// prepare_runtime_environment (1185-1193)
    fn prepare_runtime_environment(
        runtime: &mut ProjectRuntimeConfig,
        origin: &'static str,
    ) -> Result<RuntimeEnvironmentProvenance> {
        let built = build_protected_runtime_environment(&runtime.env)
            .map_err(|error| Self::runtime_env_build_error(error, origin))?;
        runtime.env = built.env;
        Ok(built.provenance)
    }

// has_model_flag (1195-1199)
    fn has_model_flag(args: &[String], short_alias: bool) -> bool {
        args.iter().any(|arg| {
            arg == "--model" || arg.starts_with("--model=") || (short_alias && arg == "-m")
        })
    }

// blobs_dir (1841-1843)
    fn blobs_dir(&self) -> PathBuf {
        self.config.data_dir.join("blobs")
    }

// blob_path_for_digest (1845-1851)
    fn blob_path_for_digest(&self, digest: &str) -> PathBuf {
        let prefix = digest.get(..2).unwrap_or("00");
        self.blobs_dir()
            .join("sha256")
            .join(prefix)
            .join(format!("{digest}.blob"))
    }

// write_baseline_artifact (5965-6013)
    fn write_baseline_artifact(&self, baseline: &Baseline) -> Result<()> {
        let files_dir = self.baseline_files_dir(baseline.id);
        fs::create_dir_all(&files_dir).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_baseline_artifact",
            )
        })?;

        let json = serde_json::to_vec_pretty(baseline).map_err(|e| {
            HivemindError::system(
                "artifact_serialize_failed",
                e.to_string(),
                "registry:write_baseline_artifact",
            )
        })?;
        fs::write(self.baseline_json_path(baseline.id), json).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_baseline_artifact",
            )
        })?;

        for snapshot in baseline.files.values() {
            if snapshot.is_dir {
                continue;
            }

            let src = baseline.root.join(&snapshot.path);
            let dst = files_dir.join(&snapshot.path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    HivemindError::system(
                        "artifact_write_failed",
                        e.to_string(),
                        "registry:write_baseline_artifact",
                    )
                })?;
            }

            let Ok(contents) = fs::read(src) else {
                continue;
            };
            let _ = fs::write(dst, contents);
        }
        Ok(())
    }

// read_baseline_artifact (6015-6030)
    fn read_baseline_artifact(&self, baseline_id: Uuid) -> Result<Baseline> {
        let bytes = fs::read(self.baseline_json_path(baseline_id)).map_err(|e| {
            HivemindError::system(
                "artifact_read_failed",
                e.to_string(),
                "registry:read_baseline_artifact",
            )
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            HivemindError::system(
                "artifact_deserialize_failed",
                e.to_string(),
                "registry:read_baseline_artifact",
            )
        })
    }

// write_diff_artifact (6032-6055)
    fn write_diff_artifact(&self, artifact: &DiffArtifact) -> Result<()> {
        fs::create_dir_all(self.diffs_dir()).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_diff_artifact",
            )
        })?;
        let json = serde_json::to_vec_pretty(artifact).map_err(|e| {
            HivemindError::system(
                "artifact_serialize_failed",
                e.to_string(),
                "registry:write_diff_artifact",
            )
        })?;
        fs::write(self.diff_json_path(artifact.diff.id), json).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_diff_artifact",
            )
        })?;
        Ok(())
    }

// read_diff_artifact (6057-6072)
    fn read_diff_artifact(&self, diff_id: Uuid) -> Result<DiffArtifact> {
        let bytes = fs::read(self.diff_json_path(diff_id)).map_err(|e| {
            HivemindError::system(
                "artifact_read_failed",
                e.to_string(),
                "registry:read_diff_artifact",
            )
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            HivemindError::system(
                "artifact_deserialize_failed",
                e.to_string(),
                "registry:read_diff_artifact",
            )
        })
    }

// unified_diff_for_change (6074-6089)
    fn unified_diff_for_change(
        &self,
        baseline_id: Uuid,
        worktree_root: &std::path::Path,
        change: &FileChange,
    ) -> std::io::Result<String> {
        let baseline_files = self.baseline_files_dir(baseline_id);
        let old = baseline_files.join(&change.path);
        let new = worktree_root.join(&change.path);

        match change.change_type {
            ChangeType::Created => unified_diff(None, Some(&new)),
            ChangeType::Deleted => unified_diff(Some(&old), None),
            ChangeType::Modified => unified_diff(Some(&old), Some(&new)),
        }
    }

// open (6277-6283)
    /// Opens or creates a registry at the default location.
    ///
    /// # Errors
    /// Returns an error if the event store cannot be opened.
    pub fn open() -> Result<Self> {
        Self::open_with_config(RegistryConfig::default_dir())
    }

// open_with_config (6285-6298)
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

// with_store (6300-6304)
    /// Creates a registry with a custom event store (for testing).
    #[must_use]
    pub fn with_store(store: Arc<dyn EventStore>, config: RegistryConfig) -> Self {
        Self { store, config }
    }

// state (6306-6315)
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

// list_events (6317-6329)
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

// create_project (6877-6934)
    /// Creates a new project.
    ///
    /// # Errors
    /// Returns an error if a project with that name already exists.
    pub fn create_project(&self, name: &str, description: Option<&str>) -> Result<Project> {
        if name.trim().is_empty() {
            let err = HivemindError::user(
                "invalid_project_name",
                "Project name cannot be empty",
                "registry:create_project",
            )
            .with_hint("Provide a non-empty project name");
            self.record_error_event(&err, CorrelationIds::none());
            return Err(err);
        }

        let state = self.state()?;

        // Check for duplicate name
        if state.projects.values().any(|p| p.name == name) {
            let err = HivemindError::user(
                "project_exists",
                format!("Project '{name}' already exists"),
                "registry:create_project",
            )
            .with_hint("Choose a different project name");
            self.record_error_event(&err, CorrelationIds::none());
            return Err(err);
        }

        let id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::ProjectCreated {
                id,
                name: name.to_string(),
                description: description.map(String::from),
            },
            CorrelationIds::for_project(id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:create_project",
            )
        })?;

        // Return the created project by replaying
        let new_state = self.state()?;
        new_state.projects.get(&id).cloned().ok_or_else(|| {
            HivemindError::system(
                "project_not_found_after_create",
                "Project was not found after creation",
                "registry:create_project",
            )
        })
    }

// list_projects (6936-6945)
    /// Lists all projects.
    ///
    /// # Errors
    /// Returns an error if state cannot be read.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let state = self.state()?;
        let mut projects: Vec<_> = state.projects.into_values().collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(projects)
    }

// get_project (6947-6975)
    /// Gets a project by ID or name.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn get_project(&self, id_or_name: &str) -> Result<Project> {
        let state = self.state()?;

        // Try parsing as UUID first
        if let Ok(id) = Uuid::parse_str(id_or_name) {
            if let Some(project) = state.projects.get(&id) {
                return Ok(project.clone());
            }
        }

        // Search by name
        state
            .projects
            .values()
            .find(|p| p.name == id_or_name)
            .cloned()
            .ok_or_else(|| {
                HivemindError::user(
                    "project_not_found",
                    format!("Project '{id_or_name}' not found"),
                    "registry:get_project",
                )
                .with_hint("Use 'hivemind project list' to see available projects")
            })
    }

// update_project (6977-7047)
    /// Updates a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn update_project(
        &self,
        id_or_name: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if let Some(new_name) = name {
            if new_name.trim().is_empty() {
                let err = HivemindError::user(
                    "invalid_project_name",
                    "Project name cannot be empty",
                    "registry:update_project",
                )
                .with_hint("Provide a non-empty project name");
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
        }

        let name = name.filter(|n| *n != project.name);
        let description = description.filter(|d| project.description.as_deref() != Some(*d));

        if name.is_none() && description.is_none() {
            return Ok(project);
        }

        // Check for name conflict if changing name
        if let Some(new_name) = name {
            let state = self.state()?;
            if state
                .projects
                .values()
                .any(|p| p.name == new_name && p.id != project.id)
            {
                let err = HivemindError::user(
                    "project_name_conflict",
                    format!("Project name '{new_name}' is already taken"),
                    "registry:update_project",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
        }

        let event = Event::new(
            EventPayload::ProjectUpdated {
                id: project.id,
                name: name.map(String::from),
                description: description.map(String::from),
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:update_project",
            )
        })?;

        self.get_project(&project.id.to_string())
    }

// delete_project (7049-7134)
    /// Deletes a project.
    ///
    /// # Errors
    /// Returns an error if the project still has tasks, graphs, or flows.
    pub fn delete_project(&self, id_or_name: &str) -> Result<Uuid> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let task_count = state
            .tasks
            .values()
            .filter(|task| task.project_id == project.id)
            .count();
        if task_count > 0 {
            let err = HivemindError::user(
                "project_has_tasks",
                format!(
                    "Project '{}' cannot be deleted while it still has tasks",
                    project.name
                ),
                "registry:delete_project",
            )
            .with_context("task_count", task_count.to_string())
            .with_hint("Delete project tasks first");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let graph_count = state
            .graphs
            .values()
            .filter(|graph| graph.project_id == project.id)
            .count();
        if graph_count > 0 {
            let err = HivemindError::user(
                "project_has_graphs",
                format!(
                    "Project '{}' cannot be deleted while it still has graphs",
                    project.name
                ),
                "registry:delete_project",
            )
            .with_context("graph_count", graph_count.to_string())
            .with_hint("Delete project graphs first");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let flow_count = state
            .flows
            .values()
            .filter(|flow| flow.project_id == project.id)
            .count();
        if flow_count > 0 {
            let err = HivemindError::user(
                "project_has_flows",
                format!(
                    "Project '{}' cannot be deleted while it still has flows",
                    project.name
                ),
                "registry:delete_project",
            )
            .with_context("flow_count", flow_count.to_string())
            .with_hint("Delete project flows first");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::ProjectDeleted {
                project_id: project.id,
            },
            CorrelationIds::for_project(project.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:delete_project",
            )
        })?;

        Ok(project.id)
    }

// config (9482-9486)
    /// Returns the registry configuration.
    #[must_use]
    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

