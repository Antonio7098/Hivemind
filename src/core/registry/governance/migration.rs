use super::*;

impl Registry {
    pub(crate) fn legacy_governance_mappings(
        &self,
        project: &Project,
    ) -> Vec<LegacyGovernanceArtifactMapping> {
        let mut mappings = Vec::new();
        let canonical = self.governance_artifact_locations(project.id);
        for repo in &project.repositories {
            let legacy_root = PathBuf::from(&repo.path).join(".hivemind");
            for destination in &canonical {
                let source = if destination.scope == "project" {
                    legacy_root.join(&destination.artifact_key)
                } else if destination.artifact_key == "notepad.md" {
                    legacy_root.join("global").join("notepad.md")
                } else {
                    legacy_root.join("global").join(&destination.artifact_key)
                };
                mappings.push(LegacyGovernanceArtifactMapping {
                    source,
                    destination: destination.clone(),
                });
            }
        }
        mappings
    }

    pub(crate) fn copy_dir_if_missing(
        source: &Path,
        destination: &Path,
        origin: &'static str,
    ) -> Result<bool> {
        if !source.exists() || !source.is_dir() {
            return Ok(false);
        }

        fs::create_dir_all(destination).map_err(|e| {
            HivemindError::system("governance_migration_failed", e.to_string(), origin)
        })?;

        let mut copied = false;
        let mut stack = vec![(source.to_path_buf(), destination.to_path_buf())];
        while let Some((src_dir, dst_dir)) = stack.pop() {
            for entry in fs::read_dir(&src_dir).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })? {
                let entry = entry.map_err(|e| {
                    HivemindError::system("governance_migration_failed", e.to_string(), origin)
                })?;
                let src_path = entry.path();
                let dst_path = dst_dir.join(entry.file_name());
                let file_type = entry.file_type().map_err(|e| {
                    HivemindError::system("governance_migration_failed", e.to_string(), origin)
                })?;
                if file_type.is_dir() {
                    fs::create_dir_all(&dst_path).map_err(|e| {
                        HivemindError::system("governance_migration_failed", e.to_string(), origin)
                    })?;
                    stack.push((src_path, dst_path));
                    continue;
                }

                if dst_path.exists() {
                    continue;
                }
                fs::copy(&src_path, &dst_path).map_err(|e| {
                    HivemindError::system("governance_migration_failed", e.to_string(), origin)
                })?;
                copied = true;
            }
        }

        Ok(copied)
    }

    pub(crate) fn copy_file_if_missing(
        source: &Path,
        destination: &Path,
        default_contents: Option<&[u8]>,
        origin: &'static str,
    ) -> Result<bool> {
        if !source.exists() || !source.is_file() {
            return Ok(false);
        }
        if destination.exists() {
            let source_bytes = fs::read(source).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })?;
            if source_bytes.is_empty() {
                return Ok(false);
            }

            let destination_bytes = fs::read(destination).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })?;
            if destination_bytes == source_bytes {
                return Ok(false);
            }

            let can_overwrite_scaffold = default_contents
                .is_some_and(|default_bytes| destination_bytes.as_slice() == default_bytes);
            if !destination_bytes.is_empty() && !can_overwrite_scaffold {
                return Ok(false);
            }

            fs::write(destination, source_bytes).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })?;
            return Ok(true);
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })?;
        }
        fs::copy(source, destination).map_err(|e| {
            HivemindError::system("governance_migration_failed", e.to_string(), origin)
        })?;
        Ok(true)
    }
}
