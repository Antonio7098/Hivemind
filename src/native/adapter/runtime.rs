use super::observer::{compact_progress_value, NativeProgressObserver, ProgressEmitter};
use super::*;
use std::collections::BTreeSet;
use std::time::Instant;

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
