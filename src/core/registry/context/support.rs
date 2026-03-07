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
    pub(crate) graph_summary: AttemptContextGraphManifest,
    pub(crate) graph_section: String,
    pub(crate) retry_links: Vec<AttemptContextRetryLink>,
    pub(crate) prior_manifest_hashes: Vec<String>,
    pub(crate) template_document_ids: Vec<String>,
    pub(crate) included_document_ids: Vec<String>,
    pub(crate) excluded_document_ids: Vec<String>,
    pub(crate) resolved_document_ids: Vec<String>,
}

impl Registry {
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
