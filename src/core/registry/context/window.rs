use super::*;

pub(crate) struct AttemptContextWindowBuildResult {
    pub(crate) context_window_id: String,
    pub(crate) context_window_ops: Vec<ContextOpRecord>,
    pub(crate) context: String,
    pub(crate) context_size_bytes: usize,
    pub(crate) original_size_bytes: usize,
    pub(crate) truncated_sections: Vec<String>,
    pub(crate) truncation_reasons: BTreeMap<String, Vec<String>>,
    pub(crate) context_window_state_hash: String,
    pub(crate) context_window_snapshot_json: String,
}

impl Registry {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn build_attempt_context_window(
        attempt_id: Uuid,
        runtime_name: &str,
        ordered_inputs: &[String],
        budget_policy: ContextBudgetPolicy,
        constitution_section: String,
        system_prompt_section: String,
        skill_sections: &[String],
        document_sections: &[String],
        workflow_section: Option<String>,
        graph_section: String,
        origin: &'static str,
    ) -> Result<AttemptContextWindowBuildResult> {
        let context_window_id = format!("attempt-context-{attempt_id}");
        let mut window = ContextWindow::new(
            context_window_id.clone(),
            ordered_inputs.to_vec(),
            budget_policy,
        );
        let window_actor = ContextOperationActor {
            actor: "flow_engine".to_string(),
            runtime: Some(runtime_name.to_string()),
            tool: None,
        };
        let mut context_window_ops = Vec::new();
        context_window_ops.push(window.add_entry(
            ContextEntryCandidate {
                entry_id: "constitution-primary".to_string(),
                section: "constitution".to_string(),
                content: constitution_section,
                source: "governance_constitution".to_string(),
                depth: 0,
            },
            "seed_constitution",
            window_actor.clone(),
        ));
        context_window_ops.push(window.add_entry(
            ContextEntryCandidate {
                entry_id: "system-prompt-primary".to_string(),
                section: "system_prompt".to_string(),
                content: system_prompt_section,
                source: "template_system_prompt".to_string(),
                depth: 0,
            },
            "seed_system_prompt",
            window_actor.clone(),
        ));
        if skill_sections.is_empty() {
            context_window_ops.push(window.add_entry(
                ContextEntryCandidate {
                    entry_id: "skills-none".to_string(),
                    section: "skills".to_string(),
                    content: "Skills:\n- none selected".to_string(),
                    source: "template_skills".to_string(),
                    depth: 0,
                },
                "seed_skills_empty",
                window_actor.clone(),
            ));
        } else {
            context_window_ops.push(window.add_entry(
                ContextEntryCandidate {
                    entry_id: "skills-header".to_string(),
                    section: "skills".to_string(),
                    content: "Skills:".to_string(),
                    source: "template_skills".to_string(),
                    depth: 0,
                },
                "seed_skills_header",
                window_actor.clone(),
            ));
            let skill_candidates: Vec<ContextEntryCandidate> = skill_sections
                .iter()
                .enumerate()
                .map(|(idx, content)| ContextEntryCandidate {
                    entry_id: format!("skills-entry-{idx:03}"),
                    section: "skills".to_string(),
                    content: content.clone(),
                    source: "template_skills".to_string(),
                    depth: 1,
                })
                .collect();
            context_window_ops.push(window.expand_entries(
                skill_candidates,
                "expand_skills",
                window_actor.clone(),
                true,
            ));
        }
        if document_sections.is_empty() {
            context_window_ops.push(window.add_entry(
                ContextEntryCandidate {
                    entry_id: "documents-none".to_string(),
                    section: "project_documents".to_string(),
                    content: "Documents:\n- none selected".to_string(),
                    source: "project_documents".to_string(),
                    depth: 0,
                },
                "seed_documents_empty",
                window_actor.clone(),
            ));
        } else {
            context_window_ops.push(window.add_entry(
                ContextEntryCandidate {
                    entry_id: "documents-header".to_string(),
                    section: "project_documents".to_string(),
                    content: "Documents:".to_string(),
                    source: "project_documents".to_string(),
                    depth: 0,
                },
                "seed_documents_header",
                window_actor.clone(),
            ));
            let document_candidates: Vec<ContextEntryCandidate> = document_sections
                .iter()
                .enumerate()
                .map(|(idx, content)| ContextEntryCandidate {
                    entry_id: format!("documents-entry-{idx:03}"),
                    section: "project_documents".to_string(),
                    content: content.clone(),
                    source: "project_documents".to_string(),
                    depth: 1,
                })
                .collect();
            context_window_ops.push(window.expand_entries(
                document_candidates,
                "expand_documents",
                window_actor.clone(),
                true,
            ));
        }
        if let Some(workflow_section) = workflow_section {
            context_window_ops.push(window.add_entry(
                ContextEntryCandidate {
                    entry_id: "workflow-step-primary".to_string(),
                    section: "workflow_step".to_string(),
                    content: workflow_section,
                    source: "workflow_runtime".to_string(),
                    depth: 0,
                },
                "seed_workflow_step",
                window_actor.clone(),
            ));
        }
        context_window_ops.push(window.add_entry(
            ContextEntryCandidate {
                entry_id: "graph-summary-primary".to_string(),
                section: "graph_summary".to_string(),
                content: graph_section,
                source: "graph_snapshot".to_string(),
                depth: 0,
            },
            "seed_graph_summary",
            window_actor.clone(),
        ));

        let original_size_bytes: usize =
            window.entries.values().map(ContextEntry::size_bytes).sum();
        let prune_op = window.prune("enforce_budget_policy", window_actor.clone());
        context_window_ops.push(prune_op.clone());
        let (snapshot, snapshot_op) = window.snapshot("create_context_snapshot", window_actor);
        context_window_ops.push(snapshot_op);

        let context_window_snapshot_json = serde_json::to_string(&snapshot).map_err(|e| {
            HivemindError::system(
                "context_window_snapshot_serialize_failed",
                e.to_string(),
                origin,
            )
        })?;

        Ok(AttemptContextWindowBuildResult {
            context_window_id,
            context_window_ops,
            context: snapshot.rendered_prompt.clone(),
            context_size_bytes: snapshot.total_size_bytes,
            original_size_bytes,
            truncated_sections: prune_op.truncated_sections.clone(),
            truncation_reasons: prune_op.section_reasons,
            context_window_state_hash: snapshot.state_hash,
            context_window_snapshot_json,
        })
    }
}
