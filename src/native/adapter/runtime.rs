use super::*;

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
        let report = self.execute(input.clone())?;
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
        let model = Self::build_model_client(&self.config, &runtime_env)?;
        let mut loop_harness = AgentLoop::new(self.config.native.clone(), model);
        let prompt = format!(
            "Task: {}\nSuccess Criteria: {}",
            input.task_description, input.success_criteria
        );
        let run = loop_harness.run(prompt, input.context.as_deref());
        let transport_telemetry = loop_harness.take_transport_telemetry();
        let duration = started_at.elapsed();
        let report = match run {
            Ok(result) => {
                let trace = Self::trace_from_success(
                    &invocation_id,
                    &self.config,
                    &result,
                    transport_telemetry,
                    runtime_state,
                    readiness_transitions,
                    &tool_engine,
                    &tool_context,
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
                let trace = Self::trace_from_failure(
                    &invocation_id,
                    &self.config,
                    loop_harness.state(),
                    &err,
                    transport_telemetry,
                    runtime_state,
                    readiness_transitions,
                );
                ExecutionReport::failure_with_output(
                    1,
                    duration,
                    err.to_runtime_error(),
                    String::new(),
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
