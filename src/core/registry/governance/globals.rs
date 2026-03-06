use super::*;

impl Registry {
    pub fn governance_global_root(&self) -> PathBuf {
        self.config.data_dir.join("global")
    }

    pub(crate) fn ensure_global_governance_layout(&self, origin: &'static str) -> Result<()> {
        let global_root = self.governance_global_root();
        fs::create_dir_all(global_root.join("skills")).map_err(|e| {
            HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
        })?;
        fs::create_dir_all(global_root.join("system_prompts")).map_err(|e| {
            HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
        })?;
        fs::create_dir_all(global_root.join("templates")).map_err(|e| {
            HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
        })?;
        let notepad = global_root.join("notepad.md");
        if !notepad.exists() {
            fs::write(&notepad, b"\n").map_err(|e| {
                HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
            })?;
        }
        Ok(())
    }

    pub(crate) fn governance_global_location(
        artifact_kind: &'static str,
        artifact_key: &str,
        path: PathBuf,
    ) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: None,
            scope: "global",
            artifact_kind,
            artifact_key: artifact_key.to_string(),
            is_dir: false,
            path,
        }
    }
}
