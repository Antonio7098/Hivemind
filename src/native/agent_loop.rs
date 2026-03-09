use super::*;
use crate::native::tool_engine::NativeToolAction;
use crate::native::turn_items::{TurnItemKind, TurnItemOutcome};
use serde_json::Value;

impl<M: ModelClient> AgentLoop<M> {
    const MAX_MALFORMED_DIRECTIVE_RECOVERY_ATTEMPTS: u8 = 4;
    const MAX_TOKEN_BUDGET_RECOVERY_ATTEMPTS: u8 = 3;
    const MAX_CHECKPOINT_DONE_RECOVERY_ATTEMPTS: u8 = 3;
    const MAX_POST_CHECKPOINT_DONE_RECOVERY_ATTEMPTS: u8 = 1;
    const SOFT_TOKEN_BUDGET_COMPACTION_PERCENT: usize = 60;

    #[must_use]
    pub fn new(config: NativeRuntimeConfig, model_client: M) -> Self {
        Self {
            config,
            model_client,
            state: AgentLoopState::Init,
            started_at: Instant::now(),
            next_turn_index: 0,
            used_tokens: 0,
            emitted_budget_thresholds: Vec::new(),
            initial_items: Vec::new(),
            history_items: Vec::new(),
            completed_turns: Vec::new(),
        }
    }

    #[must_use]
    pub const fn state(&self) -> AgentLoopState {
        self.state
    }

    #[must_use]
    pub fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        self.model_client.take_transport_telemetry()
    }

    #[must_use]
    pub fn snapshot_result(&self, invocation_id: &str) -> AgentLoopResult {
        let final_summary = self.completed_turns.iter().rev().find_map(|turn| {
            if let ModelDirective::Done { summary } = &turn.directive {
                Some(summary.clone())
            } else {
                None
            }
        });

        AgentLoopResult {
            invocation_id: invocation_id.to_string(),
            final_state: self.state,
            final_summary,
            initial_items: self.initial_items.clone(),
            history_items: self.history_items.clone(),
            turns: self.completed_turns.clone(),
        }
    }

    fn transition_to(&mut self, to: AgentLoopState) -> Result<(), NativeRuntimeError> {
        let allowed = matches!(
            (self.state, to),
            (
                AgentLoopState::Init | AgentLoopState::Think | AgentLoopState::Act,
                AgentLoopState::Think
            ) | (
                AgentLoopState::Think | AgentLoopState::Act,
                AgentLoopState::Act
            ) | (
                AgentLoopState::Think | AgentLoopState::Act | AgentLoopState::Done,
                AgentLoopState::Done
            )
        );
        if !allowed {
            return Err(NativeRuntimeError::InvalidTransition {
                from: self.state,
                to,
            });
        }
        self.state = to;
        Ok(())
    }

    fn enforce_budgets(&self) -> Result<(), NativeRuntimeError> {
        if self.next_turn_index >= self.config.max_turns {
            return Err(NativeRuntimeError::TurnBudgetExceeded {
                max_turns: self.config.max_turns,
            });
        }

        if self.started_at.elapsed() > self.config.timeout_budget {
            let budget_ms = u64::try_from(
                self.config
                    .timeout_budget
                    .as_millis()
                    .min(u128::from(u64::MAX)),
            )
            .unwrap_or(u64::MAX);
            return Err(NativeRuntimeError::TimeoutBudgetExceeded { budget_ms });
        }

        if self.used_tokens > self.config.token_budget {
            return Err(NativeRuntimeError::TokenBudgetExceeded {
                budget: self.config.token_budget,
                used: self.used_tokens,
            });
        }

        Ok(())
    }

    fn parse_directive(raw: &str) -> Result<ModelDirective, NativeRuntimeError> {
        let raw = raw.trim();
        if let Some(directive) = ModelDirective::parse_relaxed(raw) {
            return Ok(directive);
        }
        Err(NativeRuntimeError::MalformedModelOutput {
            raw_output: raw.to_string(),
            expected: "THINK:<message> | ACT:<action> | DONE:<summary>".to_string(),
            recovery_hint: "Return one explicit directive with a known prefix (THINK/ACT/DONE)"
                .to_string(),
        })
    }

    fn malformed_output_repair_item(
        invocation_id: &str,
        turn_index: u32,
        repair_attempt: u8,
        raw_output: &str,
    ) -> TurnItem {
        let raw_output = if raw_output.chars().count() > 600 {
            let mut truncated = raw_output.chars().take(600).collect::<String>();
            truncated.push_str(" …");
            truncated
        } else {
            raw_output.to_string()
        };
        user_input_item(
            invocation_id,
            turn_index
                .saturating_mul(100)
                .saturating_add(90)
                .saturating_add(u32::from(repair_attempt)),
            "controller_repair",
            format!(
                "Your previous response could not be parsed as one native directive. Return exactly one line beginning with THINK:, ACT:, or DONE:. Do not include prose before the directive. If acting, use ACT:tool:<name>:<json_object>. Previous response:\n{raw_output}"
            ),
            "runtime.repair",
        )
    }

    fn checkpoint_completion_recorded(&self, history: &[TurnItem]) -> bool {
        history.iter().any(|item| {
            matches!(
                &item.kind,
                TurnItemKind::ToolResult {
                    tool_name,
                    outcome: TurnItemOutcome::Success,
                    ..
                } if tool_name == "checkpoint_complete"
            )
        }) || self.completed_turns.iter().any(|turn| {
            turn.tool_calls
                .iter()
                .any(|trace| trace.tool_name == "checkpoint_complete" && trace.failure.is_none())
        })
    }

    fn synthetic_checkpoint_completion_already_satisfied(calls: &[NativeToolCallTrace]) -> bool {
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

    fn checkpoint_id_from_response_payload(content: &str) -> Option<String> {
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

    fn completed_checkpoint_ids(&self, history: &[TurnItem]) -> std::collections::BTreeSet<String> {
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

    fn checkpoint_summary_from_request_payload(content: &str) -> Option<String> {
        let value = serde_json::from_str::<Value>(content).ok()?;
        value
            .get("input")
            .and_then(|input| input.get("summary"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
            .map(ToString::to_string)
    }

    fn latest_completed_checkpoint_summary(&self) -> Option<String> {
        self.completed_turns.iter().rev().find_map(|turn| {
            turn.tool_calls.iter().rev().find_map(|trace| {
                (trace.tool_name == "checkpoint_complete" && trace.failure.is_none())
                    .then_some(trace.request.as_str())
                    .and_then(Self::checkpoint_summary_from_request_payload)
            })
        })
    }

    fn soft_token_budget_compaction_limit(&self) -> usize {
        self.config
            .token_budget
            .saturating_mul(Self::SOFT_TOKEN_BUDGET_COMPACTION_PERCENT)
            / 100
    }

    fn should_attempt_preemptive_budget_compaction(&self, request_tokens: usize) -> bool {
        self.used_tokens.saturating_add(request_tokens) >= self.soft_token_budget_compaction_limit()
    }

    fn requires_checkpoint_completion_repair(
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

    fn checkpoint_done_repair_item(
        invocation_id: &str,
        turn_index: u32,
        repair_attempt: u8,
        raw_output: &str,
    ) -> TurnItem {
        user_input_item(
            invocation_id,
            turn_index
                .saturating_mul(100)
                .saturating_add(95)
                .saturating_add(u32::from(repair_attempt)),
            "controller_repair",
            format!(
                "Your previous response returned DONE before the active execution checkpoint was completed. Before DONE, call the built-in checkpoint tool exactly as instructed in the prompt, e.g. ACT:tool:checkpoint_complete:{{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}}. After the checkpoint tool succeeds, continue and return DONE only if the task is truly complete. Previous response:\n{raw_output}"
            ),
            "runtime.repair",
        )
    }

    fn declared_checkpoint_ids(request: &ModelTurnRequest) -> Vec<String> {
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

    fn first_declared_checkpoint_id(request: &ModelTurnRequest) -> Option<String> {
        Self::declared_checkpoint_ids(request).into_iter().next()
    }

    fn checkpoint_completion_action_payload(
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

    fn redundant_checkpoint_completion_done_summary(
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

    fn all_declared_checkpoints_completed(
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

    fn post_checkpoint_done_repair_item(
        invocation_id: &str,
        turn_index: u32,
        repair_attempt: u8,
    ) -> TurnItem {
        user_input_item(
            invocation_id,
            turn_index
                .saturating_mul(100)
                .saturating_add(98)
                .saturating_add(u32::from(repair_attempt)),
            "controller_repair",
            format!(
                "Runtime repair #{repair_attempt}: all declared execution checkpoints are complete. Return DONE now with a concise summary. Do not call more tools unless essential to fix a newly discovered failure."
            ),
            "runtime.repair",
        )
    }

    fn checkpoint_auto_completion_action(
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
            serde_json::json!({
                "id": checkpoint_id,
                "summary": summary,
            })
        };
        Some(format!("tool:checkpoint_complete:{payload}"))
    }

    fn budget_thresholds_crossed(&mut self) -> Vec<u8> {
        if self.config.token_budget == 0 {
            return Vec::new();
        }
        let used_percent = self.used_tokens.saturating_mul(100) / self.config.token_budget;
        let mut crossed = Vec::new();
        for threshold in [70_u8, 85, 95] {
            if used_percent >= usize::from(threshold)
                && !self.emitted_budget_thresholds.contains(&threshold)
            {
                self.emitted_budget_thresholds.push(threshold);
                crossed.push(threshold);
            }
        }
        crossed
    }

    /// Execute the loop with explicit history-backed prompt assembly and in-loop tool handling.
    pub(crate) fn run_with_history<BuildRequest, ExecuteAction>(
        &mut self,
        invocation_id: &str,
        initial_items: Vec<TurnItem>,
        build_request: BuildRequest,
        execute_action: ExecuteAction,
    ) -> Result<AgentLoopResult, NativeRuntimeError>
    where
        BuildRequest:
            FnMut(u32, AgentLoopState, &[TurnItem]) -> Result<ModelTurnRequest, NativeRuntimeError>,
        ExecuteAction: FnMut(u32, &str) -> Vec<NativeToolCallTrace>,
    {
        self.run_with_history_observed(
            invocation_id,
            initial_items,
            build_request,
            execute_action,
            None,
        )
    }

    /// Execute the loop with explicit history-backed prompt assembly and optional observers.
    #[allow(
        clippy::collapsible_if,
        clippy::needless_pass_by_value,
        clippy::too_many_lines
    )]
    pub(crate) fn run_with_history_observed<BuildRequest, ExecuteAction>(
        &mut self,
        invocation_id: &str,
        initial_items: Vec<TurnItem>,
        mut build_request: BuildRequest,
        mut execute_action: ExecuteAction,
        mut observer: Option<&mut dyn AgentLoopObserver>,
    ) -> Result<AgentLoopResult, NativeRuntimeError>
    where
        BuildRequest:
            FnMut(u32, AgentLoopState, &[TurnItem]) -> Result<ModelTurnRequest, NativeRuntimeError>,
        ExecuteAction: FnMut(u32, &str) -> Vec<NativeToolCallTrace>,
    {
        self.transition_to(AgentLoopState::Think)?;

        let mut turns = Vec::new();
        let initial_items = normalize_turn_items(&initial_items);
        let mut history = initial_items.clone();
        self.initial_items = initial_items.clone();
        self.history_items = history.clone();
        self.completed_turns.clear();
        let mut malformed_repair_attempts = 0u8;
        let mut token_budget_recovery_attempts = 0u8;
        let mut checkpoint_done_repair_attempts = 0u8;
        let mut post_checkpoint_done_repair_attempts = 0u8;

        while self.state != AgentLoopState::Done {
            self.enforce_budgets()?;
            let from_state = self.state;
            let turn_started = Instant::now();
            let budget_used_before = self.used_tokens;
            let request = build_request(self.next_turn_index, from_state, &history)?;
            let request_tokens = request.prompt.chars().count();
            if self.should_attempt_preemptive_budget_compaction(request_tokens) {
                if let Some(compacted_history) = compact_history_for_budget_pressure(
                    invocation_id,
                    self.next_turn_index,
                    &history,
                ) {
                    if compacted_history != history {
                        let visible_items_before =
                            history.iter().filter(|item| item.model_visible).count();
                        let visible_items_after = compacted_history
                            .iter()
                            .filter(|item| item.model_visible)
                            .count();
                        if let Some(observer) = observer.as_deref_mut() {
                            observer.on_history_compacted(
                                self.next_turn_index,
                                "soft_budget_pressure",
                                request
                                    .prompt_assembly
                                    .as_ref()
                                    .map(|assembly| assembly.rendered_prompt_bytes)
                                    .unwrap_or_default(),
                                request
                                    .prompt_assembly
                                    .as_ref()
                                    .map(|assembly| assembly.selected_history_count)
                                    .unwrap_or_default(),
                                request
                                    .prompt_assembly
                                    .as_ref()
                                    .map(|assembly| assembly.selected_history_chars)
                                    .unwrap_or_default(),
                                visible_items_before,
                                visible_items_after,
                                request_tokens,
                                self.used_tokens.saturating_add(request_tokens),
                                self.config.token_budget,
                                u64::try_from(self.started_at.elapsed().as_millis())
                                    .unwrap_or(u64::MAX),
                            )?;
                        }
                        history = compacted_history;
                        self.history_items = history.clone();
                        continue;
                    }
                }
            }
            if self.used_tokens.saturating_add(request_tokens) > self.config.token_budget {
                if token_budget_recovery_attempts < Self::MAX_TOKEN_BUDGET_RECOVERY_ATTEMPTS {
                    if let Some(compacted_history) = compact_history_for_hard_budget_limit(
                        invocation_id,
                        self.next_turn_index,
                        &history,
                    ) {
                        if compacted_history != history {
                            let visible_items_before =
                                history.iter().filter(|item| item.model_visible).count();
                            let visible_items_after = compacted_history
                                .iter()
                                .filter(|item| item.model_visible)
                                .count();
                            if let Some(observer) = observer.as_deref_mut() {
                                observer.on_history_compacted(
                                    self.next_turn_index,
                                    "hard_budget_limit",
                                    request
                                        .prompt_assembly
                                        .as_ref()
                                        .map(|assembly| assembly.rendered_prompt_bytes)
                                        .unwrap_or_default(),
                                    request
                                        .prompt_assembly
                                        .as_ref()
                                        .map(|assembly| assembly.selected_history_count)
                                        .unwrap_or_default(),
                                    request
                                        .prompt_assembly
                                        .as_ref()
                                        .map(|assembly| assembly.selected_history_chars)
                                        .unwrap_or_default(),
                                    visible_items_before,
                                    visible_items_after,
                                    request_tokens,
                                    self.used_tokens.saturating_add(request_tokens),
                                    self.config.token_budget,
                                    u64::try_from(self.started_at.elapsed().as_millis())
                                        .unwrap_or(u64::MAX),
                                )?;
                            }
                            history = compacted_history;
                            self.history_items = history.clone();
                            token_budget_recovery_attempts =
                                token_budget_recovery_attempts.saturating_add(1);
                            continue;
                        }
                    }
                }
            }
            token_budget_recovery_attempts = 0;
            if let Some(observer) = observer.as_deref_mut() {
                observer.on_turn_request_prepared(&request)?;
            }
            self.used_tokens = self.used_tokens.saturating_add(request_tokens);
            self.enforce_budgets()?;

            let model_started = Instant::now();
            if let Some(observer) = observer.as_deref_mut() {
                observer.on_model_request_started(&request)?;
            }
            let raw_output = match self.model_client.complete_turn(&request) {
                Ok(response) => {
                    if let Some(observer) = observer.as_deref_mut() {
                        observer.on_model_response_received(&request, &response)?;
                    }
                    response
                }
                Err(error) => {
                    if let Some(observer) = observer.as_deref_mut() {
                        observer.on_model_request_failed(&request, &error)?;
                    }
                    return Err(error);
                }
            };
            let model_latency_ms =
                u64::try_from(model_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let response_tokens = raw_output.chars().count();
            self.used_tokens = self.used_tokens.saturating_add(response_tokens);
            let budget_thresholds_crossed = self.budget_thresholds_crossed();
            self.enforce_budgets()?;

            let mut directive = match Self::parse_directive(&raw_output) {
                Ok(directive) => {
                    malformed_repair_attempts = 0;
                    directive
                }
                Err(error @ NativeRuntimeError::MalformedModelOutput { .. })
                    if malformed_repair_attempts
                        < Self::MAX_MALFORMED_DIRECTIVE_RECOVERY_ATTEMPTS =>
                {
                    malformed_repair_attempts = malformed_repair_attempts.saturating_add(1);
                    history.push(Self::malformed_output_repair_item(
                        invocation_id,
                        self.next_turn_index,
                        malformed_repair_attempts,
                        &raw_output,
                    ));
                    history = normalize_turn_items(&history);
                    self.history_items = history.clone();
                    if let Some(observer) = observer.as_deref_mut() {
                        observer.on_model_request_failed(&request, &error)?;
                    }
                    continue;
                }
                Err(error) => return Err(error),
            };
            let mut synthetic_tool_action = None;
            if self.requires_checkpoint_completion_repair(&request, &history, &directive) {
                if checkpoint_done_repair_attempts >= Self::MAX_CHECKPOINT_DONE_RECOVERY_ATTEMPTS {
                    synthetic_tool_action =
                        Self::checkpoint_auto_completion_action(&request, &directive);
                    if synthetic_tool_action.is_none() {
                        return Err(NativeRuntimeError::MalformedModelOutput {
                            raw_output,
                            expected: "ACT:tool:checkpoint_complete:<json_object> before DONE while checkpoints remain"
                                .to_string(),
                            recovery_hint: "Complete the active execution checkpoint with checkpoint_complete before returning DONE"
                                .to_string(),
                        });
                    }
                } else {
                    checkpoint_done_repair_attempts =
                        checkpoint_done_repair_attempts.saturating_add(1);
                    history.push(Self::checkpoint_done_repair_item(
                        invocation_id,
                        self.next_turn_index,
                        checkpoint_done_repair_attempts,
                        &raw_output,
                    ));
                    history = normalize_turn_items(&history);
                    self.history_items = history.clone();
                    continue;
                }
            }
            checkpoint_done_repair_attempts = 0;
            if let Some(summary) =
                self.redundant_checkpoint_completion_done_summary(&request, &history, &directive)
            {
                directive = ModelDirective::Done { summary };
            }
            let all_declared_checkpoints_completed =
                self.all_declared_checkpoints_completed(&request, &history);
            if all_declared_checkpoints_completed
                && !matches!(directive, ModelDirective::Done { .. })
            {
                if post_checkpoint_done_repair_attempts
                    >= Self::MAX_POST_CHECKPOINT_DONE_RECOVERY_ATTEMPTS
                {
                    directive = ModelDirective::Done {
                        summary: self
                            .latest_completed_checkpoint_summary()
                            .unwrap_or_else(|| {
                                let declared = Self::declared_checkpoint_ids(&request);
                                if declared.is_empty() {
                                    "completed declared execution checkpoints".to_string()
                                } else {
                                    format!("completed checkpoints: {}", declared.join(", "))
                                }
                            }),
                    };
                } else {
                    post_checkpoint_done_repair_attempts =
                        post_checkpoint_done_repair_attempts.saturating_add(1);
                    history.push(Self::post_checkpoint_done_repair_item(
                        invocation_id,
                        self.next_turn_index,
                        post_checkpoint_done_repair_attempts,
                    ));
                    history = normalize_turn_items(&history);
                    self.history_items = history.clone();
                    continue;
                }
            } else {
                post_checkpoint_done_repair_attempts = 0;
            }
            let tool_started = Instant::now();
            let tool_calls = if let Some(action) = synthetic_tool_action.as_deref() {
                if let Some(observer) = observer.as_deref_mut() {
                    observer.on_tool_action_started(self.next_turn_index, action)?;
                }
                let calls = execute_action(self.next_turn_index, action);
                if let Some(observer) = observer.as_deref_mut() {
                    observer.on_tool_action_completed(self.next_turn_index, calls.len())?;
                }
                if calls.iter().any(|trace| trace.failure.is_some())
                    && !Self::synthetic_checkpoint_completion_already_satisfied(&calls)
                {
                    return Err(NativeRuntimeError::MalformedModelOutput {
                        raw_output,
                        expected: "checkpoint_complete auto-completion to succeed before DONE"
                            .to_string(),
                        recovery_hint:
                            "Ensure the runtime can execute the built-in checkpoint_complete tool"
                                .to_string(),
                    });
                }
                calls
            } else if matches!(directive, ModelDirective::Done { .. }) {
                Vec::new()
            } else {
                match &directive {
                    ModelDirective::Act { action } => {
                        if let Some(observer) = observer.as_deref_mut() {
                            observer.on_tool_action_started(self.next_turn_index, action)?;
                        }
                        let calls = execute_action(self.next_turn_index, action);
                        if let Some(observer) = observer.as_deref_mut() {
                            observer.on_tool_action_completed(self.next_turn_index, calls.len())?;
                        }
                        calls
                    }
                    _ => Vec::new(),
                }
            };
            let tool_latency_ms =
                u64::try_from(tool_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let to_state = directive.target_state();
            self.transition_to(to_state)?;
            let budget_used_after = self.used_tokens;
            let budget_remaining = self.config.token_budget.saturating_sub(self.used_tokens);
            let turn_duration_ms =
                u64::try_from(turn_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let elapsed_since_invocation_ms =
                u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX);

            let turn = AgentLoopTurn {
                turn_index: self.next_turn_index,
                from_state,
                to_state,
                request,
                directive,
                raw_output,
                budget_used_before,
                budget_used_after,
                budget_remaining,
                budget_thresholds_crossed,
                model_latency_ms,
                tool_latency_ms,
                turn_duration_ms,
                elapsed_since_invocation_ms,
                request_tokens,
                response_tokens,
                tool_calls,
            };
            history.push(assistant_item(
                invocation_id,
                turn.turn_index,
                turn.turn_index.saturating_mul(10).saturating_add(1),
                &turn.directive,
            ));
            for tool_call in &turn.tool_calls {
                history.extend(items_from_tool_trace(invocation_id, &turn, tool_call));
            }
            history = normalize_turn_items(&history);
            self.history_items = history.clone();
            self.completed_turns.push(turn.clone());
            turns.push(turn);
            self.next_turn_index = self.next_turn_index.saturating_add(1);
        }

        self.initial_items = initial_items;
        self.history_items = history;
        self.completed_turns = turns;

        Ok(self.snapshot_result(invocation_id))
    }

    /// Execute the loop deterministically until `done` or a hard budget/error boundary.
    pub fn run(
        &mut self,
        prompt: impl Into<String>,
        context: Option<&str>,
    ) -> Result<AgentLoopResult, NativeRuntimeError> {
        let prompt = prompt.into();
        let context = context.map(ToString::to_string);
        let mode = self.config.agent_mode;
        let mut initial_items = vec![user_input_item(
            "native-loop",
            1,
            "prompt",
            prompt.clone(),
            "runtime.prompt",
        )];
        if let Some(context_text) = context.clone() {
            initial_items.push(user_input_item(
                "native-loop",
                2,
                "context",
                context_text,
                "runtime.context",
            ));
        }

        self.run_with_history(
            "native-loop",
            initial_items,
            move |turn_index, state, _history| {
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: mode,
                    prompt: prompt.clone(),
                    context: context.clone(),
                    prompt_assembly: None,
                })
            },
            |_turn_index, _action| Vec::new(),
        )
    }
}
