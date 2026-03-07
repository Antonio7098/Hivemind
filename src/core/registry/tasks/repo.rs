use super::*;

impl Registry {
    /// Attaches a repository to a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found or the path is not a valid git repository.
    pub fn attach_repo(
        &self,
        id_or_name: &str,
        path: &str,
        name: Option<&str>,
        access_mode: RepoAccessMode,
    ) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let path = path.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        let path_buf = std::path::PathBuf::from(path);

        if path.is_empty() {
            let err = HivemindError::user(
                "invalid_repository_path",
                "Repository path cannot be empty",
                "registry:attach_repo",
            )
            .with_hint("Provide a valid filesystem path to a git repository");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        if !path_buf.exists() {
            let err = HivemindError::user(
                "repo_path_not_found",
                format!("Repository path '{path}' not found"),
                "registry:attach_repo",
            )
            .with_hint("Provide an existing path to a git repository");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let git_dir = path_buf.join(".git");
        if !git_dir.exists() {
            let err = HivemindError::git(
                "not_a_git_repo",
                format!("'{path}' is not a git repository"),
                "registry:attach_repo",
            )
            .with_hint("Provide a path to a directory containing a .git folder");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let canonical_path = path_buf
            .canonicalize()
            .map_err(|e| {
                let err = HivemindError::system(
                    "path_canonicalize_failed",
                    e.to_string(),
                    "registry:attach_repo",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                err
            })?
            .to_string_lossy()
            .to_string();

        if project
            .repositories
            .iter()
            .any(|r| r.path == canonical_path)
        {
            let err = HivemindError::user(
                "repo_already_attached",
                format!("Repository '{path}' is already attached to this project"),
                "registry:attach_repo",
            )
            .with_hint(
                "Use 'hivemind project inspect <project>' to view attached repos or detach the existing one with 'hivemind project detach-repo <project> <repo-name>'",
            );
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let repo_name = name
            .map(ToString::to_string)
            .or_else(|| {
                path_buf
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "repo".to_string());

        if project.repositories.iter().any(|r| r.name == repo_name) {
            let err = HivemindError::user(
                "repo_name_already_attached",
                format!(
                    "Repository name '{repo_name}' is already attached to project '{}'",
                    project.name
                ),
                "registry:attach_repo",
            )
            .with_hint("Use --name to provide a different repository name");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::RepositoryAttached {
                project_id: project.id,
                path: canonical_path,
                name: repo_name,
                access_mode,
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:attach_repo")
        })?;

        self.trigger_graph_snapshot_refresh(project.id, "project_attach", "registry:attach_repo");

        self.get_project(&project.id.to_string())
    }

    /// Detaches a repository from a project.
    ///
    /// # Errors
    /// Returns an error if the project or repository is not found.
    pub fn detach_repo(&self, id_or_name: &str, repo_name: &str) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let state = self.state()?;
        let has_active_flow = state.flows.values().any(|flow| {
            flow.project_id == project.id
                && !matches!(
                    flow.state,
                    FlowState::Completed | FlowState::Merged | FlowState::Aborted
                )
        });
        if has_active_flow {
            let err = HivemindError::user(
                "project_in_active_flow",
                "Cannot detach repositories while project has active flows",
                "registry:detach_repo",
            )
            .with_hint("Abort, complete, or merge all active flows before detaching repositories");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        if !project.repositories.iter().any(|r| r.name == repo_name) {
            let err = HivemindError::user(
                "repo_not_found",
                format!(
                    "Repository '{repo_name}' is not attached to project '{}'",
                    project.name
                ),
                "registry:detach_repo",
            )
            .with_hint("Use 'hivemind project inspect' to see attached repositories");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::RepositoryDetached {
                project_id: project.id,
                name: repo_name.to_string(),
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:detach_repo")
        })?;

        self.get_project(&project.id.to_string())
    }
}
