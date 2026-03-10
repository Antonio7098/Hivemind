use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredGraphSnapshotMetadata {
    version: u32,
    schema_version: String,
    snapshot_version: u32,
    profile_version: String,
    canonical_fingerprint: String,
    provenance: GraphSnapshotProvenance,
    summary: GraphSnapshotSummary,
    static_projection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredGraphSnapshotRepositoryManifest {
    repo_name: String,
    repo_path: String,
    worktree_path: String,
    commit_hash: String,
    profile_version: String,
    canonical_fingerprint: String,
    stats: ucp_api::CodeGraphStats,
    graph_key: String,
}

impl Registry {
    fn runtime_graph_store_path(db_path: &Path, substrate_kind: &str) -> PathBuf {
        db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("graph-store")
            .join(format!("{substrate_kind}.sqlite"))
    }

    fn persist_codegraph_documents_to_runtime_store(
        store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
        registry_key: &str,
        artifact: &GraphSnapshotArtifact,
        origin: &'static str,
    ) -> Result<(String, String)> {
        let storage_path = Self::runtime_graph_store_path(&store.db_path, "codegraph");
        if let Some(parent) = storage_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                HivemindError::system(
                    "graph_snapshot_registry_storage_failed",
                    format!(
                        "failed to create graph store directory '{}': {error}",
                        parent.display()
                    ),
                    origin,
                )
            })?;
        }

        let repo_manifest = artifact
            .repositories
            .iter()
            .enumerate()
            .map(|(index, repo)| {
                let graph_key = format!("{registry_key}:repo:{index}");
                ucp_graph::GraphNavigator::from_portable(&repo.document)
                    .and_then(|navigator| navigator.persist_sqlite(&storage_path, &graph_key))
                    .map_err(|error| {
                        HivemindError::system(
                            "graph_snapshot_registry_storage_failed",
                            format!(
                                "failed to persist GraphCode document for repository '{}': {error}",
                                repo.repo_name
                            ),
                            origin,
                        )
                    })?;
                Ok(StoredGraphSnapshotRepositoryManifest {
                    repo_name: repo.repo_name.clone(),
                    repo_path: repo.repo_path.clone(),
                    worktree_path: repo.repo_path.clone(),
                    commit_hash: repo.commit_hash.clone(),
                    profile_version: repo.profile_version.clone(),
                    canonical_fingerprint: repo.canonical_fingerprint.clone(),
                    stats: repo.stats.clone(),
                    graph_key,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        serde_json::to_string(&repo_manifest)
            .map(|manifest| (storage_path.to_string_lossy().to_string(), manifest))
            .map_err(|error| {
                HivemindError::system(
                    "graph_snapshot_registry_encode_failed",
                    error.to_string(),
                    origin,
                )
            })
    }

    fn load_external_codegraph_artifact(
        record: &crate::native::runtime_hardening::GraphCodeArtifactRecord,
        origin: &'static str,
    ) -> Result<GraphSnapshotArtifact> {
        let metadata = serde_json::from_str::<StoredGraphSnapshotMetadata>(&record.snapshot_json)
            .map_err(|error| {
            HivemindError::system(
                "graph_snapshot_registry_decode_failed",
                format!("failed to decode external GraphCode metadata: {error}"),
                origin,
            )
        })?;
        let repositories = serde_json::from_str::<Vec<StoredGraphSnapshotRepositoryManifest>>(
            &record.repo_manifest_json,
        )
        .map_err(|error| {
            HivemindError::system(
                "graph_snapshot_registry_decode_failed",
                format!("failed to decode external GraphCode manifest: {error}"),
                origin,
            )
        })?
        .into_iter()
        .map(|repo| {
            let document =
                ucp_graph::GraphNavigator::open_sqlite(&record.storage_reference, &repo.graph_key)
                    .and_then(|navigator| navigator.to_portable_document())
                    .map_err(|error| {
                        HivemindError::system(
                            "graph_snapshot_registry_storage_failed",
                            format!(
                                "failed to load GraphCode document for repository '{}': {error}",
                                repo.repo_name
                            ),
                            origin,
                        )
                    })?;
            Ok(GraphSnapshotRepositoryArtifact {
                repo_name: repo.repo_name,
                repo_path: repo.repo_path,
                commit_hash: repo.commit_hash,
                profile_version: repo.profile_version,
                canonical_fingerprint: repo.canonical_fingerprint,
                stats: repo.stats,
                structure_blocks_projection: String::new(),
                document,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        Ok(GraphSnapshotArtifact {
            schema_version: metadata.schema_version,
            snapshot_version: metadata.snapshot_version,
            profile_version: metadata.profile_version,
            ucp_engine_version: record.ucp_engine_version.clone(),
            canonical_fingerprint: metadata.canonical_fingerprint,
            summary: metadata.summary,
            provenance: metadata.provenance,
            repositories,
            static_projection: metadata.static_projection,
        })
    }

    pub(crate) fn graph_snapshot_path(&self, project_id: Uuid) -> PathBuf {
        self.governance_project_root(project_id)
            .join("graph_snapshot.json")
    }

    pub(crate) fn graphcode_registry_key(project_id: Uuid) -> String {
        format!("{project_id}:codegraph")
    }

    pub(crate) fn graphcode_attempt_registry_prefix(project_id: Uuid) -> String {
        format!("{}:attempt:", Self::graphcode_registry_key(project_id))
    }

    pub(crate) fn graphcode_attempt_registry_key(project_id: Uuid, attempt_id: Uuid) -> String {
        format!(
            "{}{}",
            Self::graphcode_attempt_registry_prefix(project_id),
            attempt_id
        )
    }

    pub(crate) fn open_native_runtime_state_store(
        &self,
        origin: &'static str,
    ) -> Result<crate::native::runtime_hardening::NativeRuntimeStateStore> {
        let config = crate::native::runtime_hardening::RuntimeHardeningConfig::for_state_dir(
            &self.config.data_dir.join("native-runtime"),
        );
        crate::native::runtime_hardening::NativeRuntimeStateStore::open(&config)
            .map_err(|error| HivemindError::system(error.code, error.message, origin))
    }

    pub(crate) fn sync_graph_snapshot_into_runtime_registry(
        &self,
        project_id: Uuid,
        artifact: &GraphSnapshotArtifact,
        constitution_path: Option<&Path>,
        origin: &'static str,
    ) -> Result<()> {
        let store = self.open_native_runtime_state_store(origin)?;
        let registry_key = Self::graphcode_registry_key(project_id);
        let active_session_ref = format!("{registry_key}:session:default");
        let (storage_reference, repo_manifest_json) =
            Self::persist_codegraph_documents_to_runtime_store(
                &store,
                &registry_key,
                artifact,
                origin,
            )?;
        let snapshot_json = serde_json::to_string(&StoredGraphSnapshotMetadata {
            version: 1,
            schema_version: artifact.schema_version.clone(),
            snapshot_version: artifact.snapshot_version,
            profile_version: artifact.profile_version.clone(),
            canonical_fingerprint: artifact.canonical_fingerprint.clone(),
            provenance: artifact.provenance.clone(),
            summary: artifact.summary.clone(),
            static_projection: artifact.static_projection.clone(),
        })
        .map_err(|error| {
            HivemindError::system(
                "graph_snapshot_registry_encode_failed",
                error.to_string(),
                origin,
            )
        })?;
        store
            .upsert_graphcode_artifact(&crate::native::runtime_hardening::GraphCodeArtifactUpsert {
                registry_key: registry_key.clone(),
                project_id: project_id.to_string(),
                substrate_kind: "codegraph".to_string(),
                storage_backend: "ucp_graph_sqlite".to_string(),
                storage_reference,
                derivative_snapshot_path: Some(
                    self.graph_snapshot_path(project_id)
                        .to_string_lossy()
                        .to_string(),
                ),
                constitution_path: constitution_path.map(|path| path.to_string_lossy().to_string()),
                canonical_fingerprint: artifact.canonical_fingerprint.clone(),
                profile_version: artifact.profile_version.clone(),
                ucp_engine_version: artifact.ucp_engine_version.clone(),
                extractor_version: CODEGRAPH_PROFILE_MARKER.to_string(),
                runtime_version: env!("CARGO_PKG_VERSION").to_string(),
                freshness_state: "current".to_string(),
                repo_manifest_json,
                active_session_ref: Some(active_session_ref.clone()),
                snapshot_json,
            })
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?;
        store
            .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
                session_ref: active_session_ref,
                registry_key,
                substrate_kind: "codegraph".to_string(),
                current_focus_json: "[]".to_string(),
                pinned_nodes_json: "[]".to_string(),
                recent_traversals_json: "[]".to_string(),
                working_set_refs_json: "[]".to_string(),
                hydrated_excerpts_json: "[]".to_string(),
                path_artifacts_json: "[]".to_string(),
                snapshot_fingerprint: artifact.canonical_fingerprint.clone(),
                freshness_state: "current".to_string(),
            })
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?;
        Ok(())
    }

    pub(crate) fn mark_graph_snapshot_registry_freshness(
        &self,
        project_id: Uuid,
        freshness_state: &str,
        origin: &'static str,
    ) -> Result<()> {
        let store = self.open_native_runtime_state_store(origin)?;
        store
            .mark_graphcode_artifact_freshness(
                &project_id.to_string(),
                "codegraph",
                freshness_state,
            )
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?;
        store
            .mark_graphcode_session_freshness_by_registry_prefix(
                &Self::graphcode_registry_key(project_id),
                freshness_state,
            )
            .map_err(|error| HivemindError::system(error.code, error.message, origin))
    }

    pub(crate) fn mark_attempt_graph_snapshot_registry_freshness(
        &self,
        project_id: Uuid,
        attempt_id: Uuid,
        freshness_state: &str,
        origin: &'static str,
    ) -> Result<()> {
        let store = self.open_native_runtime_state_store(origin)?;
        let registry_key = Self::graphcode_attempt_registry_key(project_id, attempt_id);
        store
            .mark_graphcode_artifact_freshness_by_registry_key(&registry_key, freshness_state)
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?;
        store
            .mark_graphcode_session_freshness_by_registry_key(&registry_key, freshness_state)
            .map_err(|error| HivemindError::system(error.code, error.message, origin))
    }

    pub(crate) fn mark_execution_graph_snapshot_registries_freshness(
        &self,
        project_id: Uuid,
        freshness_state: &str,
        origin: &'static str,
    ) -> Result<()> {
        let store = self.open_native_runtime_state_store(origin)?;
        let attempt_prefix = Self::graphcode_attempt_registry_prefix(project_id);
        store
            .mark_graphcode_artifact_freshness_by_registry_prefix(&attempt_prefix, freshness_state)
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?;
        store
            .mark_graphcode_session_freshness_by_registry_prefix(&attempt_prefix, freshness_state)
            .map_err(|error| HivemindError::system(error.code, error.message, origin))
    }

    pub(crate) fn mark_attempt_graph_snapshot_dirty_paths(
        &self,
        project_id: Uuid,
        attempt_id: Uuid,
        dirty_paths: &[PathBuf],
        refresh_reason: &str,
        origin: &'static str,
    ) -> Result<()> {
        let store = self.open_native_runtime_state_store(origin)?;
        crate::native::tool_engine::mark_runtime_graph_registry_dirty(
            &store,
            &Self::graphcode_attempt_registry_key(project_id, attempt_id),
            dirty_paths,
            Some(refresh_reason),
        )
        .map_err(|error| HivemindError::system(error.code, error.message, origin))
    }

    pub(crate) fn read_authoritative_graph_snapshot_artifact(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Option<GraphSnapshotArtifact>> {
        if let Some(record) = self
            .open_native_runtime_state_store(origin)?
            .graphcode_artifact_by_project(&project_id.to_string(), "codegraph")
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?
        {
            let artifact = if let Ok(artifact) =
                serde_json::from_str::<GraphSnapshotArtifact>(&record.snapshot_json)
            {
                artifact
            } else {
                Self::load_external_codegraph_artifact(&record, origin)?
            };
            return Ok(Some(artifact));
        }
        let artifact = self.read_graph_snapshot_artifact(project_id, origin)?;
        if let Some(artifact) = artifact.as_ref() {
            let constitution_path = self.constitution_path(project_id);
            let constitution = constitution_path
                .is_file()
                .then_some(constitution_path.as_path());
            self.sync_graph_snapshot_into_runtime_registry(
                project_id,
                artifact,
                constitution,
                origin,
            )?;
        }
        Ok(artifact)
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
            if matches!(class, "file" | "symbol")
                && Self::graph_block_path_value(&block.metadata.custom).is_none()
            {
                return Err(HivemindError::system(
                    "graph_snapshot_path_missing",
                    format!("UCP codegraph {class} node is missing required path metadata"),
                    origin,
                )
                .with_hint(
                    "Refresh with a UCP CodeGraphProfile v1 extractor that emits path metadata via custom.path or custom.coderef.path",
                ));
            }

            for edge in &block.edges {
                let edge_type = edge.edge_type.as_str();
                if edge_type != "references" && !edge_type.starts_with("custom:") {
                    return Err(HivemindError::system(
                            "graph_snapshot_scope_unsupported",
                            format!("Unsupported codegraph edge type '{edge_type}'"),
                            origin,
                        )
                        .with_hint(
                            "Refresh with a UCP CodeGraphProfile v1 extractor that only emits references plus custom semantic edges",
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
}
