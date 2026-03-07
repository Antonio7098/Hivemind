use super::*;

impl Registry {
    pub(crate) fn verify_repository_scope(
        scope: &Scope,
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Vec<crate::core::enforcement::ScopeViolation> {
        if scope.repositories.is_empty() {
            return Vec::new();
        }

        let Ok(worktrees) = Self::inspect_task_worktrees(flow, state, task_id, origin) else {
            return vec![crate::core::enforcement::ScopeViolation::filesystem(
                "<worktree>",
                "Repository scope verification failed: worktree missing",
            )];
        };

        let mut violations = Vec::new();
        for (repo_name, status) in worktrees {
            let changed_paths = Self::parse_git_status_paths(&status.path);
            if changed_paths.is_empty() {
                continue;
            }

            let allowed_mode = scope
                .repositories
                .iter()
                .find(|r| r.repo == repo_name || r.repo == status.path.to_string_lossy())
                .map(|r| r.mode);

            match allowed_mode {
                Some(RepoAccessMode::ReadWrite) => {}
                Some(RepoAccessMode::ReadOnly) => {
                    violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                        format!("{repo_name}/{}", changed_paths[0]),
                        format!("Repository '{repo_name}' is read-only in scope"),
                    ));
                }
                None => {
                    violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                        format!("{repo_name}/{}", changed_paths[0]),
                        format!("Repository '{repo_name}' is not declared in task scope"),
                    ));
                }
            }
        }
        violations
    }

    pub(crate) fn verify_scope_environment_baseline(
        &self,
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        attempt_id: Uuid,
        origin: &'static str,
    ) -> Vec<crate::core::enforcement::ScopeViolation> {
        let Ok(baseline) = self.read_scope_baseline_artifact(attempt_id) else {
            return Vec::new();
        };
        let Ok(worktrees) = Self::inspect_task_worktrees(flow, state, task_id, origin) else {
            return Vec::new();
        };
        let allowed_roots: Vec<PathBuf> = worktrees
            .iter()
            .filter_map(|(_, status)| status.path.canonicalize().ok())
            .collect();

        let mut violations = Vec::new();
        for snapshot in &baseline.repo_snapshots {
            let repo_path = Path::new(&snapshot.repo_path);
            let current_head = Self::repo_git_head(repo_path);
            if current_head != snapshot.git_head {
                violations.push(crate::core::enforcement::ScopeViolation::git(format!(
                    "Repository HEAD changed outside task worktree (before: {:?}, after: {:?})",
                    snapshot.git_head, current_head
                )));
            }

            let current_status = Self::repo_status_lines(repo_path);
            if current_status != snapshot.status_lines {
                let path_preview = current_status
                    .first()
                    .and_then(|line| line.strip_prefix("?? ").or_else(|| line.get(3..)))
                    .map_or("<unknown>", str::trim);
                violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                    format!("{}:{path_preview}", snapshot.repo_path),
                    "Repository workspace changed outside task worktree",
                ));
            }
        }

        let current_tmp = Self::list_tmp_entries();
        let baseline_tmp: HashSet<String> = baseline.tmp_entries.into_iter().collect();
        for created in current_tmp
            .into_iter()
            .filter(|name| !baseline_tmp.contains(name))
            .take(32)
        {
            let path = PathBuf::from("/tmp").join(&created);
            let canonical = path.canonicalize().unwrap_or(path);
            if allowed_roots.iter().any(|root| canonical.starts_with(root)) {
                continue;
            }
            if Self::scope_trace_is_ignored(&canonical, None, &self.config.data_dir) {
                continue;
            }
            violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                canonical.to_string_lossy().to_string(),
                "Filesystem write outside task worktree detected in /tmp",
            ));
        }

        violations
    }

    pub(crate) fn verify_scope_trace_writes(
        &self,
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        attempt_id: Uuid,
        origin: &'static str,
    ) -> Vec<crate::core::enforcement::ScopeViolation> {
        let trace_path = self.scope_trace_path(attempt_id);
        let Ok(contents) = fs::read_to_string(&trace_path) else {
            return Vec::new();
        };

        let Ok(worktrees) = Self::inspect_task_worktrees(flow, state, task_id, origin) else {
            return Vec::new();
        };
        let allowed_roots: Vec<PathBuf> = worktrees
            .iter()
            .filter_map(|(_, status)| status.path.canonicalize().ok())
            .collect();
        let home_dir = env::var("HOME").ok().map(PathBuf::from);

        let mut violations = Vec::new();
        for observed in Self::parse_scope_trace_written_paths(&contents) {
            let observed_abs = if observed.is_absolute() {
                observed
            } else if let Some((_, first)) = worktrees.first() {
                first.path.join(observed)
            } else {
                continue;
            };

            let canonical = observed_abs
                .canonicalize()
                .unwrap_or_else(|_| observed_abs.clone());
            if Self::scope_trace_is_ignored(&canonical, home_dir.as_deref(), &self.config.data_dir)
            {
                continue;
            }
            if allowed_roots.iter().any(|root| canonical.starts_with(root)) {
                continue;
            }

            violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                canonical.to_string_lossy().to_string(),
                "Write outside task worktree detected via runtime syscall trace",
            ));
        }

        violations.sort_by(|a, b| a.path.cmp(&b.path));
        violations.dedup_by(|a, b| a.path == b.path);
        violations
    }
}
