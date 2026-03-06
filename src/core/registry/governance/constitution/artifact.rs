use super::*;

impl Registry {
    pub(crate) fn default_constitution_artifact() -> ConstitutionArtifact {
        ConstitutionArtifact {
            version: CONSTITUTION_VERSION,
            schema_version: CONSTITUTION_SCHEMA_VERSION.to_string(),
            compatibility: ConstitutionCompatibility {
                minimum_hivemind_version: env!("CARGO_PKG_VERSION").to_string(),
                governance_schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            },
            partitions: Vec::new(),
            rules: Vec::new(),
        }
    }

    pub(crate) fn constitution_path(&self, project_id: Uuid) -> PathBuf {
        self.governance_project_root(project_id)
            .join("constitution.yaml")
    }

    pub(crate) fn governance_constitution_location(
        &self,
        project_id: Uuid,
    ) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: Some(project_id),
            scope: "project",
            artifact_kind: "constitution",
            artifact_key: "constitution.yaml".to_string(),
            is_dir: false,
            path: self.constitution_path(project_id),
        }
    }

    pub(crate) fn read_constitution_artifact(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<(ConstitutionArtifact, PathBuf)> {
        let path = self.constitution_path(project_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "constitution_not_found",
                "Project constitution is missing",
                origin,
            )
            .with_hint("Run 'hivemind constitution init <project> --confirm' to initialize it"));
        }
        let raw = fs::read_to_string(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        let artifact = Self::parse_constitution_yaml(&raw, origin)
            .map_err(|err| err.with_context("path", path.to_string_lossy().to_string()))?;
        Ok((artifact, path))
    }

    pub(crate) fn parse_constitution_yaml(
        raw: &str,
        origin: &'static str,
    ) -> Result<ConstitutionArtifact> {
        serde_yaml::from_str(raw).map_err(|e| {
            HivemindError::user(
                "constitution_schema_invalid",
                format!("Malformed constitution YAML: {e}"),
                origin,
            )
            .with_hint(
                "Fix the constitution YAML schema and rerun 'hivemind constitution validate'",
            )
        })
    }

    pub(crate) fn write_constitution_artifact(
        path: &Path,
        artifact: &ConstitutionArtifact,
        origin: &'static str,
    ) -> Result<String> {
        let mut yaml = serde_yaml::to_string(artifact).map_err(|e| {
            HivemindError::system("constitution_serialize_failed", e.to_string(), origin)
        })?;
        if !yaml.ends_with('\n') {
            yaml.push('\n');
        }
        fs::write(path, yaml.as_bytes()).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        Ok(Self::constitution_digest(yaml.as_bytes()))
    }

    pub(crate) fn constitution_digest(bytes: &[u8]) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    pub(crate) fn resolve_actor(actor: Option<&str>) -> String {
        actor
            .and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .or_else(|| {
                env::var("USER").ok().and_then(|value| {
                    let trimmed = value.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                })
            })
            .or_else(|| {
                env::var("USERNAME").ok().and_then(|value| {
                    let trimmed = value.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                })
            })
            .unwrap_or_else(|| "unknown".to_string())
    }
}
