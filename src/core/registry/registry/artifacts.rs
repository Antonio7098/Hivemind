use super::*;

impl Registry {
    pub(crate) fn blobs_dir(&self) -> PathBuf {
        self.config.data_dir.join("blobs")
    }

    pub(crate) fn blob_path_for_digest(&self, digest: &str) -> PathBuf {
        let prefix = digest.get(..2).unwrap_or("00");
        self.blobs_dir()
            .join("sha256")
            .join(prefix)
            .join(format!("{digest}.blob"))
    }

    pub(crate) fn write_baseline_artifact(&self, baseline: &Baseline) -> Result<()> {
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

    pub(crate) fn read_baseline_artifact(&self, baseline_id: Uuid) -> Result<Baseline> {
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

    pub(crate) fn write_diff_artifact(&self, artifact: &DiffArtifact) -> Result<()> {
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

    pub(crate) fn read_diff_artifact(&self, diff_id: Uuid) -> Result<DiffArtifact> {
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

    pub(crate) fn unified_diff_for_change(
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
}
