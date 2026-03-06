// AUTO-GENERATED from src/core/registry_original.rs
// Module bucket: graph

// list_graphs (2343-2358)
    pub fn list_graphs(&self, project_id_or_name: Option<&str>) -> Result<Vec<TaskGraph>> {
        let project_filter = match project_id_or_name {
            Some(id_or_name) => Some(self.get_project(id_or_name)?.id),
            None => None,
        };

        let state = self.state()?;
        let mut graphs: Vec<_> = state
            .graphs
            .into_values()
            .filter(|g| project_filter.is_none_or(|pid| g.project_id == pid))
            .collect();
        graphs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
        graphs.reverse();
        Ok(graphs)
    }

// graph_snapshot_path (4458-4461)
    fn graph_snapshot_path(&self, project_id: Uuid) -> PathBuf {
        self.governance_project_root(project_id)
            .join("graph_snapshot.json")
    }

// read_graph_snapshot_artifact (4474-4499)
    fn read_graph_snapshot_artifact(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Option<GraphSnapshotArtifact>> {
        let path = self.graph_snapshot_path(project_id);
        if !path.is_file() {
            return Ok(None);
        }
        // Governance bootstrap may create a placeholder `{}` before the first real snapshot build.
        // Treat placeholder/empty files as "no snapshot yet" for backward compatibility.
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

// resolve_repo_head_commit (4501-4523)
    fn resolve_repo_head_commit(repo_path: &Path, origin: &'static str) -> Result<String> {
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

// aggregate_codegraph_stats (4525-4541)
    fn aggregate_codegraph_stats(stats: &[ucp_api::CodeGraphStats]) -> GraphSnapshotSummary {
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

// compute_snapshot_fingerprint (4543-4556)
    fn compute_snapshot_fingerprint(repositories: &[GraphSnapshotRepositoryArtifact]) -> String {
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

// ensure_codegraph_scope_contract (4558-4618)
    fn ensure_codegraph_scope_contract(
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

// append_graph_snapshot_failed_event (11120-11139)
    fn append_graph_snapshot_failed_event(
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

// trigger_graph_snapshot_refresh (11141-11151)
    fn trigger_graph_snapshot_refresh(
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

// graph_snapshot_refresh (11153-11451)
    /// Rebuilds the UCP-backed static codegraph snapshot for a project.
    ///
    /// # Errors
    /// Returns an error when project/repository resolution fails, UCP extraction
    /// cannot produce a profile-compliant graph, or snapshot persistence fails.
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

// append_graph_query_executed_event (11453-11488)
    #[allow(clippy::too_many_arguments)]
    fn append_graph_query_executed_event(
        &self,
        project_id: Uuid,
        flow_id: Option<Uuid>,
        task_id: Option<Uuid>,
        attempt_id: Option<Uuid>,
        source: &str,
        result: &GraphQueryResult,
        duration_ms: u64,
        correlation: CorrelationIds,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::GraphQueryExecuted {
                    project_id,
                    flow_id,
                    task_id,
                    attempt_id,
                    source: source.to_string(),
                    query_kind: result.query_kind.clone(),
                    result_node_count: result.nodes.len(),
                    result_edge_count: result.edges.len(),
                    visited_node_count: result.cost.visited_nodes,
                    visited_edge_count: result.cost.visited_edges,
                    max_results: result.max_results,
                    truncated: result.truncated,
                    duration_ms,
                    canonical_fingerprint: result.canonical_fingerprint.clone(),
                },
                correlation,
            ),
            origin,
        )
    }

// append_graph_query_event_for_native_tool_call (11490-11527)
    #[allow(clippy::too_many_arguments)]
    fn append_graph_query_event_for_native_tool_call(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        tool_name: &str,
        response_payload: &str,
        correlation: CorrelationIds,
        origin: &'static str,
    ) -> Result<()> {
        if tool_name != "graph_query" {
            return Ok(());
        }
        let Ok(decoded) = serde_json::from_str::<serde_json::Value>(response_payload) else {
            return Ok(());
        };
        if decoded.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
            return Ok(());
        }
        let Some(output) = decoded.get("output") else {
            return Ok(());
        };
        let Ok(result) = serde_json::from_value::<GraphQueryResult>(output.clone()) else {
            return Ok(());
        };
        self.append_graph_query_executed_event(
            flow.project_id,
            Some(flow.id),
            Some(task_id),
            Some(attempt_id),
            "native_tool_graph_query",
            &result,
            result.duration_ms,
            correlation,
            origin,
        )
    }

// map_graph_query_error (11529-11549)
    fn map_graph_query_error(err: crate::core::graph_query::GraphQueryError) -> HivemindError {
        let mut mapped = HivemindError::user(err.code, err.message, "registry:graph_query_execute");
        if matches!(
            mapped.code.as_str(),
            "graph_query_bounds_invalid"
                | "graph_query_bounds_exceeded"
                | "graph_query_depth_invalid"
                | "graph_query_depth_exceeded"
        ) {
            mapped =
                mapped.with_hint("Reduce --max-results/--depth to fit bounded graph query limits");
        } else if matches!(
            mapped.code.as_str(),
            "graph_query_node_not_found" | "graph_query_node_ambiguous"
        ) {
            mapped = mapped.with_hint(
                "Use 'hivemind graph query filter <project> --type file' to inspect available node IDs",
            );
        }
        mapped
    }

// encode_runtime_graph_query_gate_error (11551-11567)
    fn encode_runtime_graph_query_gate_error(error: &HivemindError) -> String {
        let payload = RuntimeGraphQueryGateError {
            code: error.code.clone(),
            message: error.message.clone(),
            hint: error.recovery_hint.clone().or_else(|| {
                if error.code.starts_with("graph_snapshot_") {
                    Some(GRAPH_QUERY_REFRESH_HINT.to_string())
                } else {
                    None
                }
            }),
        };
        serde_json::to_string(&payload).unwrap_or_else(|_| {
            "{\"code\":\"graph_snapshot_invalid\",\"message\":\"graph query gate failed\"}"
                .to_string()
        })
    }

// set_native_graph_query_runtime_env (11569-11605)
    fn set_native_graph_query_runtime_env(
        &self,
        project: &Project,
        runtime_env: &mut HashMap<String, String>,
        origin: &'static str,
    ) {
        runtime_env.remove(GRAPH_QUERY_ENV_GATE_ERROR);
        runtime_env.remove(GRAPH_QUERY_ENV_SNAPSHOT_PATH);
        runtime_env.remove(GRAPH_QUERY_ENV_CONSTITUTION_PATH);

        if project.repositories.is_empty() {
            return;
        }

        match self.ensure_graph_snapshot_current_for_constitution(project, origin) {
            Ok(()) => {
                runtime_env.insert(
                    GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
                    self.graph_snapshot_path(project.id)
                        .to_string_lossy()
                        .to_string(),
                );
                runtime_env.insert(
                    GRAPH_QUERY_ENV_CONSTITUTION_PATH.to_string(),
                    self.constitution_path(project.id)
                        .to_string_lossy()
                        .to_string(),
                );
            }
            Err(error) => {
                runtime_env.insert(
                    GRAPH_QUERY_ENV_GATE_ERROR.to_string(),
                    Self::encode_runtime_graph_query_gate_error(&error),
                );
            }
        }
    }

// graph_query_execute (11607-11663)
    pub fn graph_query_execute(
        &self,
        id_or_name: &str,
        request: &GraphQueryRequest,
        source: &str,
    ) -> Result<GraphQueryResult> {
        let origin = "registry:graph_query_execute";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_graph_snapshot_current_for_constitution(&project, origin)?;
        let snapshot = self
            .read_graph_snapshot_artifact(project.id, origin)?
            .ok_or_else(|| {
                HivemindError::user(
                    "graph_snapshot_missing",
                    "Graph snapshot is missing for this project",
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>")
            })?;

        let partition_paths =
            load_partition_paths_from_constitution(&self.constitution_path(project.id));
        let repositories = snapshot
            .repositories
            .iter()
            .map(|repo| GraphQueryRepository {
                repo_name: repo.repo_name.clone(),
                document: repo.document.clone(),
            })
            .collect::<Vec<_>>();
        let index = GraphQueryIndex::from_snapshot_repositories(&repositories, &partition_paths)
            .map_err(Self::map_graph_query_error)?;
        let started = Instant::now();
        let mut result = index
            .execute(
                request,
                &snapshot.canonical_fingerprint,
                &GraphQueryBounds::default(),
            )
            .map_err(Self::map_graph_query_error)?;
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        result.duration_ms = duration_ms;
        self.append_graph_query_executed_event(
            project.id,
            None,
            None,
            None,
            source,
            &result,
            duration_ms,
            CorrelationIds::for_project(project.id),
            origin,
        )?;
        Ok(result)
    }

// ensure_graph_snapshot_current_for_constitution (11665-11789)
    #[allow(clippy::too_many_lines)]
    fn ensure_graph_snapshot_current_for_constitution(
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

// normalize_graph_path (11791-11797)
    fn normalize_graph_path(path: &str) -> String {
        path.trim()
            .replace('\\', "/")
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string()
    }

// path_in_partition (11799-11807)
    fn path_in_partition(path: &str, partition_path: &str) -> bool {
        let normalized_path = Self::normalize_graph_path(path);
        let normalized_partition = Self::normalize_graph_path(partition_path);
        if normalized_partition.is_empty() {
            return false;
        }
        normalized_path == normalized_partition
            || normalized_path.starts_with(&format!("{normalized_partition}/"))
    }

// graph_facts_from_snapshot (11809-11911)
    fn graph_facts_from_snapshot(
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

// evaluate_constitution_rules (11913-12098)
    #[allow(clippy::too_many_lines)]
    fn evaluate_constitution_rules(
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

// append_constitution_violation_events (12100-12131)
    fn append_constitution_violation_events(
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

// run_constitution_check (12133-12243)
    fn run_constitution_check(
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

// enforce_constitution_gate (12245-12285)
    fn enforce_constitution_gate(
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

// get_graph (14293-14310)
    pub fn get_graph(&self, graph_id: &str) -> Result<TaskGraph> {
        let id = Uuid::parse_str(graph_id).map_err(|_| {
            HivemindError::user(
                "invalid_graph_id",
                format!("'{graph_id}' is not a valid graph ID"),
                "registry:get_graph",
            )
        })?;

        let state = self.state()?;
        state.graphs.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "graph_not_found",
                format!("Graph '{graph_id}' not found"),
                "registry:get_graph",
            )
        })
    }

// create_graph (14312-14392)
    pub fn create_graph(
        &self,
        project_id_or_name: &str,
        name: &str,
        from_tasks: &[Uuid],
    ) -> Result<TaskGraph> {
        let project = self
            .get_project(project_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let mut tasks_to_add = Vec::new();
        for tid in from_tasks {
            let task = state.tasks.get(tid).cloned().ok_or_else(|| {
                let err = HivemindError::user(
                    "task_not_found",
                    format!("Task '{tid}' not found"),
                    "registry:create_graph",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                err
            })?;
            if task.state != TaskState::Open {
                let err = HivemindError::user(
                    "task_not_open",
                    format!("Task '{tid}' is not open"),
                    "registry:create_graph",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
            tasks_to_add.push(task);
        }

        let graph_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::TaskGraphCreated {
                graph_id,
                project_id: project.id,
                name: name.to_string(),
                description: None,
            },
            CorrelationIds::for_graph(project.id, graph_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:create_graph",
            )
        })?;

        for task in tasks_to_add {
            let graph_task = GraphTask {
                id: task.id,
                title: task.title,
                description: task.description,
                criteria: SuccessCriteria::new("Done"),
                retry_policy: RetryPolicy::default(),
                checkpoints: vec!["checkpoint-1".to_string()],
                scope: task.scope,
            };
            let event = Event::new(
                EventPayload::TaskAddedToGraph {
                    graph_id,
                    task: graph_task,
                },
                CorrelationIds::for_graph(project.id, graph_id),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system(
                    "event_append_failed",
                    e.to_string(),
                    "registry:create_graph",
                )
            })?;
        }

        self.get_graph(&graph_id.to_string())
    }

// add_graph_dependency (14394-14516)
    #[allow(clippy::too_many_lines)]
    pub fn add_graph_dependency(
        &self,
        graph_id: &str,
        from_task: &str,
        to_task: &str,
    ) -> Result<TaskGraph> {
        let gid = Uuid::parse_str(graph_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_graph_id",
                format!("'{graph_id}' is not a valid graph ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let from = Uuid::parse_str(from_task).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{from_task}' is not a valid task ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let to = Uuid::parse_str(to_task).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{to_task}' is not a valid task ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        let state = self.state()?;
        let graph = state.graphs.get(&gid).cloned().ok_or_else(|| {
            let err = HivemindError::user(
                "graph_not_found",
                format!("Graph '{graph_id}' not found"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        if graph.state != GraphState::Draft {
            let locking_flow_id = state
                .flows
                .values()
                .filter(|flow| flow.graph_id == gid)
                .max_by_key(|flow| flow.updated_at)
                .map(|flow| flow.id);
            let message = locking_flow_id.map_or_else(
                || format!("Graph '{graph_id}' is immutable"),
                |flow_id| format!("Graph '{graph_id}' is immutable (locked by flow '{flow_id}')"),
            );

            let mut err = HivemindError::user(
                "graph_immutable",
                message,
                "registry:add_graph_dependency",
            )
            .with_hint(
                "Create a new graph if you need additional dependencies, or modify task execution in the existing flow",
            );
            if let Some(flow_id) = locking_flow_id {
                err = err.with_context("locking_flow_id", flow_id.to_string());
            }
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            return Err(err);
        }

        if !graph.tasks.contains_key(&from) || !graph.tasks.contains_key(&to) {
            let err = HivemindError::user(
                "task_not_in_graph",
                "One or more tasks are not in the graph",
                "registry:add_graph_dependency",
            )
            .with_hint("Ensure both task IDs were included when the graph was created");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            return Err(err);
        }

        if graph
            .dependencies
            .get(&to)
            .is_some_and(|deps| deps.contains(&from))
        {
            return Ok(graph);
        }

        let mut graph_for_check = graph.clone();
        graph_for_check.add_dependency(to, from).map_err(|e| {
            let err = HivemindError::user(
                "cycle_detected",
                e.to_string(),
                "registry:add_graph_dependency",
            )
            .with_hint("Remove one dependency in the cycle and try again");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            err
        })?;

        let event = Event::new(
            EventPayload::DependencyAdded {
                graph_id: gid,
                from_task: from,
                to_task: to,
            },
            CorrelationIds::for_graph(graph.project_id, gid),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:add_graph_dependency",
            )
        })?;

        self.get_graph(graph_id)
    }

// add_graph_task_check (14518-14587)
    pub fn add_graph_task_check(
        &self,
        graph_id: &str,
        task_id: &str,
        check: crate::core::verification::CheckConfig,
    ) -> Result<TaskGraph> {
        let origin = "registry:add_graph_task_check";
        let gid = Uuid::parse_str(graph_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_graph_id",
                format!("'{graph_id}' is not a valid graph ID"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let tid = Uuid::parse_str(task_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        let state = self.state()?;
        let graph = state.graphs.get(&gid).cloned().ok_or_else(|| {
            let err = HivemindError::user(
                "graph_not_found",
                format!("Graph '{graph_id}' not found"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        if graph.state != GraphState::Draft {
            let err = HivemindError::user(
                "graph_immutable",
                format!("Graph '{graph_id}' is immutable"),
                origin,
            )
            .with_hint("Checks can only be added to draft graphs");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }
        if !graph.tasks.contains_key(&tid) {
            let err = HivemindError::user(
                "task_not_in_graph",
                format!("Task '{task_id}' is not part of graph '{graph_id}'"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::GraphTaskCheckAdded {
                graph_id: gid,
                task_id: tid,
                check,
            },
            CorrelationIds::for_graph(graph.project_id, gid),
        );
        self.store
            .append(event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        self.get_graph(graph_id)
    }

// validate_graph_issues (14589-14651)
    fn validate_graph_issues(graph: &TaskGraph) -> Vec<String> {
        fn has_cycle(graph: &TaskGraph) -> bool {
            use std::collections::HashSet;

            fn visit(
                graph: &TaskGraph,
                node: Uuid,
                visited: &mut HashSet<Uuid>,
                stack: &mut HashSet<Uuid>,
            ) -> bool {
                if stack.contains(&node) {
                    return true;
                }
                if visited.contains(&node) {
                    return false;
                }

                visited.insert(node);
                stack.insert(node);

                if let Some(deps) = graph.dependencies.get(&node) {
                    for dep in deps {
                        if visit(graph, *dep, visited, stack) {
                            return true;
                        }
                    }
                }

                stack.remove(&node);
                false
            }

            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            for node in graph.tasks.keys() {
                if visit(graph, *node, &mut visited, &mut stack) {
                    return true;
                }
            }
            false
        }

        if graph.tasks.is_empty() {
            return vec!["Graph must contain at least one task".to_string()];
        }

        for (task_id, deps) in &graph.dependencies {
            if !graph.tasks.contains_key(task_id) {
                return vec![format!("Task not found: {task_id}")];
            }
            for dep in deps {
                if !graph.tasks.contains_key(dep) {
                    return vec![format!("Task not found: {dep}")];
                }
            }
        }

        if has_cycle(graph) {
            return vec!["Cycle detected in task dependencies".to_string()];
        }

        Vec::new()
    }

// validate_graph (14653-14661)
    pub fn validate_graph(&self, graph_id: &str) -> Result<GraphValidationResult> {
        let graph = self.get_graph(graph_id)?;
        let issues = Self::validate_graph_issues(&graph);
        Ok(GraphValidationResult {
            graph_id: graph.id,
            valid: issues.is_empty(),
            issues,
        })
    }

// delete_graph (14663-14700)
    /// Deletes a graph.
    ///
    /// # Errors
    /// Returns an error if the graph is referenced by any flow.
    pub fn delete_graph(&self, graph_id: &str) -> Result<Uuid> {
        let graph = self
            .get_graph(graph_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        if state.flows.values().any(|flow| flow.graph_id == graph.id) {
            let err = HivemindError::user(
                "graph_in_use",
                "Graph is referenced by an existing flow",
                "registry:delete_graph",
            )
            .with_hint("Delete the flow(s) that reference this graph first");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::TaskGraphDeleted {
                graph_id: graph.id,
                project_id: graph.project_id,
            },
            CorrelationIds::for_graph(graph.project_id, graph.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:delete_graph",
            )
        })?;

        Ok(graph.id)
    }

