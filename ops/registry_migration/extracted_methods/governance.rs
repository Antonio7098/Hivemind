// AUTO-GENERATED from src/core/registry_original.rs
// Module bucket: governance

// governance_projects_dir (4002-4004)
    fn governance_projects_dir(&self) -> PathBuf {
        self.config.data_dir.join("projects")
    }

// governance_project_root (4006-4008)
    fn governance_project_root(&self, project_id: Uuid) -> PathBuf {
        self.governance_projects_dir().join(project_id.to_string())
    }

// governance_global_root (4010-4012)
    pub fn governance_global_root(&self) -> PathBuf {
        self.config.data_dir.join("global")
    }

// governance_artifact_locations (4014-4083)
    fn governance_artifact_locations(&self, project_id: Uuid) -> Vec<GovernanceArtifactLocation> {
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

// governance_default_file_contents (4085-4091)
    fn governance_default_file_contents(location: &GovernanceArtifactLocation) -> &'static [u8] {
        match (location.scope, location.artifact_kind) {
            ("project", "constitution") => b"version: 1\nschema_version: constitution.v1\ncompatibility:\n  minimum_hivemind_version: 0.1.32\n  governance_schema_version: governance.v1\npartitions: []\nrules: []\n",
            ("project", "graph_snapshot") => b"{}\n",
            _ => b"\n",
        }
    }

// governance_artifact_projection_key (4093-4103)
    fn governance_artifact_projection_key(location: &GovernanceArtifactLocation) -> String {
        format!(
            "{}::{}::{}::{}",
            location
                .project_id
                .map_or_else(|| "global".to_string(), |id| id.to_string()),
            location.scope,
            location.artifact_kind,
            location.artifact_key
        )
    }

// governance_projection_for_location (4105-4112)
    fn governance_projection_for_location<'a>(
        state: &'a AppState,
        location: &GovernanceArtifactLocation,
    ) -> Option<&'a crate::core::state::GovernanceArtifact> {
        state
            .governance_artifacts
            .get(&Self::governance_artifact_projection_key(location))
    }

// next_governance_revision (4114-4128)
    fn next_governance_revision(
        state: &AppState,
        location: &GovernanceArtifactLocation,
        pending_revisions: &mut HashMap<String, u64>,
    ) -> u64 {
        let key = Self::governance_artifact_projection_key(location);
        if let Some(next) = pending_revisions.get_mut(&key) {
            *next = next.saturating_add(1);
            return *next;
        }
        let next = Self::governance_projection_for_location(state, location)
            .map_or(1, |existing| existing.revision.saturating_add(1));
        pending_revisions.insert(key, next);
        next
    }

// ensure_governance_layout (4130-4191)
    fn ensure_governance_layout(
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

// legacy_governance_mappings (4193-4216)
    fn legacy_governance_mappings(
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

// copy_dir_if_missing (4218-4264)
    fn copy_dir_if_missing(
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

// copy_file_if_missing (4266-4310)
    fn copy_file_if_missing(
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

// validate_governance_identifier (4312-4342)
    fn validate_governance_identifier(
        raw: &str,
        field: &str,
        origin: &'static str,
    ) -> Result<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(HivemindError::user(
                "invalid_governance_identifier",
                format!("{field} cannot be empty"),
                origin,
            )
            .with_hint(
                "Use lowercase letters, numbers, '.', '_' or '-' when naming governance artifacts",
            ));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err(HivemindError::user(
                "invalid_governance_identifier",
                format!(
                    "{field} '{trimmed}' contains unsupported characters; only [a-zA-Z0-9._-] are allowed"
                ),
                origin,
            )
            .with_hint("Replace unsupported characters with '-', '_' or '.'"));
        }
        Ok(trimmed.to_string())
    }

// normalized_string_list (4344-4358)
    fn normalized_string_list(values: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for value in values {
            let item = value.trim();
            if item.is_empty() {
                continue;
            }
            let item_owned = item.to_string();
            if seen.insert(item_owned.clone()) {
                out.push(item_owned);
            }
        }
        out
    }

// read_governance_json (4360-4385)
    fn read_governance_json<T: DeserializeOwned>(
        path: &Path,
        artifact_kind: &str,
        artifact_key: &str,
        origin: &'static str,
    ) -> Result<T> {
        let raw = fs::read_to_string(path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
                .with_context("artifact_kind", artifact_kind.to_string())
                .with_context("artifact_key", artifact_key.to_string())
        })?;
        serde_json::from_str(&raw).map_err(|e| {
            HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "Malformed {artifact_kind} artifact '{artifact_key}': {e}"
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string())
            .with_hint(
                "Inspect and repair this artifact JSON, or recreate it using the corresponding CLI create/update command",
            )
        })
    }

// write_governance_json (4387-4407)
    fn write_governance_json<T: Serialize>(
        path: &Path,
        value: &T,
        origin: &'static str,
    ) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
            })?;
        }
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| {
            HivemindError::system(
                "governance_artifact_serialize_failed",
                e.to_string(),
                origin,
            )
        })?;
        fs::write(path, bytes).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })
    }

// ensure_global_governance_layout (4409-4427)
    fn ensure_global_governance_layout(&self, origin: &'static str) -> Result<()> {
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

// default_constitution_artifact (4429-4440)
    fn default_constitution_artifact() -> ConstitutionArtifact {
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

// constitution_path (4442-4445)
    fn constitution_path(&self, project_id: Uuid) -> PathBuf {
        self.governance_project_root(project_id)
            .join("constitution.yaml")
    }

// governance_constitution_location (4447-4456)
    fn governance_constitution_location(&self, project_id: Uuid) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: Some(project_id),
            scope: "project",
            artifact_kind: "constitution",
            artifact_key: "constitution.yaml".to_string(),
            is_dir: false,
            path: self.constitution_path(project_id),
        }
    }

// governance_graph_snapshot_location (4463-4472)
    fn governance_graph_snapshot_location(&self, project_id: Uuid) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: Some(project_id),
            scope: "project",
            artifact_kind: "graph_snapshot",
            artifact_key: "graph_snapshot.json".to_string(),
            is_dir: false,
            path: self.graph_snapshot_path(project_id),
        }
    }

// validate_constitution (4620-4776)
    #[allow(clippy::too_many_lines)]
    fn validate_constitution(artifact: &ConstitutionArtifact) -> Vec<ConstitutionValidationIssue> {
        let mut issues = Vec::new();

        if artifact.version != CONSTITUTION_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_version_unsupported".to_string(),
                field: "version".to_string(),
                message: format!(
                    "Unsupported constitution version {}; expected {CONSTITUTION_VERSION}",
                    artifact.version
                ),
            });
        }
        if artifact.schema_version.trim() != CONSTITUTION_SCHEMA_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_schema_version_unsupported".to_string(),
                field: "schema_version".to_string(),
                message: format!(
                    "Unsupported schema_version '{}'; expected '{CONSTITUTION_SCHEMA_VERSION}'",
                    artifact.schema_version
                ),
            });
        }
        if artifact
            .compatibility
            .minimum_hivemind_version
            .trim()
            .is_empty()
        {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_minimum_hivemind_version_missing".to_string(),
                field: "compatibility.minimum_hivemind_version".to_string(),
                message: "minimum_hivemind_version cannot be empty".to_string(),
            });
        }
        if artifact.compatibility.governance_schema_version.trim() != GOVERNANCE_SCHEMA_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_governance_schema_mismatch".to_string(),
                field: "compatibility.governance_schema_version".to_string(),
                message: format!("governance_schema_version must be '{GOVERNANCE_SCHEMA_VERSION}'"),
            });
        }

        let mut partition_ids = HashSet::new();
        let mut partition_paths = HashSet::new();
        for partition in &artifact.partitions {
            let partition_id = partition.id.trim();
            if partition_id.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_id_missing".to_string(),
                    field: "partitions[].id".to_string(),
                    message: "Partition id cannot be empty".to_string(),
                });
            } else if !partition_ids.insert(partition_id.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_id_duplicate".to_string(),
                    field: format!("partitions.{}", partition.id),
                    message: format!("Duplicate partition id '{}'", partition.id),
                });
            }

            let partition_path = partition.path.trim();
            if partition_path.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_path_missing".to_string(),
                    field: format!("partitions.{}.path", partition.id),
                    message: "Partition path cannot be empty".to_string(),
                });
            } else if !partition_paths.insert(partition_path.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_path_duplicate".to_string(),
                    field: format!("partitions.{}.path", partition.id),
                    message: format!("Duplicate partition path '{}'", partition.path),
                });
            }
        }

        let mut rule_ids = HashSet::new();
        for rule in &artifact.rules {
            let rule_id = rule.id().trim();
            if rule_id.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_rule_id_missing".to_string(),
                    field: "rules[].id".to_string(),
                    message: "Rule id cannot be empty".to_string(),
                });
                continue;
            }
            if !rule_ids.insert(rule_id.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_rule_id_duplicate".to_string(),
                    field: format!("rules.{rule_id}"),
                    message: format!("Duplicate rule id '{rule_id}'"),
                });
            }

            match rule {
                ConstitutionRule::ForbiddenDependency { from, to, .. }
                | ConstitutionRule::AllowedDependency { from, to, .. } => {
                    if from.trim() == to.trim() {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_self_dependency".to_string(),
                            field: format!("rules.{rule_id}"),
                            message: "Rule cannot reference the same partition for from/to"
                                .to_string(),
                        });
                    }
                    for (field, partition_ref) in [("from", from), ("to", to)] {
                        if partition_ref.trim().is_empty() {
                            issues.push(ConstitutionValidationIssue {
                                code: "constitution_rule_partition_missing".to_string(),
                                field: format!("rules.{rule_id}.{field}"),
                                message: format!("Rule field '{field}' cannot be empty"),
                            });
                        } else if !partition_ids.contains(partition_ref.trim()) {
                            issues.push(ConstitutionValidationIssue {
                                code: "constitution_rule_partition_unknown".to_string(),
                                field: format!("rules.{rule_id}.{field}"),
                                message: format!(
                                    "Rule references unknown partition '{partition_ref}'"
                                ),
                            });
                        }
                    }
                }
                ConstitutionRule::CoverageRequirement {
                    target, threshold, ..
                } => {
                    if target.trim().is_empty() {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_target_missing".to_string(),
                            field: format!("rules.{rule_id}.target"),
                            message: "Coverage target partition cannot be empty".to_string(),
                        });
                    } else if !partition_ids.contains(target.trim()) {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_target_unknown".to_string(),
                            field: format!("rules.{rule_id}.target"),
                            message: format!(
                                "Coverage rule references unknown partition '{target}'"
                            ),
                        });
                    }
                    if *threshold > 100 {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_threshold_invalid".to_string(),
                            field: format!("rules.{rule_id}.threshold"),
                            message: "Coverage threshold must be between 0 and 100".to_string(),
                        });
                    }
                }
            }
        }

        issues
    }

// read_constitution_artifact (4778-4799)
    fn read_constitution_artifact(
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

// parse_constitution_yaml (4801-4812)
    fn parse_constitution_yaml(raw: &str, origin: &'static str) -> Result<ConstitutionArtifact> {
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

// write_constitution_artifact (4814-4830)
    fn write_constitution_artifact(
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

// constitution_digest (4832-4836)
    fn constitution_digest(bytes: &[u8]) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

// resolve_actor (4838-4857)
    fn resolve_actor(actor: Option<&str>) -> String {
        actor
            .and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .or_else(|| {
                env::var("USER")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| value.trim().to_string())
            })
            .or_else(|| {
                env::var("USERNAME")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| value.trim().to_string())
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

// project_document_path (4859-4863)
    fn project_document_path(&self, project_id: Uuid, document_id: &str) -> PathBuf {
        self.governance_project_root(project_id)
            .join("documents")
            .join(format!("{document_id}.json"))
    }

// global_skill_path (4865-4869)
    fn global_skill_path(&self, skill_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("skills")
            .join(format!("{skill_id}.json"))
    }

// global_system_prompt_path (4871-4875)
    fn global_system_prompt_path(&self, prompt_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("system_prompts")
            .join(format!("{prompt_id}.json"))
    }

// global_template_path (4877-4881)
    fn global_template_path(&self, template_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("templates")
            .join(format!("{template_id}.json"))
    }

// governance_document_location (4883-4896)
    fn governance_document_location(
        &self,
        project_id: Uuid,
        document_id: &str,
    ) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: Some(project_id),
            scope: "project",
            artifact_kind: "document",
            artifact_key: document_id.to_string(),
            is_dir: false,
            path: self.project_document_path(project_id, document_id),
        }
    }

// governance_global_location (4898-4911)
    fn governance_global_location(
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

// governance_notepad_location (4913-4932)
    fn governance_notepad_location(&self, project_id: Option<Uuid>) -> GovernanceArtifactLocation {
        project_id.map_or_else(
            || GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "notepad",
                artifact_key: "notepad.md".to_string(),
                is_dir: false,
                path: self.governance_global_root().join("notepad.md"),
            },
            |project_id| GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "notepad",
                artifact_key: "notepad.md".to_string(),
                is_dir: false,
                path: self.governance_project_root(project_id).join("notepad.md"),
            },
        )
    }

// governance_snapshot_root (4934-4938)
    fn governance_snapshot_root(&self, project_id: Uuid) -> PathBuf {
        self.governance_project_root(project_id)
            .join("recovery")
            .join("snapshots")
    }

// governance_snapshot_path (4940-4943)
    fn governance_snapshot_path(&self, project_id: Uuid, snapshot_id: &str) -> PathBuf {
        self.governance_snapshot_root(project_id)
            .join(format!("{snapshot_id}.json"))
    }

// governance_projection_key_from_parts (4945-4953)
    fn governance_projection_key_from_parts(
        project_id: Option<Uuid>,
        scope: &str,
        artifact_kind: &str,
        artifact_key: &str,
    ) -> String {
        let project_key = project_id.map_or_else(|| "global".to_string(), |id| id.to_string());
        format!("{project_key}::{scope}::{artifact_kind}::{artifact_key}")
    }

// governance_location_for_path (4955-5013)
    fn governance_location_for_path(
        &self,
        project_id: Uuid,
        path: &Path,
    ) -> Option<GovernanceArtifactLocation> {
        let project_root = self.governance_project_root(project_id);
        let global_root = self.governance_global_root();
        let documents_root = project_root.join("documents");
        let skills_root = global_root.join("skills");
        let prompts_root = global_root.join("system_prompts");
        let templates_root = global_root.join("templates");

        if path == self.constitution_path(project_id) {
            return Some(self.governance_constitution_location(project_id));
        }
        if path == self.graph_snapshot_path(project_id) {
            return Some(self.governance_graph_snapshot_location(project_id));
        }
        if path == self.governance_notepad_location(Some(project_id)).path {
            return Some(self.governance_notepad_location(Some(project_id)));
        }
        if path == self.governance_notepad_location(None).path {
            return Some(self.governance_notepad_location(None));
        }

        if path.starts_with(&documents_root) && path.extension().is_some_and(|ext| ext == "json") {
            let key = path.file_stem()?.to_str()?.to_string();
            return Some(self.governance_document_location(project_id, &key));
        }

        if path.starts_with(&skills_root) && path.extension().is_some_and(|ext| ext == "json") {
            let key = path.file_stem()?.to_str()?.to_string();
            return Some(Self::governance_global_location(
                "skill",
                &key,
                path.to_path_buf(),
            ));
        }

        if path.starts_with(&prompts_root) && path.extension().is_some_and(|ext| ext == "json") {
            let key = path.file_stem()?.to_str()?.to_string();
            return Some(Self::governance_global_location(
                "system_prompt",
                &key,
                path.to_path_buf(),
            ));
        }

        if path.starts_with(&templates_root) && path.extension().is_some_and(|ext| ext == "json") {
            let key = path.file_stem()?.to_str()?.to_string();
            return Some(Self::governance_global_location(
                "template",
                &key,
                path.to_path_buf(),
            ));
        }

        None
    }

// governance_managed_file_locations (5015-5070)
    fn governance_managed_file_locations(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Vec<GovernanceArtifactLocation>> {
        let mut out = Vec::new();
        let project_root = self.governance_project_root(project_id);
        let global_root = self.governance_global_root();

        let constitution = self.governance_constitution_location(project_id);
        if constitution.path.is_file() {
            out.push(constitution);
        }
        let graph_snapshot = self.governance_graph_snapshot_location(project_id);
        if graph_snapshot.path.is_file() {
            out.push(graph_snapshot);
        }
        let project_notepad = self.governance_notepad_location(Some(project_id));
        if project_notepad.path.is_file() {
            out.push(project_notepad);
        }
        let global_notepad = self.governance_notepad_location(None);
        if global_notepad.path.is_file() {
            out.push(global_notepad);
        }

        for path in Self::governance_json_paths(&project_root.join("documents"), origin)? {
            if let Some(location) = self.governance_location_for_path(project_id, &path) {
                out.push(location);
            }
        }
        for path in Self::governance_json_paths(&global_root.join("skills"), origin)? {
            if let Some(location) = self.governance_location_for_path(project_id, &path) {
                out.push(location);
            }
        }
        for path in Self::governance_json_paths(&global_root.join("system_prompts"), origin)? {
            if let Some(location) = self.governance_location_for_path(project_id, &path) {
                out.push(location);
            }
        }
        for path in Self::governance_json_paths(&global_root.join("templates"), origin)? {
            if let Some(location) = self.governance_location_for_path(project_id, &path) {
                out.push(location);
            }
        }

        out.sort_by(|a, b| {
            a.scope
                .cmp(b.scope)
                .then(a.artifact_kind.cmp(b.artifact_kind))
                .then(a.artifact_key.cmp(&b.artifact_key))
                .then(a.path.cmp(&b.path))
        });
        Ok(out)
    }

// load_snapshot_manifest (5072-5100)
    fn load_snapshot_manifest(
        &self,
        project_id: Uuid,
        snapshot_id: &str,
        origin: &'static str,
    ) -> Result<GovernanceRecoverySnapshotManifest> {
        let path = self.governance_snapshot_path(project_id, snapshot_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "governance_snapshot_not_found",
                format!("Governance snapshot '{snapshot_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind project governance snapshot list <project>' to inspect available snapshots"));
        }
        let raw = fs::read_to_string(&path).map_err(|e| {
            HivemindError::system("governance_snapshot_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        serde_json::from_str::<GovernanceRecoverySnapshotManifest>(&raw).map_err(|e| {
            HivemindError::user(
                "governance_snapshot_schema_invalid",
                format!("Malformed governance snapshot '{snapshot_id}': {e}"),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string())
            .with_hint("Delete the invalid snapshot and create a new one")
        })
    }

// list_snapshot_manifests (5102-5142)
    fn list_snapshot_manifests(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Vec<(PathBuf, GovernanceRecoverySnapshotManifest)>> {
        let root = self.governance_snapshot_root(project_id);
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        for entry in fs::read_dir(&root).map_err(|e| {
            HivemindError::system("governance_snapshot_list_failed", e.to_string(), origin)
                .with_context("path", root.to_string_lossy().to_string())
        })? {
            let entry = entry.map_err(|e| {
                HivemindError::system("governance_snapshot_list_failed", e.to_string(), origin)
            })?;
            let path = entry.path();
            if !path.is_file() || path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = fs::read_to_string(&path).map_err(|e| {
                HivemindError::system("governance_snapshot_read_failed", e.to_string(), origin)
                    .with_context("path", path.to_string_lossy().to_string())
            })?;
            let manifest = serde_json::from_str::<GovernanceRecoverySnapshotManifest>(&raw)
                .map_err(|e| {
                    HivemindError::user(
                        "governance_snapshot_schema_invalid",
                        format!("Malformed governance snapshot at '{}': {e}", path.display()),
                        origin,
                    )
                    .with_context("path", path.to_string_lossy().to_string())
                })?;
            if manifest.project_id == project_id {
                items.push((path, manifest));
            }
        }
        items.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));
        Ok(items)
    }

// append_governance_upsert_for_location (5144-5170)
    fn append_governance_upsert_for_location(
        &self,
        state: &AppState,
        pending_revisions: &mut HashMap<String, u64>,
        location: &GovernanceArtifactLocation,
        corr: CorrelationIds,
        origin: &'static str,
    ) -> Result<u64> {
        let revision = Self::next_governance_revision(state, location, pending_revisions);
        self.append_event(
            Event::new(
                EventPayload::GovernanceArtifactUpserted {
                    project_id: location.project_id,
                    scope: location.scope.to_string(),
                    artifact_kind: location.artifact_kind.to_string(),
                    artifact_key: location.artifact_key.clone(),
                    path: location.path.to_string_lossy().to_string(),
                    revision,
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                corr,
            ),
            origin,
        )?;
        Ok(revision)
    }

// append_governance_delete_for_location (5172-5193)
    fn append_governance_delete_for_location(
        &self,
        location: &GovernanceArtifactLocation,
        corr: CorrelationIds,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::GovernanceArtifactDeleted {
                    project_id: location.project_id,
                    scope: location.scope.to_string(),
                    artifact_kind: location.artifact_kind.to_string(),
                    artifact_key: location.artifact_key.clone(),
                    path: location.path.to_string_lossy().to_string(),
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                corr,
            ),
            origin,
        )
    }

// read_project_document_artifact (5195-5236)
    fn read_project_document_artifact(
        &self,
        project_id: Uuid,
        document_id: &str,
        origin: &'static str,
    ) -> Result<ProjectDocumentArtifact> {
        let path = self.project_document_path(project_id, document_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "document_not_found",
                format!("Project document '{document_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind project governance document list <project>' to inspect available documents"));
        }
        let artifact = Self::read_governance_json::<ProjectDocumentArtifact>(
            &path,
            "project_document",
            document_id,
            origin,
        )?;
        if artifact.document_id != document_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "Document file key mismatch: expected '{document_id}', found '{}'",
                    artifact.document_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        if artifact.revisions.is_empty() {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!("Document '{document_id}' has no revision history"),
                origin,
            )
            .with_hint("Repair the document JSON to include at least one revision"));
        }
        Ok(artifact)
    }

// governance_document_attachment_states (5238-5261)
    fn governance_document_attachment_states(
        state: &AppState,
        project_id: Uuid,
        task_id: Uuid,
    ) -> (Vec<String>, Vec<String>) {
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        for item in state.governance_attachments.values().filter(|item| {
            item.project_id == project_id
                && item.task_id == task_id
                && item.artifact_kind == "document"
        }) {
            if item.attached {
                included.push(item.artifact_key.clone());
            } else {
                excluded.push(item.artifact_key.clone());
            }
        }
        included.sort();
        included.dedup();
        excluded.sort();
        excluded.dedup();
        (included, excluded)
    }

// governance_artifact_revision (5263-5280)
    fn governance_artifact_revision(
        state: &AppState,
        project_id: Option<Uuid>,
        scope: &str,
        artifact_kind: &str,
        artifact_key: &str,
    ) -> Option<u64> {
        state
            .governance_artifacts
            .values()
            .find(|item| {
                item.project_id == project_id
                    && item.scope == scope
                    && item.artifact_kind == artifact_kind
                    && item.artifact_key == artifact_key
            })
            .map(|item| item.revision)
    }

// latest_template_instantiation_snapshot (5282-5315)
    fn latest_template_instantiation_snapshot(
        &self,
        project_id: Uuid,
    ) -> Result<Option<TemplateInstantiationSnapshot>> {
        let events = self.read_events(&EventFilter::for_project(project_id))?;
        let mut snapshot: Option<TemplateInstantiationSnapshot> = None;
        for event in events {
            let event_id = event.id().as_uuid().to_string();
            if let EventPayload::TemplateInstantiated {
                project_id: event_project_id,
                template_id,
                system_prompt_id,
                skill_ids,
                document_ids,
                schema_version,
                projection_version,
            } = event.payload
            {
                if event_project_id != project_id {
                    continue;
                }
                snapshot = Some(TemplateInstantiationSnapshot {
                    event_id,
                    template_id,
                    system_prompt_id,
                    skill_ids,
                    document_ids,
                    schema_version,
                    projection_version,
                });
            }
        }
        Ok(snapshot)
    }

// attempt_manifest_hash_for_attempt (5317-5334)
    fn attempt_manifest_hash_for_attempt(&self, attempt_id: Uuid) -> Result<Option<String>> {
        let filter = EventFilter {
            attempt_id: Some(attempt_id),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;
        let mut manifest_hash = None;
        for event in events {
            if let EventPayload::AttemptContextAssembled {
                manifest_hash: hash,
                ..
            } = event.payload
            {
                manifest_hash = Some(hash);
            }
        }
        Ok(manifest_hash)
    }

// read_global_skill_artifact (5817-5849)
    fn read_global_skill_artifact(
        &self,
        skill_id: &str,
        origin: &'static str,
    ) -> Result<GlobalSkillArtifact> {
        let path = self.global_skill_path(skill_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "skill_not_found",
                format!("Global skill '{skill_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind global skill list' to inspect available skills"));
        }
        let artifact = Self::read_governance_json::<GlobalSkillArtifact>(
            &path,
            "global_skill",
            skill_id,
            origin,
        )?;
        if artifact.skill_id != skill_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "Skill file key mismatch: expected '{skill_id}', found '{}'",
                    artifact.skill_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        Ok(artifact)
    }

// read_global_system_prompt_artifact (5851-5885)
    fn read_global_system_prompt_artifact(
        &self,
        prompt_id: &str,
        origin: &'static str,
    ) -> Result<GlobalSystemPromptArtifact> {
        let path = self.global_system_prompt_path(prompt_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "system_prompt_not_found",
                format!("Global system prompt '{prompt_id}' not found"),
                origin,
            )
            .with_hint(
                "Use 'hivemind global system-prompt list' to inspect available system prompts",
            ));
        }
        let artifact = Self::read_governance_json::<GlobalSystemPromptArtifact>(
            &path,
            "global_system_prompt",
            prompt_id,
            origin,
        )?;
        if artifact.prompt_id != prompt_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "System prompt file key mismatch: expected '{prompt_id}', found '{}'",
                    artifact.prompt_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        Ok(artifact)
    }

// read_global_template_artifact (5887-5919)
    fn read_global_template_artifact(
        &self,
        template_id: &str,
        origin: &'static str,
    ) -> Result<GlobalTemplateArtifact> {
        let path = self.global_template_path(template_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "template_not_found",
                format!("Global template '{template_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind global template list' to inspect available templates"));
        }
        let artifact = Self::read_governance_json::<GlobalTemplateArtifact>(
            &path,
            "global_template",
            template_id,
            origin,
        )?;
        if artifact.template_id != template_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "Template file key mismatch: expected '{template_id}', found '{}'",
                    artifact.template_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        Ok(artifact)
    }

// governance_json_paths (5921-5945)
    fn governance_json_paths(dir: &Path, origin: &'static str) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        if !dir.is_dir() {
            return Ok(paths);
        }
        for entry in fs::read_dir(dir).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", dir.to_string_lossy().to_string())
        })? {
            let entry = entry.map_err(|e| {
                HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                    .with_context("path", dir.to_string_lossy().to_string())
            })?;
            let path = entry.path();
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }

// project_document_summary_from_artifact (5947-5963)
    fn project_document_summary_from_artifact(
        project_id: Uuid,
        artifact: &ProjectDocumentArtifact,
        path: &Path,
    ) -> ProjectGovernanceDocumentSummary {
        let revision = artifact.revisions.last().map_or(0, |rev| rev.revision);
        ProjectGovernanceDocumentSummary {
            project_id,
            document_id: artifact.document_id.clone(),
            title: artifact.title.clone(),
            owner: artifact.owner.clone(),
            tags: artifact.tags.clone(),
            updated_at: artifact.updated_at,
            revision,
            path: path.to_string_lossy().to_string(),
        }
    }

// project_governance_init (9673-9761)
    /// Initializes canonical governance storage and projections for a project.
    ///
    /// # Errors
    /// Returns an error if the project cannot be resolved, storage cannot be created,
    /// or governance events cannot be appended.
    pub fn project_governance_init(&self, id_or_name: &str) -> Result<ProjectGovernanceInitResult> {
        let origin = "registry:project_governance_init";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let created_paths = self.ensure_governance_layout(project.id, origin)?;
        let created_set: HashSet<PathBuf> = created_paths.iter().cloned().collect();
        let corr = CorrelationIds::for_project(project.id);

        if !state.governance_projects.contains_key(&project.id) || !created_paths.is_empty() {
            self.append_event(
                Event::new(
                    EventPayload::GovernanceProjectStorageInitialized {
                        project_id: project.id,
                        schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                        projection_version: GOVERNANCE_PROJECTION_VERSION,
                        root_path: self
                            .governance_project_root(project.id)
                            .to_string_lossy()
                            .to_string(),
                    },
                    corr.clone(),
                ),
                origin,
            )?;
        }

        let mut pending_revisions = HashMap::new();
        for location in self.governance_artifact_locations(project.id) {
            let exists = if location.is_dir {
                location.path.is_dir()
            } else {
                location.path.is_file()
            };
            if !exists {
                continue;
            }

            let projected_exists =
                Self::governance_projection_for_location(&state, &location).is_some();
            let should_emit = created_set.contains(&location.path) || !projected_exists;
            if !should_emit {
                continue;
            }

            let revision =
                Self::next_governance_revision(&state, &location, &mut pending_revisions);
            self.append_event(
                Event::new(
                    EventPayload::GovernanceArtifactUpserted {
                        project_id: location.project_id,
                        scope: location.scope.to_string(),
                        artifact_kind: location.artifact_kind.to_string(),
                        artifact_key: location.artifact_key.clone(),
                        path: location.path.to_string_lossy().to_string(),
                        revision,
                        schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                        projection_version: GOVERNANCE_PROJECTION_VERSION,
                    },
                    corr.clone(),
                ),
                origin,
            )?;
        }

        let mut created_paths_rendered: Vec<String> = created_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        created_paths_rendered.sort();

        Ok(ProjectGovernanceInitResult {
            project_id: project.id,
            root_path: self
                .governance_project_root(project.id)
                .to_string_lossy()
                .to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            created_paths: created_paths_rendered,
        })
    }

// project_governance_migrate (9763-9833)
    /// Migrates legacy governance artifacts from repo-local layout into canonical
    /// global governance storage for the selected project.
    ///
    /// # Errors
    /// Returns an error if migration copy operations fail or governance events
    /// cannot be appended.
    pub fn project_governance_migrate(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceMigrateResult> {
        let origin = "registry:project_governance_migrate";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let mut migrated_paths = BTreeSet::new();
        for mapping in self
            .legacy_governance_mappings(&project)
            .into_iter()
            .filter(|m| m.source.exists())
        {
            let copied = if mapping.destination.is_dir {
                Self::copy_dir_if_missing(&mapping.source, &mapping.destination.path, origin)?
            } else {
                Self::copy_file_if_missing(
                    &mapping.source,
                    &mapping.destination.path,
                    Some(Self::governance_default_file_contents(&mapping.destination)),
                    origin,
                )?
            };

            if copied {
                migrated_paths.insert(mapping.destination.path.to_string_lossy().to_string());
            }
        }

        let _ = self.project_governance_init(&project.id.to_string())?;

        let rollback_hint = format!(
            "Rollback by restoring repo-local governance paths from backups under each attached repo '.hivemind/' directory. New layout root: {}",
            self.governance_project_root(project.id).to_string_lossy()
        );
        let migrated_paths_vec: Vec<String> = migrated_paths.into_iter().collect();

        self.append_event(
            Event::new(
                EventPayload::GovernanceStorageMigrated {
                    project_id: Some(project.id),
                    from_layout: GOVERNANCE_FROM_LAYOUT.to_string(),
                    to_layout: GOVERNANCE_TO_LAYOUT.to_string(),
                    migrated_paths: migrated_paths_vec.clone(),
                    rollback_hint: rollback_hint.clone(),
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceMigrateResult {
            project_id: project.id,
            from_layout: GOVERNANCE_FROM_LAYOUT.to_string(),
            to_layout: GOVERNANCE_TO_LAYOUT.to_string(),
            migrated_paths: migrated_paths_vec,
            rollback_hint,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// project_governance_inspect (9835-9928)
    /// Inspects governance storage and projection status for a project.
    ///
    /// # Errors
    /// Returns an error if the project cannot be resolved or state replay fails.
    pub fn project_governance_inspect(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceInspectResult> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let mut artifacts: Vec<GovernanceArtifactInspect> = self
            .governance_artifact_locations(project.id)
            .into_iter()
            .map(|location| {
                let projection = Self::governance_projection_for_location(&state, &location);
                GovernanceArtifactInspect {
                    scope: location.scope.to_string(),
                    artifact_kind: location.artifact_kind.to_string(),
                    artifact_key: location.artifact_key,
                    path: location.path.to_string_lossy().to_string(),
                    exists: if location.is_dir {
                        location.path.is_dir()
                    } else {
                        location.path.is_file()
                    },
                    projected: projection.is_some(),
                    revision: projection.map_or(0, |item| item.revision),
                }
            })
            .collect();
        artifacts.sort_by(|a, b| {
            a.scope
                .cmp(&b.scope)
                .then(a.artifact_kind.cmp(&b.artifact_kind))
                .then(a.artifact_key.cmp(&b.artifact_key))
        });

        let mut migrations: Vec<GovernanceMigrationSummary> = state
            .governance_migrations
            .iter()
            .filter(|migration| {
                migration.project_id.is_none() || migration.project_id == Some(project.id)
            })
            .map(|migration| GovernanceMigrationSummary {
                from_layout: migration.from_layout.clone(),
                to_layout: migration.to_layout.clone(),
                migrated_paths: migration.migrated_paths.clone(),
                rollback_hint: migration.rollback_hint.clone(),
                schema_version: migration.schema_version.clone(),
                projection_version: migration.projection_version,
                migrated_at: migration.migrated_at,
            })
            .collect();
        migrations.sort_by(|a, b| b.migrated_at.cmp(&a.migrated_at));

        let mut legacy_candidates = BTreeSet::new();
        for mapping in self.legacy_governance_mappings(&project) {
            if mapping.source.exists() {
                legacy_candidates.insert(mapping.source.to_string_lossy().to_string());
            }
        }

        let projected_storage = state.governance_projects.get(&project.id);
        Ok(ProjectGovernanceInspectResult {
            project_id: project.id,
            root_path: projected_storage.map_or_else(
                || {
                    self.governance_project_root(project.id)
                        .to_string_lossy()
                        .to_string()
                },
                |item| item.root_path.clone(),
            ),
            initialized: projected_storage.is_some(),
            schema_version: projected_storage.map_or_else(
                || GOVERNANCE_SCHEMA_VERSION.to_string(),
                |item| item.schema_version.clone(),
            ),
            projection_version: projected_storage.map_or(GOVERNANCE_PROJECTION_VERSION, |item| {
                item.projection_version
            }),
            export_import_boundary: GOVERNANCE_EXPORT_IMPORT_BOUNDARY.to_string(),
            worktree_base_dir: WorktreeConfig::default()
                .base_dir
                .to_string_lossy()
                .to_string(),
            artifacts,
            migrations,
            legacy_candidates: legacy_candidates.into_iter().collect(),
        })
    }

// project_governance_diagnose (9930-10249)
    /// Diagnoses governance artifact health for operators.
    ///
    /// # Errors
    /// Returns an error if the project cannot be resolved or artifact listing fails.
    #[allow(clippy::too_many_lines)]
    pub fn project_governance_diagnose(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceDiagnosticsResult> {
        let origin = "registry:project_governance_diagnose";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let mut issues: Vec<GovernanceDiagnosticIssue> = Vec::new();
        let mut push_issue = |issue: GovernanceDiagnosticIssue| {
            issues.push(issue);
        };

        let constitution_path = self.constitution_path(project.id);
        match self.read_constitution_artifact(project.id, origin) {
            Ok((artifact, path)) => {
                for issue in Self::validate_constitution(&artifact) {
                    push_issue(GovernanceDiagnosticIssue {
                        code: issue.code,
                        severity: "error".to_string(),
                        message: issue.message,
                        hint: Some(
                            "Run 'hivemind constitution validate <project>' and fix the invalid rule or partition".to_string(),
                        ),
                        artifact_kind: Some("constitution".to_string()),
                        artifact_id: Some("constitution.yaml".to_string()),
                        template_id: None,
                        path: Some(path.to_string_lossy().to_string()),
                    });
                }
            }
            Err(err) => {
                push_issue(GovernanceDiagnosticIssue {
                    code: err.code,
                    severity: "error".to_string(),
                    message: err.message,
                    hint: err.recovery_hint,
                    artifact_kind: Some("constitution".to_string()),
                    artifact_id: Some("constitution.yaml".to_string()),
                    template_id: None,
                    path: err
                        .context
                        .get("path")
                        .cloned()
                        .or_else(|| Some(constitution_path.to_string_lossy().to_string())),
                });
            }
        }

        if !project.repositories.is_empty() {
            let snapshot_path = self
                .governance_graph_snapshot_location(project.id)
                .path
                .to_string_lossy()
                .to_string();
            if let Err(err) = self.ensure_graph_snapshot_current_for_constitution(&project, origin)
            {
                push_issue(GovernanceDiagnosticIssue {
                    code: err.code,
                    severity: "error".to_string(),
                    message: err.message,
                    hint: err.recovery_hint,
                    artifact_kind: Some("graph_snapshot".to_string()),
                    artifact_id: Some("graph_snapshot.json".to_string()),
                    template_id: None,
                    path: Some(snapshot_path),
                });
            }
        }

        let template_root = self.governance_global_root().join("templates");
        for path in Self::governance_json_paths(&template_root, origin)? {
            let template_id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map_or_else(|| "unknown".to_string(), std::string::ToString::to_string);
            let path_rendered = path.to_string_lossy().to_string();

            let artifact = match Self::read_governance_json::<GlobalTemplateArtifact>(
                &path,
                "global_template",
                &template_id,
                origin,
            ) {
                Ok(artifact) => artifact,
                Err(err) => {
                    push_issue(GovernanceDiagnosticIssue {
                        code: err.code,
                        severity: "error".to_string(),
                        message: err.message,
                        hint: err.recovery_hint,
                        artifact_kind: Some("template".to_string()),
                        artifact_id: Some(template_id.clone()),
                        template_id: Some(template_id.clone()),
                        path: Some(path_rendered),
                    });
                    continue;
                }
            };

            if artifact.template_id != template_id {
                push_issue(GovernanceDiagnosticIssue {
                    code: "template_id_mismatch".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Template file key mismatch: expected '{template_id}', found '{}'",
                        artifact.template_id
                    ),
                    hint: Some("Rename the template file or fix template_id in JSON".to_string()),
                    artifact_kind: Some("template".to_string()),
                    artifact_id: Some(template_id.clone()),
                    template_id: Some(template_id.clone()),
                    path: Some(path_rendered.clone()),
                });
            }

            if self
                .read_global_system_prompt_artifact(&artifact.system_prompt_id, origin)
                .is_err()
            {
                push_issue(GovernanceDiagnosticIssue {
                    code: "template_system_prompt_missing".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Template '{template_id}' references missing system prompt '{}'",
                        artifact.system_prompt_id
                    ),
                    hint: Some(
                        "Create the missing system prompt or update template.system_prompt_id"
                            .to_string(),
                    ),
                    artifact_kind: Some("system_prompt".to_string()),
                    artifact_id: Some(artifact.system_prompt_id.clone()),
                    template_id: Some(template_id.clone()),
                    path: Some(path_rendered.clone()),
                });
            }

            for skill_id in &artifact.skill_ids {
                if self.read_global_skill_artifact(skill_id, origin).is_err() {
                    push_issue(GovernanceDiagnosticIssue {
                        code: "template_skill_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Template '{template_id}' references missing global skill '{skill_id}'"
                        ),
                        hint: Some(
                            "Create the missing skill or remove it from template.skill_ids"
                                .to_string(),
                        ),
                        artifact_kind: Some("skill".to_string()),
                        artifact_id: Some(skill_id.clone()),
                        template_id: Some(template_id.clone()),
                        path: Some(path_rendered.clone()),
                    });
                }
            }

            for document_id in &artifact.document_ids {
                if self
                    .read_project_document_artifact(project.id, document_id, origin)
                    .is_err()
                {
                    push_issue(GovernanceDiagnosticIssue {
                        code: "template_document_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Template '{template_id}' references missing project document '{document_id}'"
                        ),
                        hint: Some(
                            "Create the project document or remove it from template.document_ids"
                                .to_string(),
                        ),
                        artifact_kind: Some("document".to_string()),
                        artifact_id: Some(document_id.clone()),
                        template_id: Some(template_id.clone()),
                        path: Some(path_rendered.clone()),
                    });
                }
            }
        }

        if let Some(snapshot) = self.latest_template_instantiation_snapshot(project.id)? {
            if self
                .read_global_template_artifact(&snapshot.template_id, origin)
                .is_err()
            {
                push_issue(GovernanceDiagnosticIssue {
                    code: "instantiated_template_missing".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Latest instantiated template '{}' no longer exists",
                        snapshot.template_id
                    ),
                    hint: Some(
                        "Recreate the template or instantiate a new valid template for this project"
                            .to_string(),
                    ),
                    artifact_kind: Some("template".to_string()),
                    artifact_id: Some(snapshot.template_id.clone()),
                    template_id: Some(snapshot.template_id.clone()),
                    path: Some(self.global_template_path(&snapshot.template_id).to_string_lossy().to_string()),
                });
            }

            if self
                .read_global_system_prompt_artifact(&snapshot.system_prompt_id, origin)
                .is_err()
            {
                push_issue(GovernanceDiagnosticIssue {
                    code: "instantiated_system_prompt_missing".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Latest instantiated template '{}' references missing system prompt '{}'",
                        snapshot.template_id, snapshot.system_prompt_id
                    ),
                    hint: Some(
                        "Recreate the system prompt and re-instantiate the template".to_string(),
                    ),
                    artifact_kind: Some("system_prompt".to_string()),
                    artifact_id: Some(snapshot.system_prompt_id.clone()),
                    template_id: Some(snapshot.template_id.clone()),
                    path: None,
                });
            }

            for skill_id in &snapshot.skill_ids {
                if self.read_global_skill_artifact(skill_id, origin).is_err() {
                    push_issue(GovernanceDiagnosticIssue {
                        code: "instantiated_skill_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Latest instantiated template '{}' references missing skill '{}'",
                            snapshot.template_id, skill_id
                        ),
                        hint: Some(
                            "Recreate the skill and re-instantiate the template".to_string(),
                        ),
                        artifact_kind: Some("skill".to_string()),
                        artifact_id: Some(skill_id.clone()),
                        template_id: Some(snapshot.template_id.clone()),
                        path: None,
                    });
                }
            }

            for document_id in &snapshot.document_ids {
                if self
                    .read_project_document_artifact(project.id, document_id, origin)
                    .is_err()
                {
                    push_issue(GovernanceDiagnosticIssue {
                        code: "instantiated_document_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Latest instantiated template '{}' references missing project document '{}'",
                            snapshot.template_id, document_id
                        ),
                        hint: Some(
                            "Create the document and re-instantiate the template".to_string(),
                        ),
                        artifact_kind: Some("document".to_string()),
                        artifact_id: Some(document_id.clone()),
                        template_id: Some(snapshot.template_id.clone()),
                        path: None,
                    });
                }
            }
        }

        // Fold deterministic projection drift diagnostics into operator-facing health output.
        let repair_probe = self.build_governance_repair_plan(&project, None, false)?;
        for issue in repair_probe.result.issues {
            push_issue(GovernanceDiagnosticIssue {
                code: issue.code,
                severity: issue.severity,
                message: issue.message,
                hint: issue.hint,
                artifact_kind: issue.artifact_kind,
                artifact_id: issue.artifact_id,
                template_id: None,
                path: issue.path,
            });
        }

        let mut seen = HashSet::new();
        issues.retain(|issue| {
            let key = format!(
                "{}|{}|{}|{}|{}",
                issue.code,
                issue.artifact_id.clone().unwrap_or_default(),
                issue.template_id.clone().unwrap_or_default(),
                issue.path.clone().unwrap_or_default(),
                issue.message
            );
            seen.insert(key)
        });
        issues.sort_by(|a, b| {
            a.code
                .cmp(&b.code)
                .then(a.template_id.cmp(&b.template_id))
                .then(a.artifact_id.cmp(&b.artifact_id))
                .then(a.message.cmp(&b.message))
        });

        let issue_count = issues.len();
        Ok(ProjectGovernanceDiagnosticsResult {
            project_id: project.id,
            checked_at: Utc::now(),
            healthy: issue_count == 0,
            issue_count,
            issues,
        })
    }

// governance_projection_map_for_project (10251-10263)
    fn governance_projection_map_for_project(
        state: &AppState,
        project_id: Uuid,
    ) -> BTreeMap<String, crate::core::state::GovernanceArtifact> {
        state
            .governance_artifacts
            .iter()
            .filter(|(_, artifact)| {
                artifact.project_id == Some(project_id) || artifact.project_id.is_none()
            })
            .map(|(key, artifact)| (key.clone(), artifact.clone()))
            .collect()
    }

// governance_snapshot_summary (10265-10277)
    fn governance_snapshot_summary(
        path: &Path,
        manifest: &GovernanceRecoverySnapshotManifest,
    ) -> GovernanceSnapshotSummary {
        GovernanceSnapshotSummary {
            snapshot_id: manifest.snapshot_id.clone(),
            path: path.to_string_lossy().to_string(),
            created_at: manifest.created_at,
            artifact_count: manifest.artifact_count,
            total_bytes: manifest.total_bytes,
            source_event_sequence: manifest.source_event_sequence,
        }
    }

// project_governance_replay (10279-10347)
    /// Replays governance projections for a project from canonical event history.
    ///
    /// # Errors
    /// Returns an error if event replay fails or verification mismatches are detected.
    pub fn project_governance_replay(
        &self,
        id_or_name: &str,
        verify: bool,
    ) -> Result<ProjectGovernanceReplayResult> {
        let origin = "registry:project_governance_replay";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let all_events = self
            .store
            .read_all()
            .map_err(|e| HivemindError::system("event_read_failed", e.to_string(), origin))?;
        let replayed_once = AppState::replay(&all_events);
        let replayed_twice = AppState::replay(&all_events);
        let current = self.state()?;

        let projection_once =
            Self::governance_projection_map_for_project(&replayed_once, project.id);
        let projection_twice =
            Self::governance_projection_map_for_project(&replayed_twice, project.id);
        let projection_current = Self::governance_projection_map_for_project(&current, project.id);

        let idempotent = projection_once == projection_twice;
        let current_matches_replay = projection_once == projection_current;
        let missing_on_disk = projection_once
            .values()
            .filter(|artifact| !Path::new(&artifact.path).exists())
            .count();

        if verify && (!idempotent || !current_matches_replay || missing_on_disk > 0) {
            let err = HivemindError::system(
                "governance_replay_verification_failed",
                "Governance replay verification failed",
                origin,
            )
            .with_context("missing_on_disk", missing_on_disk.to_string())
            .with_hint(
                "Run 'hivemind project governance repair detect <project>' to inspect projection drift",
            );
            return Err(err);
        }

        let projections = projection_once
            .values()
            .map(|artifact| GovernanceProjectionEntry {
                project_id: artifact.project_id,
                scope: artifact.scope.clone(),
                artifact_kind: artifact.artifact_kind.clone(),
                artifact_key: artifact.artifact_key.clone(),
                path: artifact.path.clone(),
                revision: artifact.revision,
                exists_on_disk: Path::new(&artifact.path).exists(),
            })
            .collect();

        Ok(ProjectGovernanceReplayResult {
            project_id: project.id,
            replayed_at: Utc::now(),
            projection_count: projection_once.len(),
            idempotent,
            current_matches_replay,
            projections,
        })
    }

// project_governance_snapshot_create (10349-10482)
    /// Creates a governance recovery snapshot for a project.
    ///
    /// # Errors
    /// Returns an error if snapshot persistence fails.
    #[allow(clippy::too_many_lines)]
    pub fn project_governance_snapshot_create(
        &self,
        id_or_name: &str,
        interval_minutes: Option<u64>,
    ) -> Result<ProjectGovernanceSnapshotCreateResult> {
        let origin = "registry:project_governance_snapshot_create";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;

        if let Some(minutes) = interval_minutes {
            let manifests = self.list_snapshot_manifests(project.id, origin)?;
            if let Some((path, latest)) = manifests.first() {
                let window_secs = minutes.saturating_mul(60);
                let age_secs = Utc::now()
                    .signed_duration_since(latest.created_at)
                    .num_seconds()
                    .max(0)
                    .cast_unsigned();
                if age_secs <= window_secs {
                    return Ok(ProjectGovernanceSnapshotCreateResult {
                        project_id: project.id,
                        reused_existing: true,
                        interval_minutes: Some(minutes),
                        snapshot: Self::governance_snapshot_summary(path, latest),
                    });
                }
            }
        }

        let state = self.state()?;
        let projection_map = Self::governance_projection_map_for_project(&state, project.id);
        let locations = self.governance_managed_file_locations(project.id, origin)?;

        let mut artifacts = Vec::new();
        let mut total_bytes = 0u64;
        for location in locations {
            if !location.path.is_file() {
                continue;
            }
            let content = fs::read_to_string(&location.path).map_err(|e| {
                HivemindError::system("governance_snapshot_read_failed", e.to_string(), origin)
                    .with_context("path", location.path.to_string_lossy().to_string())
            })?;
            total_bytes = total_bytes.saturating_add(content.len() as u64);
            let projection_key = Self::governance_projection_key_from_parts(
                location.project_id,
                location.scope,
                location.artifact_kind,
                &location.artifact_key,
            );
            let revision = projection_map
                .get(&projection_key)
                .map_or(0, |artifact| artifact.revision);
            artifacts.push(GovernanceRecoverySnapshotEntry {
                path: location.path.to_string_lossy().to_string(),
                scope: location.scope.to_string(),
                artifact_kind: location.artifact_kind.to_string(),
                artifact_key: location.artifact_key,
                project_id: location.project_id,
                revision,
                content_hash: Self::constitution_digest(content.as_bytes()),
                content,
            });
        }
        artifacts.sort_by(|a, b| {
            a.scope
                .cmp(&b.scope)
                .then(a.artifact_kind.cmp(&b.artifact_kind))
                .then(a.artifact_key.cmp(&b.artifact_key))
                .then(a.path.cmp(&b.path))
        });

        let source_event_sequence = self
            .store
            .read_all()
            .ok()
            .and_then(|events| events.last().and_then(|event| event.metadata.sequence));
        let snapshot_id = Uuid::new_v4().to_string();
        let snapshot_dir = self.governance_snapshot_root(project.id);
        fs::create_dir_all(&snapshot_dir).map_err(|e| {
            HivemindError::system("governance_snapshot_write_failed", e.to_string(), origin)
        })?;
        let path = self.governance_snapshot_path(project.id, &snapshot_id);
        let manifest = GovernanceRecoverySnapshotManifest {
            schema_version: GOVERNANCE_RECOVERY_SNAPSHOT_SCHEMA_VERSION.to_string(),
            snapshot_id: snapshot_id.clone(),
            project_id: project.id,
            created_at: Utc::now(),
            source_event_sequence,
            artifact_count: artifacts.len(),
            total_bytes,
            artifacts,
        };
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| {
            HivemindError::system(
                "governance_snapshot_serialize_failed",
                e.to_string(),
                origin,
            )
        })?;
        fs::write(&path, bytes).map_err(|e| {
            HivemindError::system("governance_snapshot_write_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;

        self.append_event(
            Event::new(
                EventPayload::GovernanceSnapshotCreated {
                    project_id: project.id,
                    snapshot_id,
                    path: path.to_string_lossy().to_string(),
                    artifact_count: manifest.artifact_count,
                    total_bytes: manifest.total_bytes,
                    source_event_sequence,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceSnapshotCreateResult {
            project_id: project.id,
            reused_existing: false,
            interval_minutes,
            snapshot: Self::governance_snapshot_summary(&path, &manifest),
        })
    }

// project_governance_snapshot_list (10484-10510)
    /// Lists governance recovery snapshots for a project.
    ///
    /// # Errors
    /// Returns an error if snapshot listing fails.
    pub fn project_governance_snapshot_list(
        &self,
        id_or_name: &str,
        limit: usize,
    ) -> Result<ProjectGovernanceSnapshotListResult> {
        let origin = "registry:project_governance_snapshot_list";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let mut snapshots: Vec<GovernanceSnapshotSummary> = self
            .list_snapshot_manifests(project.id, origin)?
            .into_iter()
            .map(|(path, manifest)| Self::governance_snapshot_summary(&path, &manifest))
            .collect();
        if limit > 0 && snapshots.len() > limit {
            snapshots.truncate(limit);
        }
        Ok(ProjectGovernanceSnapshotListResult {
            project_id: project.id,
            snapshot_count: snapshots.len(),
            snapshots,
        })
    }

// project_governance_snapshot_restore (10512-10648)
    /// Restores governance artifacts from a saved snapshot while preserving event authority.
    ///
    /// # Errors
    /// Returns an error if restore confirmation is missing or snapshot files cannot be restored.
    #[allow(clippy::too_many_lines)]
    pub fn project_governance_snapshot_restore(
        &self,
        id_or_name: &str,
        snapshot_id: &str,
        confirm: bool,
    ) -> Result<ProjectGovernanceSnapshotRestoreResult> {
        let origin = "registry:project_governance_snapshot_restore";
        if !confirm {
            return Err(HivemindError::user(
                "restore_confirmation_required",
                "Snapshot restore requires explicit confirmation",
                origin,
            )
            .with_hint("Re-run with --confirm"));
        }
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
            return Err(HivemindError::user(
                "governance_restore_blocked_active_flow",
                "Cannot restore governance snapshot while project has active flows",
                origin,
            )
            .with_hint(
                "Complete, merge, or abort active flows before restoring governance artifacts",
            ));
        }

        let manifest = self.load_snapshot_manifest(project.id, snapshot_id, origin)?;
        if manifest.schema_version != GOVERNANCE_RECOVERY_SNAPSHOT_SCHEMA_VERSION {
            return Err(HivemindError::user(
                "governance_snapshot_schema_unsupported",
                format!(
                    "Unsupported governance snapshot schema '{}'",
                    manifest.schema_version
                ),
                origin,
            )
            .with_hint("Create a new snapshot with the current Hivemind version"));
        }

        let projection_map = Self::governance_projection_map_for_project(&state, project.id);
        let mut restored_files = 0usize;
        let mut skipped_files = 0usize;
        let mut stale_files = 0usize;

        for artifact in &manifest.artifacts {
            let projection_key = Self::governance_projection_key_from_parts(
                artifact.project_id,
                &artifact.scope,
                &artifact.artifact_kind,
                &artifact.artifact_key,
            );
            let Some(projected) = projection_map.get(&projection_key) else {
                stale_files += 1;
                skipped_files += 1;
                continue;
            };
            if projected.revision != artifact.revision {
                stale_files += 1;
                skipped_files += 1;
                continue;
            }

            let path = PathBuf::from(&artifact.path);
            let current_same = fs::read_to_string(&path).ok().is_some_and(|raw| {
                Self::constitution_digest(raw.as_bytes()) == artifact.content_hash
            });
            if current_same {
                skipped_files += 1;
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    HivemindError::system(
                        "governance_snapshot_restore_failed",
                        e.to_string(),
                        origin,
                    )
                    .with_context("path", path.to_string_lossy().to_string())
                })?;
            }
            fs::write(&path, artifact.content.as_bytes()).map_err(|e| {
                HivemindError::system("governance_snapshot_restore_failed", e.to_string(), origin)
                    .with_context("path", path.to_string_lossy().to_string())
            })?;
            restored_files += 1;
        }

        self.append_event(
            Event::new(
                EventPayload::GovernanceSnapshotRestored {
                    project_id: project.id,
                    snapshot_id: manifest.snapshot_id.clone(),
                    path: self
                        .governance_snapshot_path(project.id, &manifest.snapshot_id)
                        .to_string_lossy()
                        .to_string(),
                    artifact_count: manifest.artifact_count,
                    restored_files,
                    skipped_files,
                    stale_files,
                    repaired_projection_count: 0,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceSnapshotRestoreResult {
            project_id: project.id,
            snapshot_id: manifest.snapshot_id.clone(),
            path: self
                .governance_snapshot_path(project.id, &manifest.snapshot_id)
                .to_string_lossy()
                .to_string(),
            artifact_count: manifest.artifact_count,
            restored_files,
            skipped_files,
            stale_files,
            repaired_projection_count: 0,
        })
    }

// build_governance_repair_plan (10650-10928)
    #[allow(clippy::too_many_lines)]
    fn build_governance_repair_plan(
        &self,
        project: &Project,
        snapshot: Option<&GovernanceRecoverySnapshotManifest>,
        include_operations: bool,
    ) -> Result<GovernanceRepairPlanBundle> {
        let origin = "registry:build_governance_repair_plan";
        let state = self.state()?;
        let projection_map = Self::governance_projection_map_for_project(&state, project.id);
        let mut issues = Vec::new();
        let mut operations = Vec::new();
        let mut internal = Vec::new();
        let mut issue_seen = HashSet::new();
        let mut op_seen = HashSet::new();

        let snapshot_entries: HashMap<String, GovernanceRecoverySnapshotEntry> = snapshot
            .map(|manifest| {
                manifest
                    .artifacts
                    .iter()
                    .map(|entry| {
                        (
                            Self::governance_projection_key_from_parts(
                                entry.project_id,
                                &entry.scope,
                                &entry.artifact_kind,
                                &entry.artifact_key,
                            ),
                            entry.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut push_issue = |issue: GovernanceDriftIssue| {
            let key = format!(
                "{}|{}|{}|{}",
                issue.code,
                issue.path.clone().unwrap_or_default(),
                issue.artifact_kind.clone().unwrap_or_default(),
                issue.artifact_id.clone().unwrap_or_default()
            );
            if issue_seen.insert(key) {
                issues.push(issue);
            }
        };

        for artifact in projection_map.values() {
            let exists = Path::new(&artifact.path).exists();
            if exists {
                continue;
            }
            let projection_key = Self::governance_projection_key_from_parts(
                artifact.project_id,
                &artifact.scope,
                &artifact.artifact_kind,
                &artifact.artifact_key,
            );
            let recoverable = snapshot_entries
                .get(&projection_key)
                .is_some_and(|entry| entry.revision == artifact.revision);
            let issue = GovernanceDriftIssue {
                code: "governance_artifact_missing".to_string(),
                severity: "error".to_string(),
                message: format!("Governance artifact '{}' is missing on disk", artifact.path),
                recoverable,
                artifact_kind: Some(artifact.artifact_kind.clone()),
                artifact_id: Some(artifact.artifact_key.clone()),
                path: Some(artifact.path.clone()),
                hint: if recoverable {
                    Some("Run 'hivemind project governance repair apply <project> --confirm' to restore from snapshot".to_string())
                } else {
                    Some("Create a new governance snapshot to enable deterministic recovery or recreate the artifact manually".to_string())
                },
            };
            push_issue(issue);

            if include_operations && recoverable {
                let op_key = format!("restore_from_snapshot:{}", artifact.path);
                if op_seen.insert(op_key) {
                    let Some(location) =
                        self.governance_location_for_path(project.id, Path::new(&artifact.path))
                    else {
                        continue;
                    };
                    let Some(entry) = snapshot_entries.get(&projection_key).cloned() else {
                        continue;
                    };
                    operations.push(GovernanceRepairOperation {
                        action: "restore_from_snapshot".to_string(),
                        path: artifact.path.clone(),
                        artifact_kind: Some(artifact.artifact_kind.clone()),
                        artifact_id: Some(artifact.artifact_key.clone()),
                        reason: "Missing artifact file with matching snapshot revision".to_string(),
                    });
                    internal
                        .push(GovernanceRepairInternalOp::RestoreFromSnapshot { location, entry });
                }
            }
        }

        for location in self.governance_managed_file_locations(project.id, origin)? {
            let projection_key = Self::governance_projection_key_from_parts(
                location.project_id,
                location.scope,
                location.artifact_kind,
                &location.artifact_key,
            );
            if !projection_map.contains_key(&projection_key) {
                push_issue(GovernanceDriftIssue {
                    code: "governance_projection_missing".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Governance projection is missing for artifact '{}'",
                        location.path.to_string_lossy()
                    ),
                    recoverable: true,
                    artifact_kind: Some(location.artifact_kind.to_string()),
                    artifact_id: Some(location.artifact_key.clone()),
                    path: Some(location.path.to_string_lossy().to_string()),
                    hint: Some(
                        "Run 'hivemind project governance repair apply <project> --confirm' to rebuild projection metadata".to_string(),
                    ),
                });
                if include_operations {
                    let op_key =
                        format!("emit_projection_upsert:{}", location.path.to_string_lossy());
                    if op_seen.insert(op_key) {
                        operations.push(GovernanceRepairOperation {
                            action: "emit_projection_upsert".to_string(),
                            path: location.path.to_string_lossy().to_string(),
                            artifact_kind: Some(location.artifact_kind.to_string()),
                            artifact_id: Some(location.artifact_key.clone()),
                            reason:
                                "Artifact exists on disk but lacks governance projection metadata"
                                    .to_string(),
                        });
                        internal.push(GovernanceRepairInternalOp::EmitUpsert {
                            location: location.clone(),
                        });
                    }
                }
            }

            if location.path.extension().is_some_and(|ext| ext == "json") {
                let raw = fs::read_to_string(&location.path).map_err(|e| {
                    HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                        .with_context("path", location.path.to_string_lossy().to_string())
                })?;
                if serde_json::from_str::<serde_json::Value>(&raw).is_err() {
                    let recoverable =
                        projection_map
                            .get(&projection_key)
                            .is_some_and(|projected| {
                                snapshot_entries
                                    .get(&projection_key)
                                    .is_some_and(|entry| entry.revision == projected.revision)
                            });
                    push_issue(GovernanceDriftIssue {
                        code: "governance_artifact_schema_invalid".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Governance artifact '{}' contains malformed JSON",
                            location.path.to_string_lossy()
                        ),
                        recoverable,
                        artifact_kind: Some(location.artifact_kind.to_string()),
                        artifact_id: Some(location.artifact_key.clone()),
                        path: Some(location.path.to_string_lossy().to_string()),
                        hint: if recoverable {
                            Some("Run 'hivemind project governance repair apply <project> --confirm' to restore valid content from snapshot".to_string())
                        } else {
                            Some("Repair the JSON artifact manually or restore from a matching snapshot".to_string())
                        },
                    });
                    if include_operations && recoverable {
                        let op_key =
                            format!("restore_from_snapshot:{}", location.path.to_string_lossy());
                        if op_seen.insert(op_key) {
                            if let Some(entry) = snapshot_entries.get(&projection_key).cloned() {
                                operations.push(GovernanceRepairOperation {
                                    action: "restore_from_snapshot".to_string(),
                                    path: location.path.to_string_lossy().to_string(),
                                    artifact_kind: Some(location.artifact_kind.to_string()),
                                    artifact_id: Some(location.artifact_key.clone()),
                                    reason: "Malformed JSON with matching snapshot revision"
                                        .to_string(),
                                });
                                internal.push(GovernanceRepairInternalOp::RestoreFromSnapshot {
                                    location,
                                    entry,
                                });
                            }
                        }
                    }
                }
            }
        }

        if !project.repositories.is_empty() {
            let graph_snapshot_path = self
                .graph_snapshot_path(project.id)
                .to_string_lossy()
                .to_string();
            if let Err(err) = self.ensure_graph_snapshot_current_for_constitution(project, origin) {
                let graph_issue = matches!(
                    err.code.as_str(),
                    "graph_snapshot_missing"
                        | "graph_snapshot_stale"
                        | "graph_snapshot_integrity_invalid"
                        | "graph_snapshot_profile_mismatch"
                        | "graph_snapshot_schema_invalid"
                );
                push_issue(GovernanceDriftIssue {
                    code: err.code,
                    severity: "error".to_string(),
                    message: err.message,
                    recoverable: graph_issue,
                    artifact_kind: Some("graph_snapshot".to_string()),
                    artifact_id: Some("graph_snapshot.json".to_string()),
                    path: Some(graph_snapshot_path.clone()),
                    hint: err.recovery_hint,
                });
                if include_operations && graph_issue {
                    let op_key = "refresh_graph_snapshot".to_string();
                    if op_seen.insert(op_key) {
                        operations.push(GovernanceRepairOperation {
                            action: "refresh_graph_snapshot".to_string(),
                            path: graph_snapshot_path,
                            artifact_kind: Some("graph_snapshot".to_string()),
                            artifact_id: Some("graph_snapshot.json".to_string()),
                            reason: "Graph snapshot drift is recoverable by deterministic refresh"
                                .to_string(),
                        });
                        internal.push(GovernanceRepairInternalOp::RefreshGraphSnapshot {
                            project_key: project.id.to_string(),
                        });
                    }
                }
            }
        }

        issues.sort_by(|a, b| {
            a.code
                .cmp(&b.code)
                .then(a.path.cmp(&b.path))
                .then(a.artifact_kind.cmp(&b.artifact_kind))
                .then(a.artifact_id.cmp(&b.artifact_id))
        });
        operations.sort_by(|a, b| {
            a.action
                .cmp(&b.action)
                .then(a.path.cmp(&b.path))
                .then(a.artifact_kind.cmp(&b.artifact_kind))
        });

        let issue_count = issues.len();
        let recoverable_issue_count = issues.iter().filter(|item| item.recoverable).count();
        let unrecoverable_issue_count = issue_count.saturating_sub(recoverable_issue_count);
        let result = ProjectGovernanceRepairPlanResult {
            project_id: project.id,
            checked_at: Utc::now(),
            healthy: issue_count == 0,
            issue_count,
            recoverable_issue_count,
            unrecoverable_issue_count,
            snapshot_id: snapshot.map(|item| item.snapshot_id.clone()),
            ready_to_apply: issue_count == 0 || unrecoverable_issue_count == 0,
            issues,
            operations,
        };

        Ok(GovernanceRepairPlanBundle {
            result,
            operations: internal,
        })
    }

// project_governance_repair_detect (10930-10956)
    /// Detects governance drift against canonical event history.
    ///
    /// # Errors
    /// Returns an error if project resolution or replay fails.
    pub fn project_governance_repair_detect(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceRepairPlanResult> {
        let origin = "registry:project_governance_repair_detect";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let plan = self.build_governance_repair_plan(&project, None, false)?;
        self.append_event(
            Event::new(
                EventPayload::GovernanceDriftDetected {
                    project_id: project.id,
                    issue_count: plan.result.issue_count,
                    recoverable_count: plan.result.recoverable_issue_count,
                    unrecoverable_count: plan.result.unrecoverable_issue_count,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;
        Ok(plan.result)
    }

// project_governance_repair_preview (10958-10989)
    /// Previews deterministic governance repair operations.
    ///
    /// # Errors
    /// Returns an error if snapshot loading fails.
    pub fn project_governance_repair_preview(
        &self,
        id_or_name: &str,
        snapshot_id: Option<&str>,
    ) -> Result<ProjectGovernanceRepairPlanResult> {
        let origin = "registry:project_governance_repair_preview";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let snapshot = match snapshot_id {
            Some(id) => Some(self.load_snapshot_manifest(project.id, id, origin)?),
            None => None,
        };
        let plan = self.build_governance_repair_plan(&project, snapshot.as_ref(), true)?;
        self.append_event(
            Event::new(
                EventPayload::GovernanceDriftDetected {
                    project_id: project.id,
                    issue_count: plan.result.issue_count,
                    recoverable_count: plan.result.recoverable_issue_count,
                    unrecoverable_count: plan.result.unrecoverable_issue_count,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;
        Ok(plan.result)
    }

// project_governance_repair_apply (10991-11118)
    /// Applies deterministic governance repair operations.
    ///
    /// # Errors
    /// Returns an error if confirmation is missing, active flows exist, or repair cannot fully proceed.
    #[allow(clippy::too_many_lines)]
    pub fn project_governance_repair_apply(
        &self,
        id_or_name: &str,
        snapshot_id: Option<&str>,
        confirm: bool,
    ) -> Result<ProjectGovernanceRepairApplyResult> {
        let origin = "registry:project_governance_repair_apply";
        if !confirm {
            return Err(HivemindError::user(
                "repair_confirmation_required",
                "Governance repair apply requires explicit confirmation",
                origin,
            )
            .with_hint("Re-run with --confirm"));
        }
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
            return Err(HivemindError::user(
                "governance_repair_blocked_active_flow",
                "Cannot apply governance repair while project has active flows",
                origin,
            )
            .with_hint(
                "Complete, merge, or abort active flows before applying governance repair",
            ));
        }

        let snapshot = match snapshot_id {
            Some(id) => Some(self.load_snapshot_manifest(project.id, id, origin)?),
            None => None,
        };
        let plan = self.build_governance_repair_plan(&project, snapshot.as_ref(), true)?;
        if !plan.result.healthy && !plan.result.ready_to_apply {
            return Err(HivemindError::user(
                "governance_repair_unrecoverable",
                "Governance repair plan contains unrecoverable issues",
                origin,
            )
            .with_hint("Run 'hivemind project governance repair preview <project>' and resolve unrecoverable items manually"));
        }

        let mut applied_operations = Vec::new();
        let mut pending = HashMap::new();
        let replay_state = self.state()?;
        for (public_op, internal_op) in plan.result.operations.iter().zip(plan.operations.iter()) {
            match internal_op {
                GovernanceRepairInternalOp::EmitUpsert { location } => {
                    self.append_governance_upsert_for_location(
                        &replay_state,
                        &mut pending,
                        location,
                        CorrelationIds::for_project(project.id),
                        origin,
                    )?;
                }
                GovernanceRepairInternalOp::RestoreFromSnapshot { location, entry } => {
                    if let Some(parent) = Path::new(&entry.path).parent() {
                        fs::create_dir_all(parent).map_err(|e| {
                            HivemindError::system(
                                "governance_repair_apply_failed",
                                e.to_string(),
                                origin,
                            )
                        })?;
                    }
                    fs::write(&entry.path, entry.content.as_bytes()).map_err(|e| {
                        HivemindError::system(
                            "governance_repair_apply_failed",
                            e.to_string(),
                            origin,
                        )
                        .with_context("path", entry.path.clone())
                    })?;
                    self.append_governance_upsert_for_location(
                        &replay_state,
                        &mut pending,
                        location,
                        CorrelationIds::for_project(project.id),
                        origin,
                    )?;
                }
                GovernanceRepairInternalOp::RefreshGraphSnapshot { project_key } => {
                    self.graph_snapshot_refresh(project_key, "governance_repair")?;
                }
            }
            applied_operations.push(public_op.clone());
        }

        let remaining = self.project_governance_repair_detect(&project.id.to_string())?;
        self.append_event(
            Event::new(
                EventPayload::GovernanceRepairApplied {
                    project_id: project.id,
                    operation_count: applied_operations.len(),
                    repaired_count: applied_operations.len(),
                    remaining_issue_count: remaining.issue_count,
                    snapshot_id: snapshot.as_ref().map(|item| item.snapshot_id.clone()),
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceRepairApplyResult {
            project_id: project.id,
            applied_at: Utc::now(),
            snapshot_id: snapshot.map(|item| item.snapshot_id),
            operation_count: applied_operations.len(),
            applied_operations,
            remaining_issue_count: remaining.issue_count,
            remaining_issues: remaining.issues,
        })
    }

// constitution_check (12287-12299)
    pub fn constitution_check(&self, id_or_name: &str) -> Result<ProjectConstitutionCheckResult> {
        let origin = "registry:constitution_check";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.run_constitution_check(
            &project,
            "manual_check",
            &CorrelationIds::for_project(project.id),
            true,
            origin,
        )
    }

// constitution_init (12301-12422)
    /// Initializes and validates a project's constitution artifact.
    ///
    /// # Errors
    /// Returns an error if explicit confirmation is missing, project resolution fails,
    /// constitution schema is invalid, or validation checks fail.
    #[allow(clippy::too_many_lines)]
    pub fn constitution_init(
        &self,
        id_or_name: &str,
        content: Option<&str>,
        confirmed: bool,
        actor: Option<&str>,
        intent: Option<&str>,
    ) -> Result<ProjectConstitutionMutationResult> {
        let origin = "registry:constitution_init";
        if !confirmed {
            return Err(HivemindError::user(
                "constitution_confirmation_required",
                "Constitution mutation requires explicit confirmation",
                origin,
            )
            .with_hint("Re-run with --confirm"));
        }
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_graph_snapshot_current_for_constitution(&project, origin)?;
        self.ensure_governance_layout(project.id, origin)?;
        let location = self.governance_constitution_location(project.id);

        let artifact = if let Some(raw) = content {
            Self::parse_constitution_yaml(raw, origin)?
        } else if location.path.is_file() {
            let (artifact, _) = self.read_constitution_artifact(project.id, origin)?;
            artifact
        } else {
            Self::default_constitution_artifact()
        };

        let issues = Self::validate_constitution(&artifact);
        if !issues.is_empty() {
            return Err(HivemindError::user(
                "constitution_validation_failed",
                format!(
                    "Constitution initialization failed with {} validation issue(s)",
                    issues.len()
                ),
                origin,
            )
            .with_context(
                "issues",
                serde_json::to_string(&issues).unwrap_or_else(|_| "[]".to_string()),
            )
            .with_hint("Run 'hivemind constitution validate <project>' to inspect the issues"));
        }

        let state = self.state()?;
        let already_initialized = state
            .projects
            .get(&project.id)
            .and_then(|p| p.constitution_digest.as_ref())
            .is_some();
        if already_initialized {
            return Err(HivemindError::user(
                "constitution_already_initialized",
                "Constitution is already initialized for this project",
                origin,
            )
            .with_hint(
                "Use 'hivemind constitution update <project> --confirm ...' for mutations",
            ));
        }

        let digest = Self::write_constitution_artifact(&location.path, &artifact, origin)?;
        let mut pending = HashMap::new();
        let revision = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        let actor_value = Self::resolve_actor(actor);
        let mutation_intent = intent
            .and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| "initialize constitution".to_string());
        self.append_event(
            Event::new(
                EventPayload::ConstitutionInitialized {
                    project_id: project.id,
                    path: location.path.to_string_lossy().to_string(),
                    schema_version: artifact.schema_version.clone(),
                    constitution_version: artifact.version,
                    digest: digest.clone(),
                    revision,
                    actor: actor_value.clone(),
                    mutation_intent: mutation_intent.clone(),
                    confirmed,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectConstitutionMutationResult {
            project_id: project.id,
            path: location.path.to_string_lossy().to_string(),
            revision,
            digest,
            previous_digest: None,
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            actor: actor_value,
            mutation_intent,
            confirmed,
            updated_at: Utc::now(),
        })
    }

// constitution_show (12424-12455)
    /// Shows the current project constitution.
    ///
    /// # Errors
    /// Returns an error if the project or constitution cannot be resolved.
    pub fn constitution_show(&self, id_or_name: &str) -> Result<ProjectConstitutionShowResult> {
        let origin = "registry:constitution_show";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;
        let (artifact, path) = self.read_constitution_artifact(project.id, origin)?;
        let raw = fs::read(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        let digest = Self::constitution_digest(raw.as_slice());
        let location = self.governance_constitution_location(project.id);
        let revision = Self::governance_projection_for_location(&state, &location)
            .map_or(0, |item| item.revision);

        Ok(ProjectConstitutionShowResult {
            project_id: project.id,
            path: path.to_string_lossy().to_string(),
            revision,
            digest,
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            compatibility: artifact.compatibility,
            partitions: artifact.partitions,
            rules: artifact.rules,
        })
    }

// constitution_validate (12457-12513)
    /// Validates project constitution schema and semantics and emits a validation event.
    ///
    /// # Errors
    /// Returns an error if project resolution fails, constitution cannot be loaded,
    /// or event append fails.
    pub fn constitution_validate(
        &self,
        id_or_name: &str,
        validated_by: Option<&str>,
    ) -> Result<ProjectConstitutionValidationResult> {
        let origin = "registry:constitution_validate";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_graph_snapshot_current_for_constitution(&project, origin)?;
        let (artifact, path) = self.read_constitution_artifact(project.id, origin)?;
        let issues = Self::validate_constitution(&artifact);
        let valid = issues.is_empty();
        let raw = fs::read(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        let digest = Self::constitution_digest(raw.as_slice());
        let validated_by = Self::resolve_actor(validated_by);

        self.append_event(
            Event::new(
                EventPayload::ConstitutionValidated {
                    project_id: project.id,
                    path: path.to_string_lossy().to_string(),
                    schema_version: artifact.schema_version.clone(),
                    constitution_version: artifact.version,
                    digest: digest.clone(),
                    valid,
                    issues: issues
                        .iter()
                        .map(|issue| format!("{}:{}:{}", issue.code, issue.field, issue.message))
                        .collect(),
                    validated_by: validated_by.clone(),
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectConstitutionValidationResult {
            project_id: project.id,
            path: path.to_string_lossy().to_string(),
            digest,
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            valid,
            issues,
            validated_by,
            validated_at: Utc::now(),
        })
    }

// constitution_update (12515-12644)
    /// Updates a project's constitution with explicit confirmation and audit metadata.
    ///
    /// # Errors
    /// Returns an error if confirmation is missing, content is invalid,
    /// constitution is not initialized, or validation fails.
    #[allow(clippy::too_many_lines)]
    pub fn constitution_update(
        &self,
        id_or_name: &str,
        content: &str,
        confirmed: bool,
        actor: Option<&str>,
        intent: Option<&str>,
    ) -> Result<ProjectConstitutionMutationResult> {
        let origin = "registry:constitution_update";
        if !confirmed {
            return Err(HivemindError::user(
                "constitution_confirmation_required",
                "Constitution mutation requires explicit confirmation",
                origin,
            )
            .with_hint("Re-run with --confirm"));
        }
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "constitution_content_missing",
                "Constitution update requires non-empty YAML content",
                origin,
            )
            .with_hint("Pass --content '<yaml>' or --from-file <path>"));
        }

        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_graph_snapshot_current_for_constitution(&project, origin)?;
        self.ensure_governance_layout(project.id, origin)?;

        let state = self.state()?;
        let previous_digest = state
            .projects
            .get(&project.id)
            .and_then(|p| p.constitution_digest.clone())
            .ok_or_else(|| {
                HivemindError::user(
                    "constitution_not_initialized",
                    "Constitution is not initialized for this project",
                    origin,
                )
                .with_hint("Run 'hivemind constitution init <project> --confirm' first")
            })?;

        let location = self.governance_constitution_location(project.id);
        if !location.path.is_file() {
            return Err(HivemindError::user(
                "constitution_not_found",
                "Project constitution is missing",
                origin,
            )
            .with_hint("Run 'hivemind constitution init <project> --confirm' to re-create it"));
        }

        let artifact = Self::parse_constitution_yaml(content, origin)?;
        let issues = Self::validate_constitution(&artifact);
        if !issues.is_empty() {
            return Err(HivemindError::user(
                "constitution_validation_failed",
                format!(
                    "Constitution update failed with {} validation issue(s)",
                    issues.len()
                ),
                origin,
            )
            .with_context(
                "issues",
                serde_json::to_string(&issues).unwrap_or_else(|_| "[]".to_string()),
            )
            .with_hint("Run 'hivemind constitution validate <project>' to inspect the issues"));
        }

        let digest = Self::write_constitution_artifact(&location.path, &artifact, origin)?;
        let mut pending = HashMap::new();
        let revision = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        let actor_value = Self::resolve_actor(actor);
        let mutation_intent = intent
            .and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| "update constitution".to_string());
        self.append_event(
            Event::new(
                EventPayload::ConstitutionUpdated {
                    project_id: project.id,
                    path: location.path.to_string_lossy().to_string(),
                    schema_version: artifact.schema_version.clone(),
                    constitution_version: artifact.version,
                    previous_digest: previous_digest.clone(),
                    digest: digest.clone(),
                    revision,
                    actor: actor_value.clone(),
                    mutation_intent: mutation_intent.clone(),
                    confirmed,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectConstitutionMutationResult {
            project_id: project.id,
            path: location.path.to_string_lossy().to_string(),
            revision,
            digest,
            previous_digest: Some(previous_digest),
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            actor: actor_value,
            mutation_intent,
            confirmed,
            updated_at: Utc::now(),
        })
    }

// project_governance_document_create (12646-12734)
    /// Creates a project governance document with immutable revision history.
    pub fn project_governance_document_create(
        &self,
        id_or_name: &str,
        document_id: &str,
        title: &str,
        owner: &str,
        tags: &[String],
        content: &str,
    ) -> Result<ProjectGovernanceDocumentWriteResult> {
        let origin = "registry:project_governance_document_create";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        if title.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_document_title",
                "Document title cannot be empty",
                origin,
            )
            .with_hint("Pass --title with a non-empty value"));
        }
        if owner.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_document_owner",
                "Document owner cannot be empty",
                origin,
            )
            .with_hint("Pass --owner with a non-empty value"));
        }
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_document_content",
                "Document content cannot be empty",
                origin,
            )
            .with_hint("Pass --content with a non-empty value"));
        }

        self.ensure_governance_layout(project.id, origin)?;
        let path = self.project_document_path(project.id, &document_id);
        if path.exists() {
            return Err(HivemindError::user(
                "document_exists",
                format!("Project document '{document_id}' already exists"),
                origin,
            )
            .with_hint(
                "Use 'hivemind project governance document update' to create a new revision",
            ));
        }

        let now = Utc::now();
        let artifact = ProjectDocumentArtifact {
            document_id: document_id.clone(),
            title: title.trim().to_string(),
            owner: owner.trim().to_string(),
            tags: Self::normalized_string_list(tags),
            updated_at: now,
            revisions: vec![ProjectDocumentRevision {
                revision: 1,
                content: content.to_string(),
                updated_at: now,
            }],
        };
        Self::write_governance_json(&path, &artifact, origin)?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = self.governance_document_location(project.id, &document_id);
        let event_revision = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(ProjectGovernanceDocumentWriteResult {
            project_id: project.id,
            document_id,
            revision: event_revision,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            updated_at: now,
        })
    }

// project_governance_document_list (12736-12768)
    /// Lists project governance documents.
    pub fn project_governance_document_list(
        &self,
        id_or_name: &str,
    ) -> Result<Vec<ProjectGovernanceDocumentSummary>> {
        let origin = "registry:project_governance_document_list";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;

        let mut out = Vec::new();
        for path in Self::governance_json_paths(
            &self.governance_project_root(project.id).join("documents"),
            origin,
        )? {
            let key = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let artifact = Self::read_governance_json::<ProjectDocumentArtifact>(
                &path,
                "project_document",
                key,
                origin,
            )?;
            out.push(Self::project_document_summary_from_artifact(
                project.id, &artifact, &path,
            ));
        }
        out.sort_by(|a, b| a.document_id.cmp(&b.document_id));
        Ok(out)
    }

// project_governance_document_inspect (12770-12794)
    /// Inspects a project governance document with full revision history.
    pub fn project_governance_document_inspect(
        &self,
        id_or_name: &str,
        document_id: &str,
    ) -> Result<ProjectGovernanceDocumentInspectResult> {
        let origin = "registry:project_governance_document_inspect";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        let path = self.project_document_path(project.id, &document_id);
        let artifact = self.read_project_document_artifact(project.id, &document_id, origin)?;
        let summary = Self::project_document_summary_from_artifact(project.id, &artifact, &path);
        let latest_content = artifact
            .revisions
            .last()
            .map_or_else(String::new, |item| item.content.clone());

        Ok(ProjectGovernanceDocumentInspectResult {
            summary,
            revisions: artifact.revisions,
            latest_content,
        })
    }

// project_governance_document_update (12796-12909)
    /// Updates a project governance document, optionally creating a new immutable revision.
    pub fn project_governance_document_update(
        &self,
        id_or_name: &str,
        document_id: &str,
        title: Option<&str>,
        owner: Option<&str>,
        tags: Option<&[String]>,
        content: Option<&str>,
    ) -> Result<ProjectGovernanceDocumentWriteResult> {
        let origin = "registry:project_governance_document_update";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        let mut artifact = self.read_project_document_artifact(project.id, &document_id, origin)?;
        let path = self.project_document_path(project.id, &document_id);

        let mut changed = false;
        if let Some(title) = title {
            if title.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_document_title",
                    "Document title cannot be empty",
                    origin,
                )
                .with_hint("Pass --title with a non-empty value"));
            }
            if artifact.title != title.trim() {
                artifact.title = title.trim().to_string();
                changed = true;
            }
        }
        if let Some(owner) = owner {
            if owner.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_document_owner",
                    "Document owner cannot be empty",
                    origin,
                )
                .with_hint("Pass --owner with a non-empty value"));
            }
            if artifact.owner != owner.trim() {
                artifact.owner = owner.trim().to_string();
                changed = true;
            }
        }
        if let Some(tags) = tags {
            let normalized = Self::normalized_string_list(tags);
            if artifact.tags != normalized {
                artifact.tags = normalized;
                changed = true;
            }
        }

        let now = Utc::now();
        if let Some(content) = content {
            if content.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_document_content",
                    "Document content cannot be empty",
                    origin,
                )
                .with_hint("Pass --content with a non-empty value"));
            }
            let next_revision = artifact
                .revisions
                .last()
                .map_or(1, |item| item.revision.saturating_add(1));
            artifact.revisions.push(ProjectDocumentRevision {
                revision: next_revision,
                content: content.to_string(),
                updated_at: now,
            });
            changed = true;
        }

        if !changed {
            let revision = artifact.revisions.last().map_or(0, |item| item.revision);
            return Ok(ProjectGovernanceDocumentWriteResult {
                project_id: project.id,
                document_id,
                revision,
                path: path.to_string_lossy().to_string(),
                schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                projection_version: GOVERNANCE_PROJECTION_VERSION,
                updated_at: artifact.updated_at,
            });
        }

        artifact.updated_at = now;
        Self::write_governance_json(&path, &artifact, origin)?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = self.governance_document_location(project.id, &document_id);
        let event_revision = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(ProjectGovernanceDocumentWriteResult {
            project_id: project.id,
            document_id,
            revision: event_revision,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            updated_at: now,
        })
    }

// project_governance_document_delete (12911-12953)
    /// Deletes a project governance document.
    pub fn project_governance_document_delete(
        &self,
        id_or_name: &str,
        document_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:project_governance_document_delete";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        let path = self.project_document_path(project.id, &document_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "document_not_found",
                format!("Project document '{document_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind project governance document list <project>' to inspect available documents"));
        }

        fs::remove_file(&path).map_err(|e| {
            HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;

        let location = self.governance_document_location(project.id, &document_id);
        self.append_governance_delete_for_location(
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: Some(project.id),
            scope: "project".to_string(),
            artifact_kind: "document".to_string(),
            artifact_key: document_id,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// project_governance_attachment_set_document (12955-13009)
    /// Sets attachment state for a project document on a task.
    pub fn project_governance_attachment_set_document(
        &self,
        id_or_name: &str,
        task_id: &str,
        document_id: &str,
        attached: bool,
    ) -> Result<GovernanceAttachmentUpdateResult> {
        let origin = "registry:project_governance_attachment_set_document";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let task = self.get_task(task_id).inspect_err(|err| {
            self.record_error_event(err, CorrelationIds::for_project(project.id));
        })?;
        if task.project_id != project.id {
            return Err(HivemindError::user(
                "task_project_mismatch",
                format!(
                    "Task '{}' does not belong to project '{}'",
                    task.id, project.id
                ),
                origin,
            )
            .with_hint("Pass a task ID that belongs to the selected project"));
        }
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        let _ = self.read_project_document_artifact(project.id, &document_id, origin)?;

        self.append_event(
            Event::new(
                EventPayload::GovernanceAttachmentLifecycleUpdated {
                    project_id: project.id,
                    task_id: task.id,
                    artifact_kind: "document".to_string(),
                    artifact_key: document_id.clone(),
                    attached,
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                CorrelationIds::for_task(project.id, task.id),
            ),
            origin,
        )?;

        Ok(GovernanceAttachmentUpdateResult {
            project_id: project.id,
            task_id: task.id,
            artifact_kind: "document".to_string(),
            artifact_key: document_id,
            attached,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// project_governance_notepad_create (13011-13068)
    /// Creates project notepad content (non-executional, non-validating artifact).
    pub fn project_governance_notepad_create(
        &self,
        id_or_name: &str,
        content: &str,
    ) -> Result<GovernanceNotepadResult> {
        let origin = "registry:project_governance_notepad_create";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;

        let location = self.governance_notepad_location(Some(project.id));
        let existing = fs::read_to_string(&location.path).unwrap_or_default();
        if !existing.trim().is_empty() {
            return Err(HivemindError::user(
                "notepad_exists",
                "Project notepad already has content",
                origin,
            )
            .with_hint(
                "Use 'hivemind project governance notepad update <project>' to replace content",
            ));
        }
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_notepad_content",
                "Notepad content cannot be empty",
                origin,
            ));
        }

        fs::write(&location.path, content).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(GovernanceNotepadResult {
            scope: "project".to_string(),
            project_id: Some(project.id),
            path: location.path.to_string_lossy().to_string(),
            exists: true,
            content: Some(content.to_string()),
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// project_governance_notepad_show (13070-13104)
    /// Shows project notepad content.
    pub fn project_governance_notepad_show(
        &self,
        id_or_name: &str,
    ) -> Result<GovernanceNotepadResult> {
        let origin = "registry:project_governance_notepad_show";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;
        let location = self.governance_notepad_location(Some(project.id));
        let raw_content = if location.path.is_file() {
            Some(fs::read_to_string(&location.path).map_err(|e| {
                HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
            })?)
        } else {
            None
        };
        let (exists, content) = match raw_content {
            Some(content) if content.trim().is_empty() => (false, None),
            Some(content) => (true, Some(content)),
            None => (false, None),
        };
        Ok(GovernanceNotepadResult {
            scope: "project".to_string(),
            project_id: Some(project.id),
            path: location.path.to_string_lossy().to_string(),
            exists,
            content,
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// project_governance_notepad_update (13106-13150)
    /// Updates project notepad content.
    pub fn project_governance_notepad_update(
        &self,
        id_or_name: &str,
        content: &str,
    ) -> Result<GovernanceNotepadResult> {
        let origin = "registry:project_governance_notepad_update";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_notepad_content",
                "Notepad content cannot be empty",
                origin,
            ));
        }
        let location = self.governance_notepad_location(Some(project.id));
        fs::write(&location.path, content).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(GovernanceNotepadResult {
            scope: "project".to_string(),
            project_id: Some(project.id),
            path: location.path.to_string_lossy().to_string(),
            exists: true,
            content: Some(content.to_string()),
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// project_governance_notepad_delete (13152-13183)
    /// Deletes project notepad content.
    pub fn project_governance_notepad_delete(
        &self,
        id_or_name: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:project_governance_notepad_delete";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;
        let location = self.governance_notepad_location(Some(project.id));
        if location.path.exists() {
            fs::remove_file(&location.path).map_err(|e| {
                HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
            })?;
        }
        self.append_governance_delete_for_location(
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: Some(project.id),
            scope: "project".to_string(),
            artifact_kind: "notepad".to_string(),
            artifact_key: "notepad.md".to_string(),
            path: location.path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// global_skill_create (13185-13249)
    /// Creates a global skill artifact.
    pub fn global_skill_create(
        &self,
        skill_id: &str,
        name: &str,
        tags: &[String],
        content: &str,
    ) -> Result<GlobalSkillSummary> {
        let origin = "registry:global_skill_create";
        self.ensure_global_governance_layout(origin)?;
        let skill_id = Self::validate_governance_identifier(skill_id, "skill_id", origin)?;
        if name.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_skill_name",
                "Skill name cannot be empty",
                origin,
            ));
        }
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_skill_content",
                "Skill content cannot be empty",
                origin,
            ));
        }

        let path = self.global_skill_path(&skill_id);
        if path.exists() {
            return Err(HivemindError::user(
                "skill_exists",
                format!("Global skill '{skill_id}' already exists"),
                origin,
            )
            .with_hint("Use 'hivemind global skill update <skill-id>' to mutate this skill"));
        }

        let now = Utc::now();
        let artifact = GlobalSkillArtifact {
            skill_id: skill_id.clone(),
            name: name.trim().to_string(),
            tags: Self::normalized_string_list(tags),
            content: content.to_string(),
            updated_at: now,
        };
        Self::write_governance_json(&path, &artifact, origin)?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = Self::governance_global_location("skill", &skill_id, path.clone());
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GlobalSkillSummary {
            skill_id,
            name: artifact.name,
            tags: artifact.tags,
            updated_at: now,
            path: path.to_string_lossy().to_string(),
        })
    }

// global_skill_list (13251-13279)
    /// Lists global skills.
    pub fn global_skill_list(&self) -> Result<Vec<GlobalSkillSummary>> {
        let origin = "registry:global_skill_list";
        self.ensure_global_governance_layout(origin)?;
        let mut out = Vec::new();
        for path in
            Self::governance_json_paths(&self.governance_global_root().join("skills"), origin)?
        {
            let key = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let artifact = Self::read_governance_json::<GlobalSkillArtifact>(
                &path,
                "global_skill",
                key,
                origin,
            )?;
            out.push(GlobalSkillSummary {
                skill_id: artifact.skill_id,
                name: artifact.name,
                tags: artifact.tags,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            });
        }
        out.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
        Ok(out)
    }

// global_skill_inspect (13281-13297)
    /// Inspects a global skill.
    pub fn global_skill_inspect(&self, skill_id: &str) -> Result<GlobalSkillInspectResult> {
        let origin = "registry:global_skill_inspect";
        let skill_id = Self::validate_governance_identifier(skill_id, "skill_id", origin)?;
        let path = self.global_skill_path(&skill_id);
        let artifact = self.read_global_skill_artifact(&skill_id, origin)?;
        Ok(GlobalSkillInspectResult {
            summary: GlobalSkillSummary {
                skill_id,
                name: artifact.name,
                tags: artifact.tags,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            },
            content: artifact.content,
        })
    }

// global_skill_update (13299-13369)
    /// Updates a global skill.
    pub fn global_skill_update(
        &self,
        skill_id: &str,
        name: Option<&str>,
        tags: Option<&[String]>,
        content: Option<&str>,
    ) -> Result<GlobalSkillSummary> {
        let origin = "registry:global_skill_update";
        let skill_id = Self::validate_governance_identifier(skill_id, "skill_id", origin)?;
        let path = self.global_skill_path(&skill_id);
        let mut artifact = self.read_global_skill_artifact(&skill_id, origin)?;
        let mut changed = false;

        if let Some(name) = name {
            if name.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_skill_name",
                    "Skill name cannot be empty",
                    origin,
                ));
            }
            if artifact.name != name.trim() {
                artifact.name = name.trim().to_string();
                changed = true;
            }
        }
        if let Some(tags) = tags {
            let normalized = Self::normalized_string_list(tags);
            if artifact.tags != normalized {
                artifact.tags = normalized;
                changed = true;
            }
        }
        if let Some(content) = content {
            if content.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_skill_content",
                    "Skill content cannot be empty",
                    origin,
                ));
            }
            if artifact.content != content {
                artifact.content = content.to_string();
                changed = true;
            }
        }

        if changed {
            artifact.updated_at = Utc::now();
            Self::write_governance_json(&path, &artifact, origin)?;
            let state = self.state()?;
            let mut pending = HashMap::new();
            let location = Self::governance_global_location("skill", &skill_id, path.clone());
            let _ = self.append_governance_upsert_for_location(
                &state,
                &mut pending,
                &location,
                CorrelationIds::none(),
                origin,
            )?;
        }

        Ok(GlobalSkillSummary {
            skill_id,
            name: artifact.name,
            tags: artifact.tags,
            updated_at: artifact.updated_at,
            path: path.to_string_lossy().to_string(),
        })
    }

// global_skill_delete (13371-13399)
    /// Deletes a global skill.
    pub fn global_skill_delete(&self, skill_id: &str) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:global_skill_delete";
        let skill_id = Self::validate_governance_identifier(skill_id, "skill_id", origin)?;
        let path = self.global_skill_path(&skill_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "skill_not_found",
                format!("Global skill '{skill_id}' not found"),
                origin,
            ));
        }
        fs::remove_file(&path).map_err(|e| {
            HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
        })?;

        let location = Self::governance_global_location("skill", &skill_id, path.clone());
        self.append_governance_delete_for_location(&location, CorrelationIds::none(), origin)?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: None,
            scope: "global".to_string(),
            artifact_kind: "skill".to_string(),
            artifact_key: skill_id,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// global_system_prompt_create (13401-13453)
    /// Creates a global system prompt.
    pub fn global_system_prompt_create(
        &self,
        prompt_id: &str,
        content: &str,
    ) -> Result<GlobalSystemPromptSummary> {
        let origin = "registry:global_system_prompt_create";
        self.ensure_global_governance_layout(origin)?;
        let prompt_id = Self::validate_governance_identifier(prompt_id, "prompt_id", origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_system_prompt_content",
                "System prompt content cannot be empty",
                origin,
            ));
        }

        let path = self.global_system_prompt_path(&prompt_id);
        if path.exists() {
            return Err(HivemindError::user(
                "system_prompt_exists",
                format!("Global system prompt '{prompt_id}' already exists"),
                origin,
            )
            .with_hint(
                "Use 'hivemind global system-prompt update <prompt-id>' to mutate this prompt",
            ));
        }

        let now = Utc::now();
        let artifact = GlobalSystemPromptArtifact {
            prompt_id: prompt_id.clone(),
            content: content.to_string(),
            updated_at: now,
        };
        Self::write_governance_json(&path, &artifact, origin)?;
        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = Self::governance_global_location("system_prompt", &prompt_id, path.clone());
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GlobalSystemPromptSummary {
            prompt_id,
            updated_at: now,
            path: path.to_string_lossy().to_string(),
        })
    }

// global_system_prompt_list (13455-13482)
    /// Lists global system prompts.
    pub fn global_system_prompt_list(&self) -> Result<Vec<GlobalSystemPromptSummary>> {
        let origin = "registry:global_system_prompt_list";
        self.ensure_global_governance_layout(origin)?;
        let mut out = Vec::new();
        for path in Self::governance_json_paths(
            &self.governance_global_root().join("system_prompts"),
            origin,
        )? {
            let key = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let artifact = Self::read_governance_json::<GlobalSystemPromptArtifact>(
                &path,
                "global_system_prompt",
                key,
                origin,
            )?;
            out.push(GlobalSystemPromptSummary {
                prompt_id: artifact.prompt_id,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            });
        }
        out.sort_by(|a, b| a.prompt_id.cmp(&b.prompt_id));
        Ok(out)
    }

// global_system_prompt_inspect (13484-13501)
    /// Inspects a global system prompt.
    pub fn global_system_prompt_inspect(
        &self,
        prompt_id: &str,
    ) -> Result<GlobalSystemPromptInspectResult> {
        let origin = "registry:global_system_prompt_inspect";
        let prompt_id = Self::validate_governance_identifier(prompt_id, "prompt_id", origin)?;
        let path = self.global_system_prompt_path(&prompt_id);
        let artifact = self.read_global_system_prompt_artifact(&prompt_id, origin)?;
        Ok(GlobalSystemPromptInspectResult {
            summary: GlobalSystemPromptSummary {
                prompt_id,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            },
            content: artifact.content,
        })
    }

// global_system_prompt_update (13503-13543)
    /// Updates a global system prompt.
    pub fn global_system_prompt_update(
        &self,
        prompt_id: &str,
        content: &str,
    ) -> Result<GlobalSystemPromptSummary> {
        let origin = "registry:global_system_prompt_update";
        let prompt_id = Self::validate_governance_identifier(prompt_id, "prompt_id", origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_system_prompt_content",
                "System prompt content cannot be empty",
                origin,
            ));
        }

        let path = self.global_system_prompt_path(&prompt_id);
        let mut artifact = self.read_global_system_prompt_artifact(&prompt_id, origin)?;
        if artifact.content != content {
            artifact.content = content.to_string();
            artifact.updated_at = Utc::now();
            Self::write_governance_json(&path, &artifact, origin)?;
            let state = self.state()?;
            let mut pending = HashMap::new();
            let location =
                Self::governance_global_location("system_prompt", &prompt_id, path.clone());
            let _ = self.append_governance_upsert_for_location(
                &state,
                &mut pending,
                &location,
                CorrelationIds::none(),
                origin,
            )?;
        }

        Ok(GlobalSystemPromptSummary {
            prompt_id,
            updated_at: artifact.updated_at,
            path: path.to_string_lossy().to_string(),
        })
    }

// global_system_prompt_delete (13545-13575)
    /// Deletes a global system prompt.
    pub fn global_system_prompt_delete(
        &self,
        prompt_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:global_system_prompt_delete";
        let prompt_id = Self::validate_governance_identifier(prompt_id, "prompt_id", origin)?;
        let path = self.global_system_prompt_path(&prompt_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "system_prompt_not_found",
                format!("Global system prompt '{prompt_id}' not found"),
                origin,
            ));
        }
        fs::remove_file(&path).map_err(|e| {
            HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
        })?;
        let location = Self::governance_global_location("system_prompt", &prompt_id, path.clone());
        self.append_governance_delete_for_location(&location, CorrelationIds::none(), origin)?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: None,
            scope: "global".to_string(),
            artifact_kind: "system_prompt".to_string(),
            artifact_key: prompt_id,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// global_template_create (13577-13657)
    /// Creates a global template artifact with strict reference validation.
    pub fn global_template_create(
        &self,
        template_id: &str,
        system_prompt_id: &str,
        skill_ids: &[String],
        document_ids: &[String],
        description: Option<&str>,
    ) -> Result<GlobalTemplateSummary> {
        let origin = "registry:global_template_create";
        self.ensure_global_governance_layout(origin)?;
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let system_prompt_id =
            Self::validate_governance_identifier(system_prompt_id, "system_prompt_id", origin)?;
        let normalized_skill_ids = Self::normalized_string_list(skill_ids)
            .into_iter()
            .map(|id| Self::validate_governance_identifier(&id, "skill_id", origin))
            .collect::<Result<Vec<_>>>()?;
        let normalized_document_ids = Self::normalized_string_list(document_ids)
            .into_iter()
            .map(|id| Self::validate_governance_identifier(&id, "document_id", origin))
            .collect::<Result<Vec<_>>>()?;

        let _ = self.read_global_system_prompt_artifact(&system_prompt_id, origin).map_err(|err| {
            err.with_hint(
                "Create the referenced system prompt first via 'hivemind global system-prompt create'",
            )
        })?;
        for skill_id in &normalized_skill_ids {
            let _ = self
                .read_global_skill_artifact(skill_id, origin)
                .map_err(|err| {
                    err.with_hint(
                        "Create the referenced skill first via 'hivemind global skill create'",
                    )
                })?;
        }

        let path = self.global_template_path(&template_id);
        if path.exists() {
            return Err(HivemindError::user(
                "template_exists",
                format!("Global template '{template_id}' already exists"),
                origin,
            )
            .with_hint(
                "Use 'hivemind global template update <template-id>' to mutate this template",
            ));
        }

        let now = Utc::now();
        let artifact = GlobalTemplateArtifact {
            template_id: template_id.clone(),
            system_prompt_id: system_prompt_id.clone(),
            skill_ids: normalized_skill_ids.clone(),
            document_ids: normalized_document_ids,
            description: description.map(std::string::ToString::to_string),
            updated_at: now,
        };
        Self::write_governance_json(&path, &artifact, origin)?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = Self::governance_global_location("template", &template_id, path.clone());
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GlobalTemplateSummary {
            template_id,
            system_prompt_id,
            skill_ids: normalized_skill_ids,
            document_ids: artifact.document_ids,
            updated_at: now,
            path: path.to_string_lossy().to_string(),
        })
    }

// global_template_list (13659-13688)
    /// Lists global templates.
    pub fn global_template_list(&self) -> Result<Vec<GlobalTemplateSummary>> {
        let origin = "registry:global_template_list";
        self.ensure_global_governance_layout(origin)?;
        let mut out = Vec::new();
        for path in
            Self::governance_json_paths(&self.governance_global_root().join("templates"), origin)?
        {
            let key = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let artifact = Self::read_governance_json::<GlobalTemplateArtifact>(
                &path,
                "global_template",
                key,
                origin,
            )?;
            out.push(GlobalTemplateSummary {
                template_id: artifact.template_id,
                system_prompt_id: artifact.system_prompt_id,
                skill_ids: artifact.skill_ids,
                document_ids: artifact.document_ids,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            });
        }
        out.sort_by(|a, b| a.template_id.cmp(&b.template_id));
        Ok(out)
    }

// global_template_inspect (13690-13711)
    /// Inspects a global template.
    pub fn global_template_inspect(
        &self,
        template_id: &str,
    ) -> Result<GlobalTemplateInspectResult> {
        let origin = "registry:global_template_inspect";
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let path = self.global_template_path(&template_id);
        let artifact = self.read_global_template_artifact(&template_id, origin)?;

        Ok(GlobalTemplateInspectResult {
            summary: GlobalTemplateSummary {
                template_id,
                system_prompt_id: artifact.system_prompt_id,
                skill_ids: artifact.skill_ids,
                document_ids: artifact.document_ids,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            },
            description: artifact.description,
        })
    }

// global_template_update (13713-13808)
    /// Updates a global template with strict reference validation.
    pub fn global_template_update(
        &self,
        template_id: &str,
        system_prompt_id: Option<&str>,
        skill_ids: Option<&[String]>,
        document_ids: Option<&[String]>,
        description: Option<&str>,
    ) -> Result<GlobalTemplateSummary> {
        let origin = "registry:global_template_update";
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let path = self.global_template_path(&template_id);
        let mut artifact = self.read_global_template_artifact(&template_id, origin)?;
        let mut changed = false;

        if let Some(system_prompt_id) = system_prompt_id {
            let system_prompt_id =
                Self::validate_governance_identifier(system_prompt_id, "system_prompt_id", origin)?;
            let _ = self.read_global_system_prompt_artifact(&system_prompt_id, origin).map_err(|err| {
                err.with_hint(
                    "Create the referenced system prompt first via 'hivemind global system-prompt create'",
                )
            })?;
            if artifact.system_prompt_id != system_prompt_id {
                artifact.system_prompt_id = system_prompt_id;
                changed = true;
            }
        }

        if let Some(skill_ids) = skill_ids {
            let normalized = Self::normalized_string_list(skill_ids)
                .into_iter()
                .map(|id| Self::validate_governance_identifier(&id, "skill_id", origin))
                .collect::<Result<Vec<_>>>()?;
            for skill_id in &normalized {
                let _ = self
                    .read_global_skill_artifact(skill_id, origin)
                    .map_err(|err| {
                        err.with_hint(
                            "Create the referenced skill first via 'hivemind global skill create'",
                        )
                    })?;
            }
            if artifact.skill_ids != normalized {
                artifact.skill_ids = normalized;
                changed = true;
            }
        }

        if let Some(document_ids) = document_ids {
            let normalized = Self::normalized_string_list(document_ids)
                .into_iter()
                .map(|id| Self::validate_governance_identifier(&id, "document_id", origin))
                .collect::<Result<Vec<_>>>()?;
            if artifact.document_ids != normalized {
                artifact.document_ids = normalized;
                changed = true;
            }
        }

        if let Some(description) = description {
            let next = if description.trim().is_empty() {
                None
            } else {
                Some(description.to_string())
            };
            if artifact.description != next {
                artifact.description = next;
                changed = true;
            }
        }

        if changed {
            artifact.updated_at = Utc::now();
            Self::write_governance_json(&path, &artifact, origin)?;
            let state = self.state()?;
            let mut pending = HashMap::new();
            let location = Self::governance_global_location("template", &template_id, path.clone());
            let _ = self.append_governance_upsert_for_location(
                &state,
                &mut pending,
                &location,
                CorrelationIds::none(),
                origin,
            )?;
        }

        Ok(GlobalTemplateSummary {
            template_id,
            system_prompt_id: artifact.system_prompt_id,
            skill_ids: artifact.skill_ids,
            document_ids: artifact.document_ids,
            updated_at: artifact.updated_at,
            path: path.to_string_lossy().to_string(),
        })
    }

// global_template_delete (13810-13840)
    /// Deletes a global template.
    pub fn global_template_delete(
        &self,
        template_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:global_template_delete";
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let path = self.global_template_path(&template_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "template_not_found",
                format!("Global template '{template_id}' not found"),
                origin,
            ));
        }
        fs::remove_file(&path).map_err(|e| {
            HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
        })?;
        let location = Self::governance_global_location("template", &template_id, path.clone());
        self.append_governance_delete_for_location(&location, CorrelationIds::none(), origin)?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: None,
            scope: "global".to_string(),
            artifact_kind: "template".to_string(),
            artifact_key: template_id,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// global_template_instantiate (13842-13906)
    /// Resolves a global template for a specific project and emits an instantiation event.
    pub fn global_template_instantiate(
        &self,
        id_or_name: &str,
        template_id: &str,
    ) -> Result<TemplateInstantiationResult> {
        let origin = "registry:global_template_instantiate";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let artifact = self.read_global_template_artifact(&template_id, origin)?;
        let _ = self
            .read_global_system_prompt_artifact(&artifact.system_prompt_id, origin)
            .map_err(|err| {
                err.with_hint(
                    "Fix template.system_prompt_id or recreate the missing system prompt before instantiation",
                )
            })?;

        for skill_id in &artifact.skill_ids {
            let _ = self
                .read_global_skill_artifact(skill_id, origin)
                .map_err(|err| {
                    err.with_hint("Fix template.skill_ids or recreate the missing global skill")
                })?;
        }
        for document_id in &artifact.document_ids {
            let _ = self
                .read_project_document_artifact(project.id, document_id, origin)
                .map_err(|err| {
                    err.with_hint(
                        "Fix template.document_ids or create the missing project document before instantiation",
                    )
                })?;
        }

        let now = Utc::now();
        self.append_event(
            Event::new(
                EventPayload::TemplateInstantiated {
                    project_id: project.id,
                    template_id: template_id.clone(),
                    system_prompt_id: artifact.system_prompt_id.clone(),
                    skill_ids: artifact.skill_ids.clone(),
                    document_ids: artifact.document_ids.clone(),
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(TemplateInstantiationResult {
            project_id: project.id,
            template_id,
            system_prompt_id: artifact.system_prompt_id,
            skill_ids: artifact.skill_ids,
            document_ids: artifact.document_ids,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            instantiated_at: now,
        })
    }

// global_notepad_create (13908-13955)
    /// Creates global notepad content.
    pub fn global_notepad_create(&self, content: &str) -> Result<GovernanceNotepadResult> {
        let origin = "registry:global_notepad_create";
        self.ensure_global_governance_layout(origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_notepad_content",
                "Notepad content cannot be empty",
                origin,
            ));
        }

        let location = self.governance_notepad_location(None);
        let existing = fs::read_to_string(&location.path).unwrap_or_default();
        if !existing.trim().is_empty() {
            return Err(HivemindError::user(
                "notepad_exists",
                "Global notepad already has content",
                origin,
            )
            .with_hint("Use 'hivemind global notepad update' to replace content"));
        }

        fs::write(&location.path, content).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })?;
        let state = self.state()?;
        let mut pending = HashMap::new();
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GovernanceNotepadResult {
            scope: "global".to_string(),
            project_id: None,
            path: location.path.to_string_lossy().to_string(),
            exists: true,
            content: Some(content.to_string()),
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// global_notepad_show (13957-13985)
    /// Shows global notepad content.
    pub fn global_notepad_show(&self) -> Result<GovernanceNotepadResult> {
        let origin = "registry:global_notepad_show";
        self.ensure_global_governance_layout(origin)?;
        let location = self.governance_notepad_location(None);
        let raw_content = if location.path.is_file() {
            Some(fs::read_to_string(&location.path).map_err(|e| {
                HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
            })?)
        } else {
            None
        };
        let (exists, content) = match raw_content {
            Some(content) if content.trim().is_empty() => (false, None),
            Some(content) => (true, Some(content)),
            None => (false, None),
        };
        Ok(GovernanceNotepadResult {
            scope: "global".to_string(),
            project_id: None,
            path: location.path.to_string_lossy().to_string(),
            exists,
            content,
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// global_notepad_update (13987-14023)
    /// Updates global notepad content.
    pub fn global_notepad_update(&self, content: &str) -> Result<GovernanceNotepadResult> {
        let origin = "registry:global_notepad_update";
        self.ensure_global_governance_layout(origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_notepad_content",
                "Notepad content cannot be empty",
                origin,
            ));
        }
        let location = self.governance_notepad_location(None);
        fs::write(&location.path, content).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })?;
        let state = self.state()?;
        let mut pending = HashMap::new();
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GovernanceNotepadResult {
            scope: "global".to_string(),
            project_id: None,
            path: location.path.to_string_lossy().to_string(),
            exists: true,
            content: Some(content.to_string()),
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

// global_notepad_delete (14025-14046)
    /// Deletes global notepad content.
    pub fn global_notepad_delete(&self) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:global_notepad_delete";
        self.ensure_global_governance_layout(origin)?;
        let location = self.governance_notepad_location(None);
        if location.path.exists() {
            fs::remove_file(&location.path).map_err(|e| {
                HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
            })?;
        }
        self.append_governance_delete_for_location(&location, CorrelationIds::none(), origin)?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: None,
            scope: "global".to_string(),
            artifact_kind: "notepad".to_string(),
            artifact_key: "notepad.md".to_string(),
            path: location.path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

