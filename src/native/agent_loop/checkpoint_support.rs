use super::*;

impl<M: ModelClient> AgentLoop<M> {
    pub(crate) fn checkpoint_completion_recorded(&self, history: &[TurnItem]) -> bool {
        history.iter().any(|item| matches!(&item.kind, TurnItemKind::ToolResult { tool_name, outcome: TurnItemOutcome::Success, .. } if tool_name == "checkpoint_complete")) || self.completed_turns.iter().any(|turn| turn.tool_calls.iter().any(|trace| trace.tool_name == "checkpoint_complete" && trace.failure.is_none()))
    }

    pub(crate) fn synthetic_checkpoint_completion_already_satisfied(
        calls: &[NativeToolCallTrace],
    ) -> bool {
        !calls.is_empty()
            && calls.iter().all(|trace| {
                trace.tool_name == "checkpoint_complete"
                    && trace.failure.as_ref().is_some_and(|failure| {
                        failure.code == "checkpoint_already_completed"
                            || failure.message.contains("checkpoint_already_completed")
                            || failure.message.contains("already completed")
                    })
            })
    }

    pub(crate) fn checkpoint_id_from_response_payload(content: &str) -> Option<String> {
        let value = serde_json::from_str::<Value>(content).ok()?;
        value
            .get("checkpoint_id")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .get("output")
                    .and_then(|output| output.get("checkpoint_id"))
                    .and_then(Value::as_str)
            })
            .map(ToString::to_string)
    }

    pub(crate) fn completed_checkpoint_ids(
        &self,
        history: &[TurnItem],
    ) -> std::collections::BTreeSet<String> {
        let mut ids = std::collections::BTreeSet::new();
        for item in history {
            if let TurnItemKind::ToolResult {
                tool_name,
                outcome: TurnItemOutcome::Success,
                content,
                ..
            } = &item.kind
            {
                if tool_name == "checkpoint_complete" {
                    if let Some(id) = Self::checkpoint_id_from_response_payload(content) {
                        ids.insert(id);
                    }
                }
            }
        }
        for turn in &self.completed_turns {
            for trace in &turn.tool_calls {
                if trace.tool_name == "checkpoint_complete" && trace.failure.is_none() {
                    if let Some(id) = trace
                        .response
                        .as_deref()
                        .and_then(Self::checkpoint_id_from_response_payload)
                    {
                        ids.insert(id);
                    }
                }
            }
        }
        ids
    }

    pub(crate) fn checkpoint_summary_from_request_payload(content: &str) -> Option<String> {
        let value = serde_json::from_str::<Value>(content).ok()?;
        value
            .get("input")
            .and_then(|input| input.get("summary"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
            .map(ToString::to_string)
    }

    pub(crate) fn latest_completed_checkpoint_summary(&self) -> Option<String> {
        self.completed_turns.iter().rev().find_map(|turn| {
            turn.tool_calls.iter().rev().find_map(|trace| {
                (trace.tool_name == "checkpoint_complete" && trace.failure.is_none())
                    .then_some(trace.request.as_str())
                    .and_then(Self::checkpoint_summary_from_request_payload)
            })
        })
    }

    pub(crate) fn requires_checkpoint_completion_repair(
        &self,
        request: &ModelTurnRequest,
        history: &[TurnItem],
        directive: &ModelDirective,
    ) -> bool {
        matches!(directive, ModelDirective::Done { .. })
            && request
                .prompt_assembly
                .as_ref()
                .is_some_and(|assembly| assembly.objective_state.starts_with("checkpoint"))
            && !self.checkpoint_completion_recorded(history)
    }

    pub(crate) fn checkpoint_done_repair_item(
        invocation_id: &str,
        turn_index: u32,
        repair_attempt: u8,
        raw_output: &str,
    ) -> TurnItem {
        user_input_item(invocation_id, turn_index.saturating_mul(100).saturating_add(95).saturating_add(u32::from(repair_attempt)), "controller_repair", format!("Your previous response returned DONE before the active execution checkpoint was completed. Before DONE, call the built-in checkpoint tool exactly as instructed in the prompt, e.g. ACT:tool:checkpoint_complete:{{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}}. After the checkpoint tool succeeds, continue and return DONE only if the task is truly complete. Previous response:\n{raw_output}"), "runtime.repair")
    }

    pub(crate) fn declared_checkpoint_ids(request: &ModelTurnRequest) -> Vec<String> {
        const PREFIX: &str = "Execution checkpoints (in order):";
        [request.context.as_deref(), Some(request.prompt.as_str())]
            .into_iter()
            .flatten()
            .find_map(|text| {
                text.lines().find_map(|line| {
                    line.trim().strip_prefix(PREFIX).map(|rest| {
                        rest.split(',')
                            .map(str::trim)
                            .filter(|id| !id.is_empty())
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    })
                })
            })
            .unwrap_or_default()
    }

    pub(crate) fn first_declared_checkpoint_id(request: &ModelTurnRequest) -> Option<String> {
        Self::declared_checkpoint_ids(request).into_iter().next()
    }

    pub(crate) fn checkpoint_completion_action_payload(
        directive: &ModelDirective,
    ) -> Option<(String, Option<String>)> {
        let ModelDirective::Act { action } = directive else {
            return None;
        };
        let tool_action = NativeToolAction::parse(action).ok().flatten()?;
        if tool_action.name != "checkpoint_complete" {
            return None;
        }
        let checkpoint_id = tool_action.input.get("id")?.as_str()?.trim();
        if checkpoint_id.is_empty() {
            return None;
        }
        let summary = tool_action
            .input
            .get("summary")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        Some((checkpoint_id.to_string(), summary))
    }

    pub(crate) fn redundant_checkpoint_completion_done_summary(
        &self,
        request: &ModelTurnRequest,
        history: &[TurnItem],
        directive: &ModelDirective,
    ) -> Option<String> {
        let declared = Self::declared_checkpoint_ids(request);
        if declared.is_empty() {
            return None;
        }
        let completed = self.completed_checkpoint_ids(history);
        if !declared.iter().all(|id| completed.contains(id)) {
            return None;
        }
        let (checkpoint_id, summary) = Self::checkpoint_completion_action_payload(directive)?;
        if !completed.contains(&checkpoint_id) {
            return None;
        }
        Some(summary.unwrap_or_else(|| format!("completed checkpoints: {}", declared.join(", "))))
    }

    pub(crate) fn all_declared_checkpoints_completed(
        &self,
        request: &ModelTurnRequest,
        history: &[TurnItem],
    ) -> bool {
        let declared = Self::declared_checkpoint_ids(request);
        if declared.is_empty() {
            return false;
        }
        let completed = self.completed_checkpoint_ids(history);
        declared.iter().all(|id| completed.contains(id))
    }

    pub(crate) fn post_checkpoint_done_repair_item(
        invocation_id: &str,
        turn_index: u32,
        repair_attempt: u8,
    ) -> TurnItem {
        user_input_item(invocation_id, turn_index.saturating_mul(100).saturating_add(98).saturating_add(u32::from(repair_attempt)), "controller_repair", format!("Runtime repair #{repair_attempt}: all declared execution checkpoints are complete. Return DONE now with a concise summary. Do not call more tools unless essential to fix a newly discovered failure."), "runtime.repair")
    }

    pub(crate) fn checkpoint_auto_completion_action(
        request: &ModelTurnRequest,
        directive: &ModelDirective,
    ) -> Option<String> {
        let ModelDirective::Done { summary } = directive else {
            return None;
        };
        let checkpoint_id = Self::first_declared_checkpoint_id(request)?;
        let summary = summary.trim();
        let payload = if summary.is_empty() {
            serde_json::json!({ "id": checkpoint_id })
        } else {
            serde_json::json!({ "id": checkpoint_id, "summary": summary })
        };
        Some(format!("tool:checkpoint_complete:{payload}"))
    }
}
