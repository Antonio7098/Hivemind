use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn ensure_graph_snapshot_current_for_constitution(
        &self,
        project: &Project,
        origin: &'static str,
    ) -> Result<()> {
        if project.repositories.is_empty() {
            return Ok(());
        }

        let artifact = self
            .read_graph_snapshot_artifact(project.id, origin)?
            .ok_or_else(|| {
                HivemindError::user(
                    "graph_snapshot_missing",
                    "Graph snapshot is missing for this project",
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>")
            })?;

        if artifact.profile_version != CODEGRAPH_PROFILE_MARKER {
            return Err(HivemindError::system(
                "graph_snapshot_profile_mismatch",
                format!(
                    "Snapshot profile version '{}' is not supported; expected '{}'",
                    artifact.profile_version, CODEGRAPH_PROFILE_MARKER
                ),
                origin,
            )
            .with_hint("Run: hivemind graph snapshot refresh <project>"));
        }

        let mut snapshot_repo_keys = HashSet::new();
        for repo in &artifact.repositories {
            snapshot_repo_keys.insert((repo.repo_name.clone(), repo.repo_path.clone()));
            let document = repo.document.to_document().map_err(|e| {
                HivemindError::system(
                    "graph_snapshot_schema_invalid",
                    format!("Stored graph snapshot document is invalid: {e}"),
                    origin,
                )
            })?;
            let computed = canonical_fingerprint(&document).map_err(|e| {
                HivemindError::system(
                    "graph_snapshot_integrity_check_failed",
                    format!("Failed to verify stored graph snapshot fingerprint: {e}"),
                    origin,
                )
            })?;
            if computed != repo.canonical_fingerprint {
                return Err(HivemindError::system(
                    "graph_snapshot_integrity_invalid",
                    format!(
                        "Stored fingerprint mismatch for repository '{}'",
                        repo.repo_name
                    ),
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>"));
            }
        }

        let aggregate = Self::compute_snapshot_fingerprint(&artifact.repositories);
        if aggregate != artifact.canonical_fingerprint {
            return Err(HivemindError::system(
                    "graph_snapshot_integrity_invalid",
                    "Stored aggregate graph snapshot fingerprint does not match repository fingerprints",
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>"));
        }

        let head_by_repo: HashMap<(String, String), String> = artifact
            .provenance
            .head_commits
            .iter()
            .map(|commit| {
                (
                    (commit.repo_name.clone(), commit.repo_path.clone()),
                    commit.commit_hash.clone(),
                )
            })
            .collect();

        for repo in &project.repositories {
            let key = (repo.name.clone(), repo.path.clone());
            if !snapshot_repo_keys.contains(&key) {
                return Err(HivemindError::user(
                    "graph_snapshot_stale",
                    format!(
                        "Graph snapshot does not include attached repository '{}'",
                        repo.name
                    ),
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>"));
            }
            let recorded_head = head_by_repo.get(&key).ok_or_else(|| {
                HivemindError::user(
                    "graph_snapshot_stale",
                    format!(
                        "Graph snapshot missing commit provenance for repository '{}'",
                        repo.name
                    ),
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>")
            })?;
            let current_head = Self::resolve_repo_head_commit(Path::new(&repo.path), origin)?;
            if recorded_head != &current_head {
                return Err(HivemindError::user(
                    "graph_snapshot_stale",
                    format!(
                        "Graph snapshot is stale for repository '{}' (snapshot={} current={})",
                        repo.name, recorded_head, current_head
                    ),
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>"));
            }
        }

        Ok(())
    }
}
