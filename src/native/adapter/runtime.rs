use super::*;
use crate::adapters::runtime::NativeHistoryCompactionTrace;
use crate::native::{AgentLoopObserver, NativeRuntimeError};
use std::collections::BTreeSet;
use std::time::Instant;

type ProgressEmitter<'a> = Box<dyn FnMut(String) -> Result<(), RuntimeError> + 'a>;

fn compact_progress_value(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn progress_callback_error(error: &RuntimeError) -> NativeRuntimeError {
    NativeRuntimeError::ModelRequestFailed {
        code: "native_observability_callback_failed".to_string(),
        message: format!(
            "Native progress callback failed: {}",
            compact_progress_value(&error.message, 200)
        ),
        recoverable: false,
    }
}

struct NativeProgressObserver<'a> {
    emit: Option<ProgressEmitter<'a>>,
    started_at: Instant,
    history_compactions: Vec<NativeHistoryCompactionTrace>,
}

impl<'a> NativeProgressObserver<'a> {
    fn new(emit: Option<ProgressEmitter<'a>>) -> Self {
        Self {
            emit,
            started_at: Instant::now(),
            history_compactions: Vec::new(),
        }
    }

    fn take_history_compactions(&mut self) -> Vec<NativeHistoryCompactionTrace> {
        std::mem::take(&mut self.history_compactions)
    }

    fn emit_line(&mut self, line: impl Into<String>) -> Result<(), NativeRuntimeError> {
        let line = line.into();
        let line = if let Some(stripped) = line.strip_prefix("[native-progress] ") {
            format!(
                "[native-progress] elapsed_ms={} {stripped}",
                self.started_at.elapsed().as_millis()
            )
        } else {
            line
        };
        if let Some(emit) = self.emit.as_mut() {
            emit(format!("{line}\n")).map_err(|error| progress_callback_error(&error))?;
        }
        Ok(())
    }
}

impl AgentLoopObserver for NativeProgressObserver<'_> {
    fn on_turn_request_prepared(
        &mut self,
        request: &ModelTurnRequest,
    ) -> Result<(), NativeRuntimeError> {
        let assembly = request.prompt_assembly.as_ref();
        self.emit_line(format!(
            "[native-progress] stage=turn_request_prepared turn={} state={} prompt_bytes={} context_bytes={} prompt_headroom={} available_budget={} rendered_prompt_bytes={} runtime_context_bytes={} visible_items={} selected_history_count={} selected_history_chars={} compacted_summary_count={} compacted_summary_chars={} assembly_latency_ms={}",
            request.turn_index,
            request.state.as_str(),
            request.prompt.len(),
            request.context.as_ref().map_or(0, String::len),
            assembly.map_or(0, |value| value.prompt_headroom),
            assembly.map_or(0, |value| value.available_budget),
            assembly.map_or(0, |value| value.rendered_prompt_bytes),
            assembly.map_or(0, |value| value.runtime_context_bytes),
            assembly.map_or(0, |value| value.selected_item_count),
            assembly.map_or(0, |value| value.selected_history_count),
            assembly.map_or(0, |value| value.selected_history_chars),
            assembly.map_or(0, |value| value.compacted_summary_count),
            assembly.map_or(0, |value| value.compacted_summary_chars),
            assembly.map_or(0, |value| value.assembly_duration_ms),
        ))
    }

    fn on_model_request_started(
        &mut self,
        request: &ModelTurnRequest,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=model_request_started turn={} state={} agent_mode={}",
            request.turn_index,
            request.state.as_str(),
            request.agent_mode.as_str(),
        ))
    }

    fn on_model_response_received(
        &mut self,
        request: &ModelTurnRequest,
        response: &str,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=model_response_received turn={} response_bytes={} preview={}",
            request.turn_index,
            response.len(),
            compact_progress_value(response, 120),
        ))
    }

    fn on_model_request_failed(
        &mut self,
        request: &ModelTurnRequest,
        error: &NativeRuntimeError,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=model_request_failed turn={} code={} recoverable={} message={}",
            request.turn_index,
            error.code(),
            error.recoverable(),
            compact_progress_value(&error.message(), 160),
        ))
    }

    fn on_tool_action_started(
        &mut self,
        turn_index: u32,
        action: &str,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=tool_action_started turn={} action={}",
            turn_index,
            compact_progress_value(action, 160),
        ))
    }

    fn on_tool_action_completed(
        &mut self,
        turn_index: u32,
        tool_call_count: usize,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=tool_action_completed turn={turn_index} tool_call_count={tool_call_count}",
        ))
    }

    fn on_history_compacted(
        &mut self,
        turn_index: u32,
        reason: &str,
        rendered_prompt_bytes_before: usize,
        selected_history_count_before: usize,
        selected_history_chars_before: usize,
        visible_items_before: usize,
        visible_items_after: usize,
        prompt_tokens_before: usize,
        projected_budget_used: usize,
        token_budget: usize,
        elapsed_since_invocation_ms: u64,
    ) -> Result<(), NativeRuntimeError> {
        self.history_compactions.push(NativeHistoryCompactionTrace {
            turn_index,
            reason: reason.to_string(),
            rendered_prompt_bytes_before,
            selected_history_count_before,
            selected_history_chars_before,
            visible_items_before,
            visible_items_after,
            prompt_tokens_before,
            projected_budget_used,
            token_budget,
            elapsed_since_invocation_ms,
        });
        self.emit_line(format!(
            "[native-progress] stage=history_compacted turn={turn_index} reason={reason} rendered_prompt_bytes_before={rendered_prompt_bytes_before} selected_history_count_before={selected_history_count_before} selected_history_chars_before={selected_history_chars_before} visible_items_before={visible_items_before} visible_items_after={visible_items_after} prompt_tokens_before={prompt_tokens_before} projected_budget_used={projected_budget_used} token_budget={token_budget}",
        ))
    }
}

impl NativeRuntimeAdapter {
    #[must_use]
    pub fn new(config: NativeAdapterConfig) -> Self {
        Self {
            config,
            prepared: false,
            worktree: None,
        }
    }

    pub fn execute_interactive<F>(
        &mut self,
        input: &ExecutionInput,
        mut on_event: F,
    ) -> Result<InteractiveExecutionResult, RuntimeError>
    where
        F: FnMut(InteractiveAdapterEvent) -> std::result::Result<(), String>,
    {
        let report = self.execute_with_progress(
            input,
            Some(Box::new(|content| {
                on_event(InteractiveAdapterEvent::Output { content })
                    .map_err(|e| RuntimeError::new("interactive_callback_failed", e, false))
            })),
        )?;
        if !report.stdout.is_empty() {
            on_event(InteractiveAdapterEvent::Output {
                content: report.stdout.clone(),
            })
            .map_err(|e| RuntimeError::new("interactive_callback_failed", e, false))?;
        }
        Ok(InteractiveExecutionResult {
            report,
            terminated_reason: None,
        })
    }

    fn scope_from_env(env: &HashMap<String, String>) -> Result<Option<Scope>, RuntimeError> {
        let Some(raw_scope) = env.get("HIVEMIND_TASK_SCOPE_JSON") else {
            return Ok(None);
        };
        serde_json::from_str::<Scope>(raw_scope)
            .map(Some)
            .map_err(|error| {
                RuntimeError::new(
                    "native_scope_decode_failed",
                    format!("Failed to decode HIVEMIND_TASK_SCOPE_JSON: {error}"),
                    false,
                )
            })
    }

    fn build_model_client(
        config: &NativeAdapterConfig,
        runtime_env: &HashMap<String, String>,
    ) -> Result<Box<dyn ModelClient>, RuntimeError> {
        if config.provider_name.eq_ignore_ascii_case("openrouter") {
            let client = OpenRouterModelClient::from_env(config.model_name.clone(), runtime_env)
                .map_err(|error| error.to_runtime_error())?;
            Ok(Box::new(client))
        } else {
            Ok(Box::new(MockModelClient::from_outputs(
                config.scripted_directives.clone(),
            )))
        }
    }

    fn initial_turn_items(invocation_id: &str, input: &ExecutionInput) -> Vec<TurnItem> {
        let mut items = Vec::new();
        let task_item = user_input_item(
            invocation_id,
            1,
            "objective",
            format!(
                "Task: {}\nSuccess Criteria: {}",
                input.task_description, input.success_criteria
            ),
            "runtime.execution_input",
        );
        let mut source_item_ids = vec![task_item.id.clone()];
        items.push(task_item);

        if let Some(context) = input.context.clone() {
            let context_item = user_input_item(
                invocation_id,
                2,
                "context",
                context,
                "runtime.execution_context",
            );
            source_item_ids.push(context_item.id.clone());
            items.push(context_item);
        }

        if let Some(verifier_feedback) = input.verifier_feedback.clone() {
            items.push(user_input_item(
                invocation_id,
                3,
                "verifier_feedback",
                verifier_feedback,
                "runtime.verifier_feedback",
            ));
        }

        if !input.prior_attempts.is_empty() {
            let summary = input
                .prior_attempts
                .iter()
                .map(|attempt| {
                    let failure = attempt
                        .failure_reason
                        .as_deref()
                        .map(|reason| format!(" | failure={reason}"))
                        .unwrap_or_default();
                    format!(
                        "attempt {}: {}{}",
                        attempt.attempt_number, attempt.summary, failure
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            items.push(compacted_summary_item(
                invocation_id,
                4,
                None,
                summary,
                source_item_ids,
            ));
        }

        items
    }

    fn allowed_capabilities(contracts: &[crate::native::tool_engine::ToolContract]) -> Vec<String> {
        let mut capabilities = BTreeSet::new();
        for contract in contracts {
            capabilities.insert(contract.required_scope.clone());
            for permission in &contract.required_permissions {
                capabilities.insert(format!("{permission:?}").to_ascii_lowercase());
            }
        }
        capabilities.into_iter().collect()
    }

    #[allow(clippy::too_many_lines)]
    fn execute_with_progress(
        &self,
        input: &ExecutionInput,
        emit: Option<ProgressEmitter<'_>>,
    ) -> Result<ExecutionReport, RuntimeError> {
        if !self.prepared {
            return Err(RuntimeError::new(
                "not_prepared",
                "Native runtime adapter not prepared",
                false,
            ));
        }
        let worktree = self.worktree.as_ref().ok_or_else(|| {
            RuntimeError::new(
                "worktree_not_prepared",
                "Native runtime adapter missing prepared worktree",
                false,
            )
        })?;
        let mut runtime_env = self.config.base.env.clone();
        let runtime_support = NativeRuntimeSupport::bootstrap(&runtime_env)
            .map_err(|error| error.to_runtime_error())?;
        let readiness_transitions = runtime_support.readiness_transitions();
        let runtime_state = Some(runtime_support.telemetry());
        runtime_support
            .ensure_secret_from_or_to_env(&mut runtime_env, "OPENROUTER_API_KEY")
            .map_err(|error| error.to_runtime_error())?;

        let scope = Self::scope_from_env(&runtime_env)?;
        let sandbox_policy = NativeSandboxPolicy::from_env(&runtime_env);
        let approval_policy = NativeApprovalPolicy::from_env(&runtime_env);
        let network_policy = NativeNetworkPolicy::from_env(&runtime_env);
        let command_policy = NativeCommandPolicy::from_env(&runtime_env);
        let exec_policy_manager = NativeExecPolicyManager::from_env(&runtime_env);
        let tool_context = ToolExecutionContext {
            worktree,
            scope: scope.as_ref(),
            sandbox_policy,
            approval_policy,
            network_policy,
            command_policy,
            exec_policy_manager,
            approval_cache: RefCell::new(NativeApprovalCache::default()),
            network_approval_cache: RefCell::new(NativeNetworkApprovalCache::default()),
            env: &runtime_env,
        };
        let tool_engine = NativeToolEngine::default();
        let allowed_contracts = tool_engine.contracts_for_mode(self.config.native.agent_mode);
        let allowed_tools = allowed_contracts
            .iter()
            .map(|contract| contract.name.clone())
            .collect::<Vec<_>>();
        let allowed_capabilities = Self::allowed_capabilities(&allowed_contracts);
        let mut observer = NativeProgressObserver::new(emit);

        let timeout_budget_ms =
            u64::try_from(self.config.native.timeout_budget.as_millis()).unwrap_or(u64::MAX);
        observer
            .emit_line(format!(
                "[native-progress] stage=invocation_starting provider={} model={} agent_mode={} max_turns={} token_budget={} prompt_headroom={} timeout_budget_ms={} capture_full_payloads={}",
                self.config.provider_name,
                self.config.model_name,
                self.config.native.agent_mode.as_str(),
                self.config.native.max_turns,
                self.config.native.token_budget,
                self.config.native.prompt_headroom,
                timeout_budget_ms,
                self.config.native.capture_full_payloads,
            ))
            .map_err(|error| error.to_runtime_error())?;
        if let Some(runtime_state) = runtime_state.as_ref() {
            observer
                .emit_line(format!(
                    "[native-progress] stage=runtime_support_ready db_path={} readiness_transition_count={}",
                    compact_progress_value(&runtime_state.db_path, 120),
                    readiness_transitions.len(),
                ))
                .map_err(|error| error.to_runtime_error())?;
        }
        observer
            .emit_line(format!(
                "[native-progress] stage=tool_contracts_ready allowed_tools={} allowed_capabilities={}",
                allowed_tools.join(","),
                allowed_capabilities.join(","),
            ))
            .map_err(|error| error.to_runtime_error())?;

        let started_at = Instant::now();
        let invocation_id = Uuid::new_v4().to_string();
        runtime_support
            .ingest_log(
                "native_runtime",
                "info",
                "invocation_starting",
                Some(format!("{{\"invocation_id\":\"{invocation_id}\"}}")),
            )
            .map_err(|error| error.to_runtime_error())?;
        observer
            .emit_line(format!(
                "[native-progress] stage=model_client_building provider={} model={}",
                self.config.provider_name, self.config.model_name,
            ))
            .map_err(|error| error.to_runtime_error())?;
        let model = Self::build_model_client(&self.config, &runtime_env)?;
        observer
            .emit_line(format!(
                "[native-progress] stage=model_client_ready provider={} model={} fallback_configured={}",
                self.config.provider_name,
                self.config.model_name,
                runtime_env
                    .get("OPENROUTER_FALLBACK_ENDPOINT")
                    .is_some_and(|value| !value.trim().is_empty()),
            ))
            .map_err(|error| error.to_runtime_error())?;
        let mut loop_harness = AgentLoop::new(self.config.native.clone(), model);
        let initial_items = Self::initial_turn_items(&invocation_id, input);
        observer
            .emit_line(format!(
                "[native-progress] stage=loop_starting invocation_id={} initial_items={}",
                invocation_id,
                initial_items.len(),
            ))
            .map_err(|error| error.to_runtime_error())?;
        let agent_mode = self.config.native.agent_mode;
        let run = loop_harness.run_with_history_observed(
            &invocation_id,
            initial_items,
            |turn_index, state, history| {
                let assembly_started = Instant::now();
                let rendered =
                    assemble_native_prompt(&self.config.native, input, history, &allowed_contracts);
                let assembly_duration_ms =
                    u64::try_from(assembly_started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let mut assembly = rendered.assembly;
                assembly.assembly_duration_ms = assembly_duration_ms;
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(assembly),
                })
            },
            |turn_index, action| {
                vec![Self::tool_trace_for_act(
                    &invocation_id,
                    agent_mode,
                    turn_index,
                    action,
                    &tool_engine,
                    &tool_context,
                )]
            },
            Some(&mut observer),
        );
        let transport_telemetry = loop_harness.take_transport_telemetry();
        let duration = started_at.elapsed();
        let report = match run {
            Ok(result) => {
                observer
                    .emit_line(format!(
                        "[native-progress] stage=invocation_finished success=true final_state={} total_turns={} summary={}",
                        result.final_state.as_str(),
                        result.turns.len(),
                        compact_progress_value(result.final_summary.as_deref().unwrap_or(""), 160),
                    ))
                    .map_err(|error| error.to_runtime_error())?;
                let trace = Self::trace_from_success(
                    &invocation_id,
                    &self.config,
                    &result,
                    transport_telemetry,
                    runtime_state,
                    readiness_transitions,
                    observer.take_history_compactions(),
                    allowed_tools,
                    allowed_capabilities,
                );
                let stdout = Self::render_stdout(&result);
                if let Some(failure) = trace.failure.clone() {
                    ExecutionReport::failure_with_output(
                        1,
                        duration,
                        RuntimeError::new(failure.code, failure.message, failure.recoverable),
                        stdout,
                        String::new(),
                    )
                    .with_native_invocation(trace)
                } else {
                    ExecutionReport::success(duration, stdout, String::new())
                        .with_native_invocation(trace)
                }
            }
            Err(err) => {
                let partial = loop_harness.snapshot_result(&invocation_id);
                observer
                    .emit_line(format!(
                        "[native-progress] stage=invocation_finished success=false final_state={} total_turns={} error_code={} error_message={}",
                        loop_harness.state().as_str(),
                        partial.turns.len(),
                        err.code(),
                        compact_progress_value(&err.message(), 160),
                    ))
                    .map_err(|error| error.to_runtime_error())?;
                let trace = Self::trace_from_failure(
                    &invocation_id,
                    &self.config,
                    &partial,
                    loop_harness.state(),
                    &err,
                    transport_telemetry,
                    runtime_state,
                    readiness_transitions,
                    observer.take_history_compactions(),
                    allowed_tools,
                    allowed_capabilities,
                );
                ExecutionReport::failure_with_output(
                    1,
                    duration,
                    err.to_runtime_error(),
                    Self::render_stdout(&partial),
                    err.message(),
                )
                .with_native_invocation(trace)
            }
        };

        runtime_support
            .ingest_log(
                "native_runtime",
                "info",
                "invocation_finished",
                Some(format!(
                    "{{\"invocation_id\":\"{invocation_id}\",\"exit_code\":{}}}",
                    report.exit_code
                )),
            )
            .map_err(|error| error.to_runtime_error())?;
        runtime_support
            .flush_logs()
            .map_err(|error| error.to_runtime_error())?;
        runtime_support
            .shutdown()
            .map_err(|error| error.to_runtime_error())?;

        Ok(report)
    }
}

impl RuntimeAdapter for NativeRuntimeAdapter {
    fn name(&self) -> &str {
        &self.config.base.name
    }

    fn initialize(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn prepare(&mut self, _task_id: Uuid, worktree: &Path) -> Result<(), RuntimeError> {
        self.prepared = true;
        self.worktree = Some(worktree.to_path_buf());
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn execute(&mut self, input: ExecutionInput) -> Result<ExecutionReport, RuntimeError> {
        self.execute_with_progress(&input, None)
    }

    fn terminate(&mut self) -> Result<(), RuntimeError> {
        let _ = cleanup_exec_sessions();
        self.prepared = false;
        self.worktree = None;
        Ok(())
    }

    fn config(&self) -> &AdapterConfig {
        &self.config.base
    }
}
