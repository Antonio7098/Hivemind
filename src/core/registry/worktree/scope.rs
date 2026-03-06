use super::*;

impl Registry {
    pub(crate) fn list_tmp_entries() -> Vec<String> {
        let Ok(entries) = fs::read_dir("/tmp") else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub(crate) fn write_scope_baseline_artifact(
        &self,
        artifact: &ScopeBaselineArtifact,
    ) -> Result<()> {
        fs::create_dir_all(self.scope_baselines_dir()).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_scope_baseline_artifact",
            )
        })?;
        let bytes = serde_json::to_vec_pretty(artifact).map_err(|e| {
            HivemindError::system(
                "artifact_serialize_failed",
                e.to_string(),
                "registry:write_scope_baseline_artifact",
            )
        })?;
        fs::write(self.scope_baseline_path(artifact.attempt_id), bytes).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_scope_baseline_artifact",
            )
        })?;
        Ok(())
    }

    pub(crate) fn read_scope_baseline_artifact(
        &self,
        attempt_id: Uuid,
    ) -> Result<ScopeBaselineArtifact> {
        let bytes = fs::read(self.scope_baseline_path(attempt_id)).map_err(|e| {
            HivemindError::system(
                "artifact_read_failed",
                e.to_string(),
                "registry:read_scope_baseline_artifact",
            )
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            HivemindError::system(
                "artifact_deserialize_failed",
                e.to_string(),
                "registry:read_scope_baseline_artifact",
            )
        })
    }

    pub(crate) fn capture_scope_baseline_for_attempt(
        &self,
        flow: &TaskFlow,
        state: &AppState,
        attempt_id: Uuid,
    ) -> Result<()> {
        let repo_snapshots = state
            .projects
            .get(&flow.project_id)
            .map(|project| {
                project
                    .repositories
                    .iter()
                    .map(|repo| ScopeRepoSnapshot {
                        repo_path: repo.path.clone(),
                        git_head: Self::repo_git_head(Path::new(&repo.path)),
                        status_lines: Self::repo_status_lines(Path::new(&repo.path)),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let artifact = ScopeBaselineArtifact {
            attempt_id,
            repo_snapshots,
            tmp_entries: Self::list_tmp_entries(),
        };
        self.write_scope_baseline_artifact(&artifact)
    }

    pub(crate) fn parse_scope_trace_written_paths(trace_contents: &str) -> Vec<PathBuf> {
        fn first_quoted_segment(line: &str) -> Option<String> {
            let start = line.find('"')?;
            let mut escaped = false;
            let mut out = String::new();
            for ch in line[start + 1..].chars() {
                if escaped {
                    out.push(ch);
                    escaped = false;
                    continue;
                }
                match ch {
                    '\\' => escaped = true,
                    '"' => return Some(out),
                    _ => out.push(ch),
                }
            }
            None
        }

        let mut paths = Vec::new();
        for line in trace_contents.lines() {
            let is_open = line.contains("open(") || line.contains("openat(");
            if !is_open {
                continue;
            }
            let has_write_intent = line.contains("O_WRONLY")
                || line.contains("O_RDWR")
                || line.contains("O_CREAT")
                || line.contains("O_TRUNC")
                || line.contains("O_APPEND");
            if !has_write_intent {
                continue;
            }
            let Some(path) = first_quoted_segment(line).filter(|p| !p.is_empty()) else {
                continue;
            };
            paths.push(PathBuf::from(path));
        }

        paths.sort();
        paths.dedup();
        paths
    }

    pub(crate) fn scope_trace_is_ignored(
        path: &Path,
        home: Option<&Path>,
        data_dir: &Path,
    ) -> bool {
        let ignored_roots = [
            Path::new("/dev"),
            Path::new("/proc"),
            Path::new("/sys"),
            Path::new("/run"),
        ];
        if ignored_roots.iter().any(|root| path.starts_with(root)) {
            return true;
        }
        if path.starts_with(data_dir) {
            return true;
        }
        if let Some(home_dir) = home {
            if path.starts_with(home_dir.join(".config"))
                || path.starts_with(home_dir.join(".cache"))
                || path.starts_with(home_dir.join(".local/share"))
                || path.starts_with(home_dir.join(".npm"))
                || path.starts_with(home_dir.join(".bun"))
            {
                return true;
            }
        }
        false
    }
}
