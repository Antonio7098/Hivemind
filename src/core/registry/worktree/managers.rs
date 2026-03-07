use super::*;

impl Registry {
    pub(crate) fn worktree_error_to_hivemind(
        err: WorktreeError,
        origin: &'static str,
    ) -> HivemindError {
        match err {
            WorktreeError::InvalidRepo(path) => HivemindError::git(
                "invalid_repo",
                format!("Invalid git repository: {}", path.display()),
                origin,
            ),
            WorktreeError::GitError(msg) => HivemindError::git("git_worktree_failed", msg, origin),
            WorktreeError::AlreadyExists(task_id) => HivemindError::user(
                "worktree_already_exists",
                format!("Worktree already exists for task {task_id}"),
                origin,
            ),
            WorktreeError::NotFound(id) => HivemindError::user(
                "worktree_not_found",
                format!("Worktree not found: {id}"),
                origin,
            ),
            WorktreeError::IoError(e) => {
                HivemindError::system("worktree_io_error", e.to_string(), origin)
            }
        }
    }

    pub(crate) fn worktree_managers_for_flow(
        flow: &TaskFlow,
        state: &AppState,
        origin: &'static str,
    ) -> Result<Vec<(String, WorktreeManager)>> {
        let project = Self::project_for_flow(flow, state)?;

        if project.repositories.is_empty() {
            return Err(HivemindError::user(
                "project_has_no_repo",
                "Project has no repository attached",
                origin,
            )
            .with_hint("Attach a repo via 'hivemind project attach-repo <project> <path>'"));
        }

        project
            .repositories
            .iter()
            .map(|repo| {
                WorktreeManager::new(PathBuf::from(&repo.path), WorktreeConfig::default())
                    .map(|manager| (repo.name.clone(), manager))
                    .map_err(|e| Self::worktree_error_to_hivemind(e, origin))
            })
            .collect()
    }

    pub(crate) fn worktree_manager_for_flow(
        flow: &TaskFlow,
        state: &AppState,
    ) -> Result<WorktreeManager> {
        let managers =
            Self::worktree_managers_for_flow(flow, state, "registry:worktree_manager_for_flow")?;
        managers
            .into_iter()
            .next()
            .map(|(_, manager)| manager)
            .ok_or_else(|| {
                HivemindError::user(
                    "project_has_no_repo",
                    "Project has no repository attached",
                    "registry:worktree_manager_for_flow",
                )
            })
    }
}
