use super::*;

impl Registry {
    pub(crate) fn governance_projects_dir(&self) -> PathBuf {
        self.config.data_dir.join("projects")
    }

    pub(crate) fn governance_project_root(&self, project_id: Uuid) -> PathBuf {
        self.governance_projects_dir().join(project_id.to_string())
    }

    pub(crate) fn governance_artifact_locations(
        &self,
        project_id: Uuid,
    ) -> Vec<GovernanceArtifactLocation> {
        let project_root = self.governance_project_root(project_id);
        let global_root = self.governance_global_root();
        vec![
            GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "constitution",
                artifact_key: "constitution.yaml".to_string(),
                is_dir: false,
                path: project_root.join("constitution.yaml"),
            },
            GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "documents",
                artifact_key: "documents".to_string(),
                is_dir: true,
                path: project_root.join("documents"),
            },
            GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "notepad",
                artifact_key: "notepad.md".to_string(),
                is_dir: false,
                path: project_root.join("notepad.md"),
            },
            GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "graph_snapshot",
                artifact_key: "graph_snapshot.json".to_string(),
                is_dir: false,
                path: project_root.join("graph_snapshot.json"),
            },
            GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "skills",
                artifact_key: "skills".to_string(),
                is_dir: true,
                path: global_root.join("skills"),
            },
            GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "system_prompts",
                artifact_key: "system_prompts".to_string(),
                is_dir: true,
                path: global_root.join("system_prompts"),
            },
            GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "templates",
                artifact_key: "templates".to_string(),
                is_dir: true,
                path: global_root.join("templates"),
            },
            GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "notepad",
                artifact_key: "notepad.md".to_string(),
                is_dir: false,
                path: global_root.join("notepad.md"),
            },
        ]
    }

    pub(crate) fn governance_default_file_contents(
        location: &GovernanceArtifactLocation,
    ) -> &'static [u8] {
        match (location.scope, location.artifact_kind) {
            ("project", "constitution") => b"version: 1\nschema_version: constitution.v1\ncompatibility:\n  minimum_hivemind_version: 0.1.32\n  governance_schema_version: governance.v1\npartitions: []\nrules: []\n",
            ("project", "graph_snapshot") => b"{}\n",
            _ => b"\n",
        }
    }

    pub(crate) fn ensure_governance_layout(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Vec<PathBuf>> {
        let mut created = Vec::new();
        for location in self.governance_artifact_locations(project_id) {
            if location.is_dir {
                if location.path.exists() {
                    if !location.path.is_dir() {
                        return Err(HivemindError::system(
                            "governance_path_conflict",
                            format!(
                                "Expected governance directory at '{}' but found a file",
                                location.path.display()
                            ),
                            origin,
                        ));
                    }
                } else {
                    fs::create_dir_all(&location.path).map_err(|e| {
                        HivemindError::system(
                            "governance_storage_create_failed",
                            e.to_string(),
                            origin,
                        )
                    })?;
                    created.push(location.path.clone());
                }
                continue;
            }

            if let Some(parent) = location.path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
                })?;
            }

            if location.path.exists() {
                if location.path.is_dir() {
                    return Err(HivemindError::system(
                        "governance_path_conflict",
                        format!(
                            "Expected governance file at '{}' but found a directory",
                            location.path.display()
                        ),
                        origin,
                    ));
                }
            } else {
                fs::write(
                    &location.path,
                    Self::governance_default_file_contents(&location),
                )
                .map_err(|e| {
                    HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
                })?;
                created.push(location.path.clone());
            }
        }
        Ok(created)
    }

    pub(crate) fn governance_graph_snapshot_location(
        &self,
        project_id: Uuid,
    ) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: Some(project_id),
            scope: "project",
            artifact_kind: "graph_snapshot",
            artifact_key: "graph_snapshot.json".to_string(),
            is_dir: false,
            path: self.graph_snapshot_path(project_id),
        }
    }
}
