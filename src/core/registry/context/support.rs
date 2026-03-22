use super::*;

pub(crate) struct AttemptContextSourceData {
    pub(crate) constitution_path: String,
    pub(crate) constitution_revision: Option<u64>,
    pub(crate) constitution_digest: Option<String>,
    pub(crate) constitution_content_hash: Option<String>,
    pub(crate) constitution_section: String,
    pub(crate) template_id: Option<String>,
    pub(crate) template_event_id: Option<String>,
    pub(crate) template_schema_version: Option<String>,
    pub(crate) template_projection_version: Option<u32>,
    pub(crate) system_prompt_manifest: Option<AttemptContextSystemPromptManifest>,
    pub(crate) system_prompt_section: String,
    pub(crate) skill_manifest: Vec<AttemptContextSkillManifest>,
    pub(crate) skill_sections: Vec<String>,
    pub(crate) document_manifest: Vec<AttemptContextDocumentManifest>,
    pub(crate) document_sections: Vec<String>,
    pub(crate) workflow_manifest: Option<AttemptContextWorkflowManifest>,
    pub(crate) workflow_section: Option<String>,
    pub(crate) graph_summary: AttemptContextGraphManifest,
    pub(crate) graph_section: String,
    pub(crate) retry_links: Vec<AttemptContextRetryLink>,
    pub(crate) prior_manifest_hashes: Vec<String>,
    pub(crate) template_document_ids: Vec<String>,
    pub(crate) included_document_ids: Vec<String>,
    pub(crate) excluded_document_ids: Vec<String>,
    pub(crate) resolved_document_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextGraphStorageManifest {
    graph_key: String,
    project_name: String,
    task_title: String,
    worktree_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextGraphSnapshotMetadata {
    version: u32,
    schema_version: String,
    manifest_version: u32,
    flow_id: Uuid,
    task_id: Uuid,
    attempt_id: Uuid,
    runtime_name: String,
    manifest_hash: String,
    context_hash: String,
    ordered_inputs: Vec<String>,
    excluded_sources: Vec<String>,
    skill_count: usize,
    document_count: usize,
    retry_count: usize,
}

impl Registry {
    fn attempt_context_runtime_graph_store_path(db_path: &Path) -> PathBuf {
        db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("graph-store")
            .join("graph.sqlite")
    }

    fn build_attempt_context_runtime_graph_document(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        runtime_name: &str,
        manifest: &AttemptContextManifest,
        manifest_hash: &str,
        context_hash: &str,
        origin: &'static str,
    ) -> Result<(
        PortableDocument,
        AttemptContextGraphStorageManifest,
        AttemptContextGraphSnapshotMetadata,
    )> {
        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "attempt_context_project_missing",
                format!(
                    "project '{}' is not available while building attempt-context graph",
                    flow.project_id
                ),
                origin,
            )
        })?;
        let task = state.tasks.get(&task_id).ok_or_else(|| {
            HivemindError::system(
                "attempt_context_task_missing",
                format!("task '{task_id}' is not available while building attempt-context graph"),
                origin,
            )
        })?;

        let mut document = ucm_core::Document::create();
        let root_id = document.root;
        let registry_key = format!("{}:graph", flow.project_id);
        let graph_key = format!("{registry_key}:attempt:{attempt_id}");

        let add_node = |document: &mut ucm_core::Document,
                        parent: &ucm_core::BlockId,
                        label: String,
                        tag: &str,
                        content: String,
                        custom: Vec<(&str, serde_json::Value)>|
         -> Result<ucm_core::BlockId> {
            let mut block_id = [0u8; 12];
            block_id.copy_from_slice(&Uuid::new_v4().as_bytes()[..12]);
            let mut block = ucm_core::Block::with_id(
                ucm_core::BlockId::from_bytes(block_id),
                ucm_core::Content::text(content.clone()),
            );
            let metadata = custom.into_iter().fold(
                block
                    .metadata
                    .clone()
                    .with_label(label)
                    .with_tag(tag)
                    .with_summary(content),
                |metadata, (key, value)| metadata.with_custom(key, value),
            );
            block = block.with_metadata(metadata);
            document.add_block(block, parent).map_err(|error| {
                HivemindError::system(
                    "attempt_context_graph_build_failed",
                    format!("failed to add attempt-context graph block: {error}"),
                    origin,
                )
            })
        };

        let project_block = add_node(
            &mut document,
            &root_id,
            format!("Project · {}", project.name),
            "project",
            format!(
                "project_id={}\nrepositories={}\nconstitution_digest={}",
                project.id,
                project.repositories.len(),
                project.constitution_digest.clone().unwrap_or_default()
            ),
            vec![
                ("logical_key", serde_json::json!("project")),
                ("project_id", serde_json::json!(project.id.to_string())),
            ],
        )?;
        let flow_block = add_node(
            &mut document,
            &root_id,
            format!("Flow · {}", flow.id),
            "flow",
            format!(
                "flow_id={}\nstate={:?}\nrun_mode={:?}\nbase_revision={}",
                flow.id,
                flow.state,
                flow.run_mode,
                flow.base_revision.clone().unwrap_or_default()
            ),
            vec![
                ("logical_key", serde_json::json!("flow")),
                ("flow_id", serde_json::json!(flow.id.to_string())),
            ],
        )?;
        let task_block = add_node(
            &mut document,
            &root_id,
            format!("Task · {}", task.title),
            "task",
            format!(
                "task_id={}\nstate={:?}\nrun_mode={:?}",
                task.id, task.state, task.run_mode
            ),
            vec![
                ("logical_key", serde_json::json!("task")),
                ("task_id", serde_json::json!(task.id.to_string())),
            ],
        )?;
        let runtime_block = add_node(
            &mut document,
            &root_id,
            format!("Runtime · {runtime_name}"),
            "runtime",
            format!("attempt_id={}\nruntime_name={runtime_name}", attempt_id),
            vec![
                ("logical_key", serde_json::json!("runtime")),
                ("attempt_id", serde_json::json!(attempt_id.to_string())),
            ],
        )?;
        let constitution_block = add_node(
            &mut document,
            &root_id,
            "Constitution".to_string(),
            "constitution",
            format!(
                "path={}\nrevision={}\ndigest={}\ncontent_hash={}",
                manifest.constitution.path,
                manifest.constitution.revision.unwrap_or_default(),
                manifest.constitution.digest.clone().unwrap_or_default(),
                manifest
                    .constitution
                    .content_hash
                    .clone()
                    .unwrap_or_default()
            ),
            vec![("logical_key", serde_json::json!("constitution"))],
        )?;
        if let Some(system_prompt) = manifest.system_prompt.as_ref() {
            let prompt_block = add_node(
                &mut document,
                &root_id,
                format!("System Prompt · {}", system_prompt.prompt_id),
                "system-prompt",
                format!(
                    "prompt_id={}\ncontent_hash={}",
                    system_prompt.prompt_id, system_prompt.content_hash
                ),
                vec![("logical_key", serde_json::json!("system_prompt"))],
            )?;
            document.add_edge(&task_block, ucm_core::EdgeType::References, prompt_block);
        }
        let graph_summary_block = add_node(
            &mut document,
            &root_id,
            "Graph Summary".to_string(),
            "graph-summary",
            format!(
                "present={}\nrepositories={}\ntotal_nodes={}\ntotal_edges={}\nfingerprint={}",
                manifest.graph_summary.present,
                manifest.graph_summary.repository_count,
                manifest.graph_summary.total_nodes,
                manifest.graph_summary.total_edges,
                manifest
                    .graph_summary
                    .canonical_fingerprint
                    .clone()
                    .unwrap_or_default()
            ),
            vec![("logical_key", serde_json::json!("graph_summary"))],
        )?;

        let skills_group = add_node(
            &mut document,
            &root_id,
            format!("Skills · {}", manifest.skills.len()),
            "skills",
            format!("count={}", manifest.skills.len()),
            vec![("logical_key", serde_json::json!("skills"))],
        )?;
        for skill in &manifest.skills {
            let skill_block = add_node(
                &mut document,
                &skills_group,
                format!("Skill · {}", skill.skill_id),
                "skill",
                format!(
                    "skill_id={}\ncontent_hash={}",
                    skill.skill_id, skill.content_hash
                ),
                vec![(
                    "logical_key",
                    serde_json::json!(format!("skill:{}", skill.skill_id)),
                )],
            )?;
            document.add_edge(&task_block, ucm_core::EdgeType::References, skill_block);
        }

        let documents_group = add_node(
            &mut document,
            &root_id,
            format!("Documents · {}", manifest.documents.len()),
            "documents",
            format!("count={}", manifest.documents.len()),
            vec![("logical_key", serde_json::json!("documents"))],
        )?;
        for item in &manifest.documents {
            let document_block = add_node(
                &mut document,
                &documents_group,
                format!("Document · {}", item.document_id),
                "document",
                format!(
                    "document_id={}\nsource={}\nrevision={}\ncontent_hash={}",
                    item.document_id, item.source, item.revision, item.content_hash
                ),
                vec![(
                    "logical_key",
                    serde_json::json!(format!("document:{}", item.document_id)),
                )],
            )?;
            document.add_edge(&task_block, ucm_core::EdgeType::References, document_block);
        }

        let retries_group = add_node(
            &mut document,
            &root_id,
            format!("Retry Links · {}", manifest.retry_links.len()),
            "retries",
            format!("count={}", manifest.retry_links.len()),
            vec![("logical_key", serde_json::json!("retry_links"))],
        )?;
        for retry in &manifest.retry_links {
            let retry_block = add_node(
                &mut document,
                &retries_group,
                format!("Retry Attempt · {}", retry.attempt_id),
                "retry-link",
                format!(
                    "attempt_id={}\nmanifest_hash={}",
                    retry.attempt_id,
                    retry.manifest_hash.clone().unwrap_or_default()
                ),
                vec![(
                    ("logical_key"),
                    serde_json::json!(format!("retry:{}", retry.attempt_id)),
                )],
            )?;
            document.add_edge(&task_block, ucm_core::EdgeType::DerivedFrom, retry_block);
        }

        document.add_edge(&project_block, ucm_core::EdgeType::References, flow_block);
        document.add_edge(&flow_block, ucm_core::EdgeType::References, task_block);
        document.add_edge(&task_block, ucm_core::EdgeType::References, runtime_block);
        document.add_edge(
            &task_block,
            ucm_core::EdgeType::References,
            constitution_block,
        );
        document.add_edge(
            &task_block,
            ucm_core::EdgeType::References,
            graph_summary_block,
        );
        document.add_edge(&task_block, ucm_core::EdgeType::References, skills_group);
        document.add_edge(&task_block, ucm_core::EdgeType::References, documents_group);
        document.add_edge(&task_block, ucm_core::EdgeType::References, retries_group);

        Ok((
            document.to_portable(),
            AttemptContextGraphStorageManifest {
                graph_key,
                project_name: project.name.clone(),
                task_title: task.title.clone(),
                worktree_paths: project
                    .repositories
                    .iter()
                    .map(|repo| repo.path.clone())
                    .collect(),
            },
            AttemptContextGraphSnapshotMetadata {
                version: 1,
                schema_version: manifest.schema_version.clone(),
                manifest_version: manifest.manifest_version,
                flow_id: flow.id,
                task_id,
                attempt_id,
                runtime_name: runtime_name.to_string(),
                manifest_hash: manifest_hash.to_string(),
                context_hash: context_hash.to_string(),
                ordered_inputs: manifest.ordered_inputs.clone(),
                excluded_sources: manifest.excluded_sources.clone(),
                skill_count: manifest.skills.len(),
                document_count: manifest.documents.len(),
                retry_count: manifest.retry_links.len(),
            },
        ))
    }

    pub(crate) fn sync_attempt_context_graph_into_runtime_registry(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        runtime_name: &str,
        manifest: &AttemptContextManifest,
        manifest_hash: &str,
        context_hash: &str,
        origin: &'static str,
    ) -> Result<()> {
        let (document, graph_manifest, snapshot_metadata) =
            Self::build_attempt_context_runtime_graph_document(
                state,
                flow,
                task_id,
                attempt_id,
                runtime_name,
                manifest,
                manifest_hash,
                context_hash,
                origin,
            )?;
        let store = self.open_native_runtime_state_store(origin)?;
        let storage_path = Self::attempt_context_runtime_graph_store_path(&store.db_path);
        if let Some(parent) = storage_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                HivemindError::system(
                    "attempt_context_graph_storage_failed",
                    format!(
                        "failed to create graph store directory '{}': {error}",
                        parent.display()
                    ),
                    origin,
                )
            })?;
        }
        ucp_graph::GraphNavigator::from_portable(&document)
            .and_then(|navigator| {
                navigator.persist_sqlite(&storage_path, &graph_manifest.graph_key)
            })
            .map_err(|error| {
                HivemindError::system(
                    "attempt_context_graph_storage_failed",
                    format!("failed to persist attempt-context graph document: {error}"),
                    origin,
                )
            })?;

        let registry_key = format!("{}:graph", flow.project_id);
        let session_ref = format!("{registry_key}:session:{attempt_id}");
        let repo_manifest_json = serde_json::to_string(&vec![graph_manifest]).map_err(|error| {
            HivemindError::system(
                "attempt_context_graph_encode_failed",
                error.to_string(),
                origin,
            )
        })?;
        let snapshot_json = serde_json::to_string(&snapshot_metadata).map_err(|error| {
            HivemindError::system(
                "attempt_context_graph_encode_failed",
                error.to_string(),
                origin,
            )
        })?;
        store
            .upsert_graphcode_artifact(&crate::native::runtime_hardening::GraphCodeArtifactUpsert {
                registry_key: registry_key.clone(),
                project_id: flow.project_id.to_string(),
                substrate_kind: "graph".to_string(),
                storage_backend: "ucp_graph_sqlite".to_string(),
                storage_reference: storage_path.to_string_lossy().to_string(),
                derivative_snapshot_path: None,
                constitution_path: Some(manifest.constitution.path.clone()),
                canonical_fingerprint: context_hash.to_string(),
                profile_version: format!("attempt-context-v{}", manifest.manifest_version),
                ucp_engine_version: "ucp-graph-runtime".to_string(),
                extractor_version: "ucp-graph-runtime".to_string(),
                runtime_version: runtime_name.to_string(),
                freshness_state: "fresh".to_string(),
                repo_manifest_json,
                active_session_ref: Some(session_ref.clone()),
                snapshot_json,
            })
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?;
        store
            .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
                session_ref,
                registry_key,
                substrate_kind: "graph".to_string(),
                current_focus_json: serde_json::json!({
                    "attempt_id": attempt_id,
                    "task_id": task_id,
                    "runtime_name": runtime_name,
                })
                .to_string(),
                pinned_nodes_json: serde_json::json!([
                    "project",
                    "flow",
                    "task",
                    "constitution",
                    "graph_summary",
                ])
                .to_string(),
                recent_traversals_json: serde_json::json!([
                    {"from": "task", "to": "documents"},
                    {"from": "task", "to": "skills"},
                ])
                .to_string(),
                working_set_refs_json: serde_json::json!([manifest_hash, context_hash,])
                    .to_string(),
                hydrated_excerpts_json: serde_json::json!([]).to_string(),
                path_artifacts_json: serde_json::json!([]).to_string(),
                snapshot_fingerprint: context_hash.to_string(),
                freshness_state: "fresh".to_string(),
            })
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn read_runtime_generic_graph_document(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Option<PortableDocument>> {
        let record = self
            .open_native_runtime_state_store(origin)?
            .graphcode_artifact_by_project(&project_id.to_string(), "graph")
            .map_err(|error| HivemindError::system(error.code, error.message, origin))?;
        let Some(record) = record else {
            return Ok(None);
        };
        let graph_manifest = serde_json::from_str::<Vec<AttemptContextGraphStorageManifest>>(
            &record.repo_manifest_json,
        )
        .map_err(|error| {
            HivemindError::system(
                "attempt_context_graph_decode_failed",
                format!("failed to decode attempt-context graph manifest: {error}"),
                origin,
            )
        })?;
        let Some(entry) = graph_manifest.first() else {
            return Ok(None);
        };
        let document =
            ucp_graph::GraphNavigator::open_sqlite(&record.storage_reference, &entry.graph_key)
                .and_then(|navigator| navigator.to_portable_document())
                .map_err(|error| {
                    HivemindError::system(
                        "attempt_context_graph_decode_failed",
                        format!("failed to load attempt-context graph document: {error}"),
                        origin,
                    )
                })?;
        Ok(Some(document))
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn build_attempt_context_sources(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        prior_attempt_ids: &[Uuid],
        origin: &'static str,
    ) -> Result<AttemptContextSourceData> {
        let constitution_location = self.governance_constitution_location(flow.project_id);
        let constitution_raw = fs::read_to_string(&constitution_location.path).unwrap_or_default();
        let constitution_path = constitution_location.path.to_string_lossy().to_string();
        let constitution_revision = Self::governance_artifact_revision(
            state,
            Some(flow.project_id),
            "project",
            "constitution",
            "constitution.yaml",
        );
        let constitution_digest = state
            .projects
            .get(&flow.project_id)
            .and_then(|project| project.constitution_digest.clone())
            .or_else(|| {
                if constitution_raw.trim().is_empty() {
                    None
                } else {
                    Some(Self::constitution_digest(constitution_raw.as_bytes()))
                }
            });
        let constitution_content_hash = if constitution_raw.trim().is_empty() {
            None
        } else {
            Some(Self::constitution_digest(constitution_raw.as_bytes()))
        };
        let constitution_section = if constitution_raw.trim().is_empty() {
            "Constitution:\n- status: missing".to_string()
        } else {
            format!(
                "Constitution:\n- path: {}\n- digest: {}\n- content:\n{}",
                constitution_path,
                constitution_digest.clone().unwrap_or_default(),
                constitution_raw
            )
        };

        let template_snapshot = self.latest_template_instantiation_snapshot(flow.project_id)?;
        let template_document_ids = template_snapshot
            .as_ref()
            .map_or_else(Vec::new, |item| item.document_ids.clone());
        let (attachment_included, attachment_excluded) =
            Self::governance_document_attachment_states(state, flow.project_id, task_id);
        let template_set: HashSet<&str> =
            template_document_ids.iter().map(String::as_str).collect();

        let mut included_document_ids: Vec<String> = attachment_included
            .iter()
            .filter(|id| !template_set.contains(id.as_str()))
            .cloned()
            .collect();
        included_document_ids.sort();
        included_document_ids.dedup();

        let mut excluded_document_ids = attachment_excluded;
        excluded_document_ids.sort();
        excluded_document_ids.dedup();

        let mut resolved_document_ids = template_document_ids.clone();
        for doc_id in &included_document_ids {
            if !resolved_document_ids.iter().any(|item| item == doc_id) {
                resolved_document_ids.push(doc_id.clone());
            }
        }
        resolved_document_ids
            .retain(|id| !excluded_document_ids.iter().any(|excluded| excluded == id));
        resolved_document_ids.sort();
        resolved_document_ids.dedup();

        let mut document_manifest = Vec::new();
        let mut document_sections = Vec::new();
        for document_id in &resolved_document_ids {
            let artifact =
                self.read_project_document_artifact(flow.project_id, document_id, origin)?;
            let latest = artifact.revisions.last().ok_or_else(|| {
                HivemindError::user(
                    "governance_artifact_schema_invalid",
                    format!("Document '{document_id}' has no latest revision"),
                    origin,
                )
            })?;
            let source = if template_set.contains(document_id.as_str()) {
                "template"
            } else {
                "attachment_include"
            };
            document_manifest.push(AttemptContextDocumentManifest {
                document_id: document_id.clone(),
                source: source.to_string(),
                revision: latest.revision,
                content_hash: Self::constitution_digest(latest.content.as_bytes()),
            });
            document_sections.push(format!(
                "- document_id: {document_id}\n  source: {source}\n  revision: {}\n  title: {}\n  owner: {}\n  content:\n{}",
                latest.revision,
                artifact.title,
                artifact.owner,
                latest.content
            ));
        }

        let mut skill_manifest = Vec::new();
        let mut skill_sections = Vec::new();
        let mut system_prompt_manifest = None;
        let mut system_prompt_section = "System Prompt:\n- none selected".to_string();
        let mut template_id = None;
        let mut template_event_id = None;
        let mut template_schema_version = None;
        let mut template_projection_version = None;
        if let Some(template) = template_snapshot.as_ref() {
            template_id = Some(template.template_id.clone());
            template_event_id = Some(template.event_id.clone());
            template_schema_version = Some(template.schema_version.clone());
            template_projection_version = Some(template.projection_version);

            let prompt =
                self.read_global_system_prompt_artifact(&template.system_prompt_id, origin)?;
            system_prompt_manifest = Some(AttemptContextSystemPromptManifest {
                prompt_id: prompt.prompt_id.clone(),
                content_hash: Self::constitution_digest(prompt.content.as_bytes()),
            });
            system_prompt_section = format!(
                "System Prompt:\n- prompt_id: {}\n- content:\n{}",
                prompt.prompt_id, prompt.content
            );

            for skill_id in &template.skill_ids {
                let skill = self.read_global_skill_artifact(skill_id, origin)?;
                skill_manifest.push(AttemptContextSkillManifest {
                    skill_id: skill.skill_id.clone(),
                    content_hash: Self::constitution_digest(skill.content.as_bytes()),
                });
                skill_sections.push(format!(
                    "- skill_id: {}\n  name: {}\n  content:\n{}",
                    skill.skill_id, skill.name, skill.content
                ));
            }
        }

        let graph_artifact = self.read_graph_snapshot_artifact(flow.project_id, origin)?;
        let graph_summary = graph_artifact.as_ref().map_or_else(
            || AttemptContextGraphManifest {
                present: false,
                canonical_fingerprint: None,
                repository_count: 0,
                total_nodes: 0,
                total_edges: 0,
                languages: BTreeMap::new(),
            },
            |item| AttemptContextGraphManifest {
                present: true,
                canonical_fingerprint: Some(item.canonical_fingerprint.clone()),
                repository_count: item.repositories.len(),
                total_nodes: item.summary.total_nodes,
                total_edges: item.summary.total_edges,
                languages: item.summary.languages.clone(),
            },
        );
        let graph_section = if graph_summary.present {
            let fingerprint = graph_summary
                .canonical_fingerprint
                .clone()
                .unwrap_or_default();
            let languages = if graph_summary.languages.is_empty() {
                "(none)".to_string()
            } else {
                graph_summary
                    .languages
                    .iter()
                    .map(|(lang, count)| format!("{lang}:{count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "Graph Summary:\n- canonical_fingerprint: {fingerprint}\n- repositories: {}\n- total_nodes: {}\n- total_edges: {}\n- languages: {languages}",
                graph_summary.repository_count,
                graph_summary.total_nodes,
                graph_summary.total_edges
            )
        } else {
            "Graph Summary:\n- status: unavailable".to_string()
        };

        let mut retry_links = Vec::new();
        let mut prior_manifest_hashes = Vec::new();
        for prior_id in prior_attempt_ids {
            let manifest_hash = self.attempt_manifest_hash_for_attempt(*prior_id)?;
            if let Some(hash) = manifest_hash.as_ref() {
                prior_manifest_hashes.push(hash.clone());
            }
            retry_links.push(AttemptContextRetryLink {
                attempt_id: *prior_id,
                manifest_hash,
            });
        }

        Ok(AttemptContextSourceData {
            constitution_path,
            constitution_revision,
            constitution_digest,
            constitution_content_hash,
            constitution_section,
            template_id,
            template_event_id,
            template_schema_version,
            template_projection_version,
            system_prompt_manifest,
            system_prompt_section,
            skill_manifest,
            skill_sections,
            document_manifest,
            document_sections,
            workflow_manifest: None,
            workflow_section: None,
            graph_summary,
            graph_section,
            retry_links,
            prior_manifest_hashes,
            template_document_ids,
            included_document_ids,
            excluded_document_ids,
            resolved_document_ids,
        })
    }

    pub(crate) fn finalize_attempt_context_manifest(
        manifest: &AttemptContextManifest,
        ordered_inputs: &[String],
        prior_manifest_hashes: &[String],
        context_window_state_hash: &str,
        context: &str,
        origin: &'static str,
    ) -> Result<(String, String, String, String)> {
        let manifest_json = serde_json::to_string(manifest).map_err(|e| {
            HivemindError::system("context_manifest_serialize_failed", e.to_string(), origin)
        })?;
        let manifest_hash = Self::constitution_digest(manifest_json.as_bytes());
        let inputs_fingerprint_payload = serde_json::json!({
            "ordered_inputs": ordered_inputs,
            "constitution": manifest.constitution,
            "template_id": manifest.template_id,
            "template_schema_version": manifest.template_schema_version,
            "template_projection_version": manifest.template_projection_version,
            "system_prompt": manifest.system_prompt,
            "skills": manifest.skills,
            "documents": manifest.documents,
            "workflow": manifest.workflow,
            "graph_summary": manifest.graph_summary,
            "retry_manifest_hashes": prior_manifest_hashes,
            "budget": {
                "total_budget_bytes": ATTEMPT_CONTEXT_TOTAL_BUDGET_BYTES,
                "default_section_budget_bytes": ATTEMPT_CONTEXT_SECTION_BUDGET_BYTES,
                "per_section_budget_bytes": manifest.budget.per_section_budget_bytes,
                "max_expand_depth": ATTEMPT_CONTEXT_MAX_EXPAND_DEPTH,
                "deduplicate": true,
                "truncated_sections": manifest.budget.truncated_sections,
                "truncation_reasons": manifest.budget.truncation_reasons,
                "policy": ATTEMPT_CONTEXT_TRUNCATION_POLICY
            },
            "context_window": {
                "state_hash": context_window_state_hash
            },
        });
        let inputs_bytes = serde_json::to_vec(&inputs_fingerprint_payload).map_err(|e| {
            HivemindError::system("context_inputs_serialize_failed", e.to_string(), origin)
        })?;
        let inputs_hash = Self::constitution_digest(&inputs_bytes);
        let context_hash = Self::constitution_digest(context.as_bytes());
        Ok((manifest_json, manifest_hash, inputs_hash, context_hash))
    }
}
