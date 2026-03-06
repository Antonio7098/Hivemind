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

    pub(crate) fn normalize_graph_path(path: &str) -> String {
        path.trim()
            .replace('\\', "/")
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string()
    }

    pub(crate) fn path_in_partition(path: &str, partition_path: &str) -> bool {
        let normalized_path = Self::normalize_graph_path(path);
        let normalized_partition = Self::normalize_graph_path(partition_path);
        if normalized_partition.is_empty() {
            return false;
        }
        normalized_path == normalized_partition
            || normalized_path.starts_with(&format!("{normalized_partition}/"))
    }

    pub(crate) fn graph_facts_from_snapshot(
        snapshot: &GraphSnapshotArtifact,
        origin: &'static str,
    ) -> Result<GraphConstitutionFacts> {
        let mut facts = GraphConstitutionFacts::default();

        for repo in &snapshot.repositories {
            let document = repo.document.to_document().map_err(|e| {
                HivemindError::system(
                    "graph_snapshot_schema_invalid",
                    format!("Failed to load graph snapshot document: {e}"),
                    origin,
                )
            })?;

            let mut file_key_by_block_id: HashMap<String, String> = HashMap::new();

            for (block_id, block) in &document.blocks {
                let Some(class_name) = block
                    .metadata
                    .custom
                    .get("node_class")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };

                if class_name == "file" {
                    let logical_key = block
                        .metadata
                        .custom
                        .get("logical_key")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            HivemindError::system(
                                "graph_snapshot_schema_invalid",
                                "File node missing logical_key metadata",
                                origin,
                            )
                        })?;
                    let path = block
                        .metadata
                        .custom
                        .get("path")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            HivemindError::system(
                                "graph_snapshot_schema_invalid",
                                "File node missing path metadata",
                                origin,
                            )
                        })?;
                    let normalized_path = Self::normalize_graph_path(path);
                    let graph_key = format!("{}::{logical_key}", repo.repo_name);
                    file_key_by_block_id.insert(block_id.to_string(), graph_key.clone());
                    facts.files.insert(
                        graph_key,
                        GraphFileFact {
                            display_path: format!("{}/{}", repo.repo_name, normalized_path),
                            path: normalized_path.clone(),
                            symbol_key: format!("{}::{normalized_path}", repo.repo_name),
                            references: Vec::new(),
                        },
                    );
                } else if class_name == "symbol" {
                    if let Some(path) = block
                        .metadata
                        .custom
                        .get("path")
                        .and_then(serde_json::Value::as_str)
                    {
                        let normalized_path = Self::normalize_graph_path(path);
                        facts
                            .symbol_file_keys
                            .insert(format!("{}::{normalized_path}", repo.repo_name));
                    }
                }
            }

            for (block_id, block) in &document.blocks {
                let Some(source_key) = file_key_by_block_id.get(&block_id.to_string()).cloned()
                else {
                    continue;
                };
                let Some(source) = facts.files.get_mut(&source_key) else {
                    continue;
                };
                for edge in &block.edges {
                    if edge.edge_type.as_str() != "references" {
                        continue;
                    }
                    let target_id = edge.target.to_string();
                    if let Some(target_key) = file_key_by_block_id.get(&target_id) {
                        source.references.push(target_key.clone());
                    }
                }
                source.references.sort();
                source.references.dedup();
            }
        }

        Ok(facts)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn evaluate_constitution_rules(
        artifact: &ConstitutionArtifact,
        snapshot: &GraphSnapshotArtifact,
        origin: &'static str,
    ) -> Result<Vec<ConstitutionRuleViolation>> {
        let facts = Self::graph_facts_from_snapshot(snapshot, origin)?;
        let mut partition_files: HashMap<String, HashSet<String>> = HashMap::new();
        for partition in &artifact.partitions {
            let mut members = HashSet::new();
            for (graph_key, file) in &facts.files {
                if Self::path_in_partition(&file.path, &partition.path) {
                    members.insert(graph_key.clone());
                }
            }
            partition_files.insert(partition.id.clone(), members);
        }

        let mut violations = Vec::new();
        for rule in &artifact.rules {
            match rule {
                ConstitutionRule::ForbiddenDependency {
                    id,
                    from,
                    to,
                    severity,
                } => {
                    let from_files = partition_files.get(from).cloned().unwrap_or_default();
                    let to_files = partition_files.get(to).cloned().unwrap_or_default();
                    let mut evidence = Vec::new();
                    for source_key in &from_files {
                        let Some(source) = facts.files.get(source_key) else {
                            continue;
                        };
                        for target_key in &source.references {
                            if !to_files.contains(target_key) {
                                continue;
                            }
                            let target = facts
                                .files
                                .get(target_key)
                                .map_or(target_key.as_str(), |item| item.display_path.as_str());
                            evidence.push(format!("{} -> {target}", source.display_path));
                        }
                    }

                    if !evidence.is_empty() {
                        violations.push(ConstitutionRuleViolation {
                            rule_id: id.clone(),
                            rule_type: "forbidden_dependency".to_string(),
                            severity: severity.clone(),
                            message: format!(
                                "Detected {} forbidden dependency edge(s) from partition '{from}' to '{to}'",
                                evidence.len()
                            ),
                            evidence: evidence.into_iter().take(20).collect(),
                            remediation_hint: Some(format!(
                                "Remove or invert dependencies from '{from}' to '{to}'"
                            )),
                            blocked: matches!(severity, ConstitutionSeverity::Hard),
                        });
                    }
                }
                ConstitutionRule::AllowedDependency {
                    id,
                    from,
                    to,
                    severity,
                } => {
                    let from_files = partition_files.get(from).cloned().unwrap_or_default();
                    let to_files = partition_files.get(to).cloned().unwrap_or_default();
                    let mut evidence = Vec::new();

                    for source_key in &from_files {
                        let Some(source) = facts.files.get(source_key) else {
                            continue;
                        };
                        for target_key in &source.references {
                            if from_files.contains(target_key) || to_files.contains(target_key) {
                                continue;
                            }
                            let violating_partitions: Vec<&str> = partition_files
                                .iter()
                                .filter_map(|(partition_id, members)| {
                                    members
                                        .contains(target_key)
                                        .then_some(partition_id.as_str())
                                })
                                .collect();
                            if violating_partitions.is_empty() {
                                continue;
                            }
                            let target = facts
                                .files
                                .get(target_key)
                                .map_or(target_key.as_str(), |item| item.display_path.as_str());
                            evidence.push(format!(
                                "{} -> {target} (target partitions: {})",
                                source.display_path,
                                violating_partitions.join(", ")
                            ));
                        }
                    }

                    if !evidence.is_empty() {
                        violations.push(ConstitutionRuleViolation {
                            rule_id: id.clone(),
                            rule_type: "allowed_dependency".to_string(),
                            severity: severity.clone(),
                            message: format!(
                                "Detected {} dependency edge(s) from partition '{from}' outside allowed target partition '{to}'",
                                evidence.len()
                            ),
                            evidence: evidence.into_iter().take(20).collect(),
                            remediation_hint: Some(format!(
                                "Restrict '{from}' dependencies to partition '{to}' (and intra-partition references)"
                            )),
                            blocked: matches!(severity, ConstitutionSeverity::Hard),
                        });
                    }
                }
                ConstitutionRule::CoverageRequirement {
                    id,
                    target,
                    threshold,
                    severity,
                } => {
                    let target_files = partition_files.get(target).cloned().unwrap_or_default();
                    let total = target_files.len();
                    let covered = target_files
                        .iter()
                        .filter(|file_key| {
                            facts.files.get(*file_key).is_some_and(|file| {
                                facts.symbol_file_keys.contains(&file.symbol_key)
                            })
                        })
                        .count();
                    let coverage_bps: u128 = if total == 0 {
                        0
                    } else {
                        (covered as u128 * 10_000) / total as u128
                    };
                    let coverage_whole = coverage_bps / 100;
                    let coverage_frac = coverage_bps % 100;
                    let meets_threshold =
                        (covered as u128 * 100) >= (u128::from(*threshold) * total as u128);
                    if !meets_threshold {
                        let missing_files: Vec<String> = target_files
                            .iter()
                            .filter_map(|file_key| {
                                facts.files.get(file_key).and_then(|file| {
                                    (!facts.symbol_file_keys.contains(&file.symbol_key))
                                        .then(|| file.display_path.clone())
                                })
                            })
                            .take(10)
                            .collect();
                        let mut evidence = vec![format!(
                            "coverage={coverage_whole}.{coverage_frac:02}% threshold={threshold}% covered_files={covered}/{total}"
                        )];
                        if !missing_files.is_empty() {
                            evidence.push(format!(
                                "files without symbol coverage: {}",
                                missing_files.join(", ")
                            ));
                        }
                        violations.push(ConstitutionRuleViolation {
                            rule_id: id.clone(),
                            rule_type: "coverage_requirement".to_string(),
                            severity: severity.clone(),
                            message: format!(
                                "Partition '{target}' coverage is below threshold ({coverage_whole}.{coverage_frac:02}% < {threshold}%)"
                            ),
                            evidence,
                            remediation_hint: Some(format!(
                                "Increase codegraph symbol coverage for files under '{target}' or lower the threshold"
                            )),
                            blocked: matches!(severity, ConstitutionSeverity::Hard),
                        });
                    }
                }
            }
        }

        Ok(violations)
    }

    pub(crate) fn append_constitution_violation_events(
        &self,
        project_id: Uuid,
        gate: &str,
        correlation: &CorrelationIds,
        violations: &[ConstitutionRuleViolation],
        origin: &'static str,
    ) -> Result<()> {
        for violation in violations {
            self.append_event(
                Event::new(
                    EventPayload::ConstitutionViolationDetected {
                        project_id,
                        flow_id: correlation.flow_id,
                        task_id: correlation.task_id,
                        attempt_id: correlation.attempt_id,
                        gate: gate.to_string(),
                        rule_id: violation.rule_id.clone(),
                        rule_type: violation.rule_type.clone(),
                        severity: violation.severity.as_str().to_string(),
                        message: violation.message.clone(),
                        evidence: violation.evidence.clone(),
                        remediation_hint: violation.remediation_hint.clone(),
                        blocked: violation.blocked,
                    },
                    correlation.clone(),
                ),
                origin,
            )?;
        }
        Ok(())
    }

    pub(crate) fn run_constitution_check(
        &self,
        project: &Project,
        gate: &str,
        correlation: &CorrelationIds,
        require_initialized: bool,
        origin: &'static str,
    ) -> Result<ProjectConstitutionCheckResult> {
        let state = self.state()?;
        let initialized = state
            .projects
            .get(&project.id)
            .and_then(|item| item.constitution_digest.as_ref())
            .is_some();
        if !initialized {
            if require_initialized {
                return Err(HivemindError::user(
                    "constitution_not_initialized",
                    "Constitution is not initialized for this project",
                    origin,
                )
                .with_hint("Run 'hivemind constitution init <project> --confirm' first"));
            }
            return Ok(ProjectConstitutionCheckResult {
                project_id: project.id,
                gate: gate.to_string(),
                path: self
                    .constitution_path(project.id)
                    .to_string_lossy()
                    .to_string(),
                digest: String::new(),
                schema_version: CONSTITUTION_SCHEMA_VERSION.to_string(),
                constitution_version: CONSTITUTION_VERSION,
                flow_id: correlation.flow_id,
                task_id: correlation.task_id,
                attempt_id: correlation.attempt_id,
                skipped: true,
                skip_reason: Some(
                    "Project constitution is not initialized; enforcement gate skipped".to_string(),
                ),
                violations: Vec::new(),
                hard_violations: 0,
                advisory_violations: 0,
                informational_violations: 0,
                blocked: false,
                checked_at: Utc::now(),
            });
        }

        self.ensure_graph_snapshot_current_for_constitution(project, origin)?;
        let (artifact, path) = self.read_constitution_artifact(project.id, origin)?;
        let raw = fs::read(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        let digest = Self::constitution_digest(raw.as_slice());

        let snapshot = self
            .read_graph_snapshot_artifact(project.id, origin)?
            .ok_or_else(|| {
                HivemindError::user(
                    "graph_snapshot_missing",
                    "Graph snapshot is missing for constitution enforcement",
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>")
            })?;
        let violations = Self::evaluate_constitution_rules(&artifact, &snapshot, origin)?;
        if !violations.is_empty() {
            self.append_constitution_violation_events(
                project.id,
                gate,
                correlation,
                &violations,
                origin,
            )?;
        }

        let hard_violations = violations
            .iter()
            .filter(|item| matches!(item.severity, ConstitutionSeverity::Hard))
            .count();
        let advisory_violations = violations
            .iter()
            .filter(|item| matches!(item.severity, ConstitutionSeverity::Advisory))
            .count();
        let informational_violations = violations
            .iter()
            .filter(|item| matches!(item.severity, ConstitutionSeverity::Informational))
            .count();

        Ok(ProjectConstitutionCheckResult {
            project_id: project.id,
            gate: gate.to_string(),
            path: path.to_string_lossy().to_string(),
            digest,
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            flow_id: correlation.flow_id,
            task_id: correlation.task_id,
            attempt_id: correlation.attempt_id,
            skipped: false,
            skip_reason: None,
            violations,
            hard_violations,
            advisory_violations,
            informational_violations,
            blocked: hard_violations > 0,
            checked_at: Utc::now(),
        })
    }

    pub(crate) fn enforce_constitution_gate(
        &self,
        project_id: Uuid,
        gate: &'static str,
        correlation: CorrelationIds,
        origin: &'static str,
    ) -> Result<Option<ProjectConstitutionCheckResult>> {
        let project = self
            .get_project(&project_id.to_string())
            .inspect_err(|err| self.record_error_event(err, correlation.clone()))?;
        let result = self.run_constitution_check(&project, gate, &correlation, false, origin)?;
        if result.skipped {
            return Ok(None);
        }
        if result.blocked {
            let hard_rule_ids = result
                .violations
                .iter()
                .filter(|item| matches!(item.severity, ConstitutionSeverity::Hard))
                .map(|item| item.rule_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let err = HivemindError::policy(
                    "constitution_hard_violation",
                    format!("Constitution hard violations block {gate}: {hard_rule_ids}"),
                    origin,
                )
                .with_context("project_id", project_id.to_string())
                .with_context("gate", gate.to_string())
                .with_context(
                    "violations",
                    serde_json::to_string(&result.violations).unwrap_or_else(|_| "[]".to_string()),
                )
                .with_hint(format!(
                    "Run 'hivemind constitution check --project {project_id}' and remediate blocking rules"
                ));
            self.record_error_event(&err, correlation);
            return Err(err);
        }
        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_graph_path_trims_and_normalizes_separators() {
        assert_eq!(
            Registry::normalize_graph_path(" ./src\\core/file.rs "),
            "src/core/file.rs"
        );
    }

    #[test]
    fn path_in_partition_matches_directories_only() {
        assert!(Registry::path_in_partition("src/core/file.rs", "src/core"));
        assert!(Registry::path_in_partition("src/core", "src/core"));
        assert!(!Registry::path_in_partition(
            "src/core-utils/file.rs",
            "src/core"
        ));
        assert!(!Registry::path_in_partition("src/core/file.rs", ""));
    }
}
