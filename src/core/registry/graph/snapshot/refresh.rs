use super::*;

impl Registry {
    pub(crate) fn trigger_graph_snapshot_refresh(
        &self,
        project_id: Uuid,
        trigger: &str,
        _origin: &'static str,
    ) {
        let project_key = project_id.to_string();
        if let Err(err) = self.graph_snapshot_refresh(&project_key, trigger) {
            self.record_error_event(&err, CorrelationIds::for_project(project_id));
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn graph_snapshot_refresh(
        &self,
        id_or_name: &str,
        trigger: &str,
    ) -> Result<GraphSnapshotRefreshResult> {
        let origin = "registry:graph_snapshot_refresh";
        let trigger = trigger.trim();
        let trigger = if trigger.is_empty() {
            "manual_refresh"
        } else {
            trigger
        };

        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if project.repositories.is_empty() {
            let err = HivemindError::user(
                    "project_has_no_repo",
                    "Project has no attached repository to extract a codegraph from",
                    origin,
                )
                .with_hint("Attach a repository first via 'hivemind project attach-repo <project> <repo-path>'");
            self.append_graph_snapshot_failed_event(project.id, trigger, &err, origin);
            return Err(err);
        }

        self.ensure_governance_layout(project.id, origin)
            .inspect_err(|err| {
                self.append_graph_snapshot_failed_event(project.id, trigger, err, origin);
            })?;
        let location = self.governance_graph_snapshot_location(project.id);
        let previous = self
            .read_graph_snapshot_artifact(project.id, origin)
            .inspect_err(|err| {
                self.append_graph_snapshot_failed_event(project.id, trigger, err, origin);
            })?;
        let previous_fingerprint = previous
            .as_ref()
            .map(|item| item.canonical_fingerprint.clone());

        self.append_event(
            Event::new(
                EventPayload::GraphSnapshotStarted {
                    project_id: project.id,
                    trigger: trigger.to_string(),
                    repository_count: project.repositories.len(),
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        let mut repositories = project.repositories.clone();
        repositories.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));

        let mut snapshots: Vec<GraphSnapshotRepositoryArtifact> = Vec::new();
        let mut graph_stats: Vec<ucp_api::CodeGraphStats> = Vec::new();
        let mut head_commits: Vec<GraphSnapshotRepositoryCommit> = Vec::new();
        let mut static_sections = Vec::new();

        for repo in &repositories {
            let repo_path = PathBuf::from(&repo.path);
            let commit_hash = match Self::resolve_repo_head_commit(&repo_path, origin) {
                Ok(head) => head,
                Err(err) => {
                    self.append_graph_snapshot_failed_event(project.id, trigger, &err, origin);
                    return Err(err.with_context("repo", repo.name.clone()));
                }
            };

            let build_input = CodeGraphBuildInput {
                repository_path: repo_path.clone(),
                commit_hash: commit_hash.clone(),
                config: ucp_api::CodeGraphExtractorConfig::default(),
            };
            let built = match build_code_graph(&build_input) {
                Ok(result) => result,
                Err(err) => {
                    let wrapped = HivemindError::system(
                        "ucp_codegraph_build_failed",
                        format!(
                            "UCP codegraph extraction failed for repository '{}': {err}",
                            repo.name
                        ),
                        origin,
                    )
                    .with_hint(
                        "Fix extraction diagnostics and rerun `hivemind graph snapshot refresh`",
                    );
                    self.append_graph_snapshot_failed_event(project.id, trigger, &wrapped, origin);
                    return Err(wrapped);
                }
            };

            if built.profile_version != CODEGRAPH_PROFILE_MARKER {
                let err = HivemindError::system(
                        "graph_snapshot_profile_version_invalid",
                        format!(
                            "Unexpected UCP profile version '{}' (expected '{CODEGRAPH_PROFILE_MARKER}')",
                            built.profile_version
                        ),
                        origin,
                    )
                    .with_hint("Update UCP integration to a CodeGraphProfile v1-compatible engine");
                self.append_graph_snapshot_failed_event(project.id, trigger, &err, origin);
                return Err(err);
            }

            if built.canonical_fingerprint.trim().is_empty() {
                let err = HivemindError::system(
                    "graph_snapshot_fingerprint_missing",
                    "UCP build result did not include canonical_fingerprint",
                    origin,
                )
                .with_hint(
                    "Re-run with a UCP codegraph extractor that emits canonical fingerprints",
                );
                self.append_graph_snapshot_failed_event(project.id, trigger, &err, origin);
                return Err(err);
            }

            let validation = validate_code_graph_profile(&built.document);
            if !validation.valid {
                let err = HivemindError::system(
                    "graph_snapshot_profile_invalid",
                    format!(
                        "UCP output failed CodeGraphProfile validation with {} issue(s)",
                        validation.diagnostics.len()
                    ),
                    origin,
                )
                .with_context(
                    "diagnostics",
                    serde_json::to_string(&validation.diagnostics)
                        .unwrap_or_else(|_| "[]".to_string()),
                )
                .with_hint(
                    "Fix UCP profile validation errors and rerun `hivemind graph snapshot refresh`",
                );
                self.append_graph_snapshot_failed_event(project.id, trigger, &err, origin);
                return Err(err);
            }

            let recomputed_fingerprint = canonical_fingerprint(&built.document).map_err(|e| {
                let err = HivemindError::system(
                    "graph_snapshot_fingerprint_recompute_failed",
                    e.to_string(),
                    origin,
                );
                self.append_graph_snapshot_failed_event(project.id, trigger, &err, origin);
                err
            })?;
            if recomputed_fingerprint != built.canonical_fingerprint {
                let err = HivemindError::system(
                    "graph_snapshot_fingerprint_mismatch",
                    "UCP canonical_fingerprint did not match recomputed canonical fingerprint",
                    origin,
                )
                .with_hint("Refresh with a deterministic UCP extractor build");
                self.append_graph_snapshot_failed_event(project.id, trigger, &err, origin);
                return Err(err);
            }

            let portable = PortableDocument::from_document(&built.document);
            if let Err(err) = Self::ensure_codegraph_scope_contract(&portable, origin) {
                self.append_graph_snapshot_failed_event(project.id, trigger, &err, origin);
                return Err(err);
            }

            let projection = codegraph_prompt_projection(&built.document);
            static_sections.push(format!("## {}\n{}", repo.name, projection));

            head_commits.push(GraphSnapshotRepositoryCommit {
                repo_name: repo.name.clone(),
                repo_path: repo.path.clone(),
                commit_hash: commit_hash.clone(),
            });
            graph_stats.push(built.stats.clone());
            snapshots.push(GraphSnapshotRepositoryArtifact {
                repo_name: repo.name.clone(),
                repo_path: repo.path.clone(),
                commit_hash,
                profile_version: built.profile_version,
                canonical_fingerprint: built.canonical_fingerprint,
                stats: built.stats,
                document: portable,
                structure_blocks_projection: projection,
            });
        }

        snapshots.sort_by(|a, b| {
            a.repo_name
                .cmp(&b.repo_name)
                .then(a.repo_path.cmp(&b.repo_path))
        });
        head_commits.sort_by(|a, b| {
            a.repo_name
                .cmp(&b.repo_name)
                .then(a.repo_path.cmp(&b.repo_path))
        });

        let summary = Self::aggregate_codegraph_stats(&graph_stats);
        let canonical_fingerprint = Self::compute_snapshot_fingerprint(&snapshots);
        let artifact = GraphSnapshotArtifact {
            schema_version: GRAPH_SNAPSHOT_SCHEMA_VERSION.to_string(),
            snapshot_version: GRAPH_SNAPSHOT_VERSION,
            provenance: GraphSnapshotProvenance {
                project_id: project.id,
                head_commits,
                generated_at: Utc::now(),
            },
            ucp_engine_version: CODEGRAPH_EXTRACTOR_VERSION.to_string(),
            profile_version: CODEGRAPH_PROFILE_MARKER.to_string(),
            canonical_fingerprint: canonical_fingerprint.clone(),
            summary: summary.clone(),
            repositories: snapshots,
            static_projection: static_sections.join("\n\n"),
        };

        Self::write_governance_json(&location.path, &artifact, origin).inspect_err(|err| {
            self.append_graph_snapshot_failed_event(project.id, trigger, err, origin);
        })?;
        let constitution_path = self.constitution_path(project.id);
        let constitution = constitution_path
            .is_file()
            .then_some(constitution_path.as_path());
        self.sync_graph_snapshot_into_runtime_registry(project.id, &artifact, constitution, origin)
            .inspect_err(|err| {
                self.append_graph_snapshot_failed_event(project.id, trigger, err, origin);
            })?;

        let state = self.state().inspect_err(|err| {
            self.append_graph_snapshot_failed_event(project.id, trigger, err, origin);
        })?;
        let mut pending = HashMap::new();
        let revision = self
            .append_governance_upsert_for_location(
                &state,
                &mut pending,
                &location,
                CorrelationIds::for_project(project.id),
                origin,
            )
            .inspect_err(|err| {
                self.append_graph_snapshot_failed_event(project.id, trigger, err, origin);
            })?;

        let diff_detected = previous_fingerprint.as_deref() != Some(canonical_fingerprint.as_str());
        if diff_detected {
            if let Err(err) =
                self.mark_execution_graph_snapshot_registries_freshness(project.id, "stale", origin)
            {
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
            }
            self.append_event(
                Event::new(
                    EventPayload::GraphSnapshotDiffDetected {
                        project_id: project.id,
                        trigger: trigger.to_string(),
                        previous_fingerprint,
                        canonical_fingerprint: canonical_fingerprint.clone(),
                    },
                    CorrelationIds::for_project(project.id),
                ),
                origin,
            )
            .inspect_err(|err| {
                self.append_graph_snapshot_failed_event(project.id, trigger, err, origin);
            })?;
        }

        self.append_event(
            Event::new(
                EventPayload::GraphSnapshotCompleted {
                    project_id: project.id,
                    trigger: trigger.to_string(),
                    path: location.path.to_string_lossy().to_string(),
                    revision,
                    repository_count: project.repositories.len(),
                    ucp_engine_version: CODEGRAPH_EXTRACTOR_VERSION.to_string(),
                    profile_version: CODEGRAPH_PROFILE_MARKER.to_string(),
                    canonical_fingerprint: canonical_fingerprint.clone(),
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )
        .inspect_err(|err| {
            self.append_graph_snapshot_failed_event(project.id, trigger, err, origin);
        })?;

        Ok(GraphSnapshotRefreshResult {
            project_id: project.id,
            path: location.path.to_string_lossy().to_string(),
            trigger: trigger.to_string(),
            revision,
            repository_count: project.repositories.len(),
            ucp_engine_version: CODEGRAPH_EXTRACTOR_VERSION.to_string(),
            profile_version: CODEGRAPH_PROFILE_MARKER.to_string(),
            canonical_fingerprint,
            summary,
            diff_detected,
        })
    }
}
