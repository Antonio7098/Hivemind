use super::*;

impl Registry {
    pub(crate) fn graph_snapshot_path(&self, project_id: Uuid) -> PathBuf {
        self.governance_project_root(project_id)
            .join("graph_snapshot.json")
    }

    pub(crate) fn read_graph_snapshot_artifact(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Option<GraphSnapshotArtifact>> {
        let path = self.graph_snapshot_path(project_id);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
        })?;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "{}" {
            return Ok(None);
        }
        let artifact = Self::read_governance_json::<GraphSnapshotArtifact>(
            &path,
            "graph_snapshot",
            "graph_snapshot.json",
            origin,
        )?;
        Ok(Some(artifact))
    }

    pub(crate) fn resolve_repo_head_commit(
        repo_path: &Path,
        origin: &'static str,
    ) -> Result<String> {
        let output = std::process::Command::new("git")
            .current_dir(repo_path)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| HivemindError::git("git_rev_parse_failed", e.to_string(), origin))?;
        if !output.status.success() {
            return Err(HivemindError::git(
                "git_rev_parse_failed",
                String::from_utf8_lossy(&output.stderr).to_string(),
                origin,
            ));
        }
        let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if head.is_empty() {
            return Err(HivemindError::git(
                "git_head_missing",
                "Failed to resolve repository HEAD commit".to_string(),
                origin,
            ));
        }
        Ok(head)
    }

    pub(crate) fn aggregate_codegraph_stats(
        stats: &[ucp_api::CodeGraphStats],
    ) -> GraphSnapshotSummary {
        let mut summary = GraphSnapshotSummary::default();
        for item in stats {
            summary.total_nodes += item.total_nodes;
            summary.repository_nodes += item.repository_nodes;
            summary.directory_nodes += item.directory_nodes;
            summary.file_nodes += item.file_nodes;
            summary.symbol_nodes += item.symbol_nodes;
            summary.total_edges += item.total_edges;
            summary.reference_edges += item.reference_edges;
            summary.export_edges += item.export_edges;
            for (lang, count) in &item.languages {
                *summary.languages.entry(lang.clone()).or_insert(0) += *count;
            }
        }
        summary
    }

    pub(crate) fn compute_snapshot_fingerprint(
        repositories: &[GraphSnapshotRepositoryArtifact],
    ) -> String {
        let mut entries: Vec<String> = repositories
            .iter()
            .map(|repo| {
                format!(
                    "{}|{}|{}|{}",
                    repo.repo_name, repo.repo_path, repo.commit_hash, repo.canonical_fingerprint
                )
            })
            .collect();
        entries.sort();
        let joined = entries.join("\n");
        Self::constitution_digest(joined.as_bytes())
    }

    pub(crate) fn ensure_codegraph_scope_contract(
        document: &PortableDocument,
        origin: &'static str,
    ) -> Result<()> {
        let allowed_classes = ["repository", "directory", "file", "symbol"];
        for (block_id, block) in &document.blocks {
            if block_id == &document.root {
                continue;
            }
            let Some(class) = block
                .metadata
                .custom
                .get("node_class")
                .and_then(serde_json::Value::as_str)
            else {
                return Err(HivemindError::system(
                    "graph_snapshot_missing_node_class",
                    "UCP codegraph node is missing required node_class metadata",
                    origin,
                ));
            };
            if !allowed_classes.contains(&class) {
                return Err(HivemindError::system(
                        "graph_snapshot_scope_unsupported",
                        format!("Unsupported codegraph node class '{class}'"),
                        origin,
                    )
                    .with_hint(
                        "Refresh with a UCP CodeGraphProfile v1 extractor that emits repository/directory/file/symbol nodes only",
                    ));
            }
            if block
                .metadata
                .custom
                .get("logical_key")
                .and_then(serde_json::Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(HivemindError::system(
                    "graph_snapshot_logical_key_missing",
                    "UCP codegraph block is missing required logical_key metadata",
                    origin,
                ));
            }

            for edge in &block.edges {
                let edge_type = edge.edge_type.as_str();
                if edge_type != "references" && edge_type != "custom:exports" {
                    return Err(HivemindError::system(
                            "graph_snapshot_scope_unsupported",
                            format!("Unsupported codegraph edge type '{edge_type}'"),
                            origin,
                        )
                        .with_hint(
                            "Refresh with a UCP CodeGraphProfile v1 extractor that only emits references/exports edges",
                        ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn append_graph_snapshot_failed_event(
        &self,
        project_id: Uuid,
        trigger: &str,
        err: &HivemindError,
        origin: &'static str,
    ) {
        let _ = self.append_event(
            Event::new(
                EventPayload::GraphSnapshotFailed {
                    project_id,
                    trigger: trigger.to_string(),
                    reason: err.message.clone(),
                    hint: err.recovery_hint.clone(),
                },
                CorrelationIds::for_project(project_id),
            ),
            origin,
        );
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_snapshot_fingerprint_is_order_independent() {
        let empty_stats = ucp_api::CodeGraphStats {
            total_nodes: 0,
            repository_nodes: 0,
            directory_nodes: 0,
            file_nodes: 0,
            symbol_nodes: 0,
            total_edges: 0,
            reference_edges: 0,
            export_edges: 0,
            languages: BTreeMap::new(),
        };
        let empty_document: PortableDocument = serde_json::from_value(serde_json::json!({
            "id": "doc",
            "root": "root",
            "structure": {},
            "blocks": {},
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z",
                "modified_at": "2024-01-01T00:00:00Z"
            },
            "version": 1
        }))
        .expect("portable document fixture");
        let repo_a = GraphSnapshotRepositoryArtifact {
            repo_name: "a".to_string(),
            repo_path: "/tmp/a".to_string(),
            commit_hash: "111".to_string(),
            profile_version: "v1".to_string(),
            canonical_fingerprint: "fa".to_string(),
            stats: empty_stats.clone(),
            document: empty_document.clone(),
            structure_blocks_projection: String::new(),
        };
        let repo_b = GraphSnapshotRepositoryArtifact {
            repo_name: "b".to_string(),
            repo_path: "/tmp/b".to_string(),
            commit_hash: "222".to_string(),
            profile_version: "v1".to_string(),
            canonical_fingerprint: "fb".to_string(),
            stats: empty_stats,
            document: empty_document,
            structure_blocks_projection: String::new(),
        };

        let first = Registry::compute_snapshot_fingerprint(&[repo_a.clone(), repo_b.clone()]);
        let second = Registry::compute_snapshot_fingerprint(&[repo_b, repo_a]);
        assert_eq!(first, second);
    }

    #[test]
    fn aggregate_codegraph_stats_sums_language_counts() {
        let mut first_languages = BTreeMap::new();
        first_languages.insert("rust".to_string(), 2);
        let mut second_languages = BTreeMap::new();
        second_languages.insert("rust".to_string(), 3);
        second_languages.insert("python".to_string(), 1);

        let summary = Registry::aggregate_codegraph_stats(&[
            ucp_api::CodeGraphStats {
                total_nodes: 1,
                repository_nodes: 1,
                directory_nodes: 0,
                file_nodes: 0,
                symbol_nodes: 0,
                total_edges: 2,
                reference_edges: 1,
                export_edges: 1,
                languages: first_languages,
            },
            ucp_api::CodeGraphStats {
                total_nodes: 4,
                repository_nodes: 0,
                directory_nodes: 1,
                file_nodes: 1,
                symbol_nodes: 2,
                total_edges: 3,
                reference_edges: 2,
                export_edges: 1,
                languages: second_languages,
            },
        ]);

        assert_eq!(summary.total_nodes, 5);
        assert_eq!(summary.reference_edges, 3);
        assert_eq!(summary.languages.get("rust"), Some(&5));
        assert_eq!(summary.languages.get("python"), Some(&1));
    }
}
