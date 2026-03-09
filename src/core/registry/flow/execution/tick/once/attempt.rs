#![allow(clippy::too_many_lines)]

use super::*;
use crate::adapters::runtime::NativePromptMetadata;

pub(super) struct TickAttemptLaunch {
    pub(super) attempt_id: Uuid,
    pub(super) attempt_corr: CorrelationIds,
    pub(super) input: ExecutionInput,
    pub(super) runtime_prompt: String,
    pub(super) runtime_flags: Vec<String>,
    pub(super) task_scope: Option<Scope>,
    pub(super) max_attempts: u32,
}

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_tick_attempt(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        graph: &TaskGraph,
        task_id: Uuid,
        runtime: &ProjectRuntimeConfig,
        repo_worktrees: &[(String, WorktreeStatus)],
        next_attempt_number: u32,
        origin: &'static str,
    ) -> Result<TickAttemptLaunch> {
        let attempt_id = self.start_task_execution(&task_id.to_string())?;
        let attempt_corr = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        let task = graph.tasks.get(&task_id).ok_or_else(|| {
            HivemindError::system("task_not_found", "Task not found in graph", origin)
        })?;

        let max_attempts = task.retry_policy.max_retries.saturating_add(1);
        let checkpoint_ids = Self::normalized_checkpoint_ids(&task.checkpoints);
        let checkpoint_help = if checkpoint_ids.is_empty() {
            None
        } else {
            Some(format!(
                "Execution checkpoints (in order): {}\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}}\nAttempt ID for this run: {attempt_id}",
                checkpoint_ids.join(", ")
            ))
        };
        let repo_context = format!(
            "Multi-repo worktrees for this attempt:\n{}",
            repo_worktrees
                .iter()
                .map(|(name, wt)| format!("- {name}: {}", wt.path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let mut retry_prior_attempt_ids = Vec::new();
        let (retry_context_text, prior_attempts) = if next_attempt_number > 1 {
            let (ctx, priors, ids, req, opt, ec, term) = self.build_retry_context(
                state,
                flow,
                task_id,
                next_attempt_number,
                max_attempts,
                origin,
            )?;
            retry_prior_attempt_ids.clone_from(&ids);

            self.append_event(
                Event::new(
                    EventPayload::RetryContextAssembled {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        attempt_number: next_attempt_number,
                        max_attempts,
                        prior_attempt_ids: ids,
                        required_check_failures: req,
                        optional_check_failures: opt,
                        runtime_exit_code: ec,
                        runtime_terminated_reason: term,
                        context: ctx.clone(),
                    },
                    attempt_corr.clone(),
                ),
                origin,
            )?;

            (Some(ctx), priors)
        } else {
            (None, Vec::new())
        };

        let context_build = self.assemble_attempt_context(
            state,
            flow,
            task_id,
            attempt_id,
            &runtime.adapter_name,
            &retry_prior_attempt_ids,
            origin,
        )?;

        if !context_build.included_document_ids.is_empty()
            || !context_build.excluded_document_ids.is_empty()
        {
            self.append_event(
                Event::new(
                    EventPayload::AttemptContextOverridesApplied {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        template_document_ids: context_build.template_document_ids.clone(),
                        included_document_ids: context_build.included_document_ids.clone(),
                        excluded_document_ids: context_build.excluded_document_ids.clone(),
                        resolved_document_ids: context_build.resolved_document_ids.clone(),
                    },
                    attempt_corr.clone(),
                ),
                origin,
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::ContextWindowCreated {
                    flow_id: flow.id,
                    task_id,
                    attempt_id,
                    window_id: context_build.context_window_id.clone(),
                    policy: ATTEMPT_CONTEXT_TRUNCATION_POLICY.to_string(),
                    state_hash: context_build.context_window_state_hash.clone(),
                },
                attempt_corr.clone(),
            ),
            origin,
        )?;

        for op in &context_build.context_window_ops {
            self.append_event(
                Event::new(
                    EventPayload::ContextOpApplied {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        window_id: context_build.context_window_id.clone(),
                        op: op.op.clone(),
                        actor: op.actor.actor.clone(),
                        runtime: op.actor.runtime.clone(),
                        tool: op.actor.tool.clone(),
                        reason: op.reason.clone(),
                        before_hash: op.before_hash.clone(),
                        after_hash: op.after_hash.clone(),
                        added_ids: op.added_ids.clone(),
                        removed_ids: op.removed_ids.clone(),
                        truncated_sections: op.truncated_sections.clone(),
                        section_reasons: op.section_reasons.clone(),
                    },
                    attempt_corr.clone(),
                ),
                origin,
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::AttemptContextAssembled {
                    flow_id: flow.id,
                    task_id,
                    attempt_id,
                    attempt_number: next_attempt_number,
                    manifest_hash: context_build.manifest_hash.clone(),
                    inputs_hash: context_build.inputs_hash.clone(),
                    context_hash: context_build.context_hash.clone(),
                    context_size_bytes: context_build.context_size_bytes,
                    truncated_sections: context_build.truncated_sections.clone(),
                    manifest_json: context_build.manifest_json.clone(),
                },
                attempt_corr.clone(),
            ),
            origin,
        )?;

        if !context_build.truncated_sections.is_empty() {
            self.append_event(
                Event::new(
                    EventPayload::AttemptContextTruncated {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        budget_bytes: ATTEMPT_CONTEXT_TOTAL_BUDGET_BYTES,
                        original_size_bytes: context_build.original_size_bytes,
                        truncated_size_bytes: context_build.context_size_bytes,
                        sections: context_build.truncated_sections.clone(),
                        section_reasons: context_build.truncation_reasons.clone(),
                        policy: ATTEMPT_CONTEXT_TRUNCATION_POLICY.to_string(),
                    },
                    attempt_corr.clone(),
                ),
                origin,
            )?;
        }

        let execution_context_base = format!("{repo_context}\n\n{}", context_build.context);
        let runtime_context = match (&retry_context_text, &checkpoint_help) {
            (Some(retry), Some(checkpoint_text)) => {
                format!("{retry}\n\n{execution_context_base}\n\n{checkpoint_text}")
            }
            (Some(retry), None) => format!("{retry}\n\n{execution_context_base}"),
            (None, Some(checkpoint_text)) => {
                format!("{execution_context_base}\n\n{checkpoint_text}")
            }
            (None, None) => execution_context_base,
        };
        let runtime_context_hash = Self::constitution_digest(runtime_context.as_bytes());

        self.append_event(
            Event::new(
                EventPayload::ContextWindowSnapshotCreated {
                    flow_id: flow.id,
                    task_id,
                    attempt_id,
                    window_id: context_build.context_window_id.clone(),
                    state_hash: context_build.context_window_state_hash.clone(),
                    rendered_prompt_hash: context_build.context_hash.clone(),
                    delivered_input_hash: runtime_context_hash.clone(),
                    snapshot_json: context_build.context_window_snapshot_json.clone(),
                },
                attempt_corr.clone(),
            ),
            origin,
        )?;

        let manifest_hash = context_build.manifest_hash.clone();
        let inputs_hash = context_build.inputs_hash.clone();
        let delivery_target = "runtime_execution_input".to_string();

        self.append_event(
            Event::new(
                EventPayload::AttemptContextDelivered {
                    flow_id: flow.id,
                    task_id,
                    attempt_id,
                    manifest_hash: manifest_hash.clone(),
                    inputs_hash: inputs_hash.clone(),
                    context_hash: runtime_context_hash.clone(),
                    delivery_target: delivery_target.clone(),
                    prior_attempt_ids: retry_prior_attempt_ids,
                    prior_manifest_hashes: context_build.prior_manifest_hashes,
                },
                attempt_corr.clone(),
            ),
            origin,
        )?;

        let input = ExecutionInput {
            task_description: task
                .description
                .clone()
                .unwrap_or_else(|| task.title.clone()),
            success_criteria: task.criteria.description.clone(),
            context: Some(runtime_context.clone()),
            prior_attempts,
            verifier_feedback: None,
            native_prompt_metadata: Some(NativePromptMetadata {
                manifest_hash: Some(manifest_hash),
                inputs_hash: Some(inputs_hash),
                delivered_context_hash: Some(runtime_context_hash.to_string()),
                rendered_context_hash: Some(runtime_context_hash.to_string()),
                context_window_state_hash: None,
                delivery_target: Some(delivery_target),
                runtime_context_bytes: runtime_context.len(),
            }),
        };

        Ok(TickAttemptLaunch {
            attempt_id,
            attempt_corr,
            runtime_prompt: format_execution_prompt(&input),
            runtime_flags: Self::runtime_start_flags(runtime),
            task_scope: task.scope.clone(),
            input,
            max_attempts,
        })
    }
}
