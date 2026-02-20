//! Native runtime adapter implementation.

use crate::adapters::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, InteractiveAdapterEvent,
    InteractiveExecutionResult, NativeInvocationFailure, NativeInvocationTrace,
    NativePayloadCaptureMode, NativeToolCallFailure, NativeToolCallTrace, NativeTurnTrace,
    RuntimeAdapter, RuntimeError,
};
use crate::core::scope::Scope;
use crate::native::tool_engine::{
    NativeCommandPolicy, NativeToolAction, NativeToolEngine, ToolExecutionContext,
};
use crate::native::{AgentLoop, MockModelClient, ModelDirective, NativeRuntimeConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Adapter configuration for the native runtime.
#[derive(Debug, Clone)]
pub struct NativeAdapterConfig {
    pub base: AdapterConfig,
    pub native: NativeRuntimeConfig,
    /// Scripted directives for deterministic harness execution.
    pub scripted_directives: Vec<String>,
    pub provider_name: String,
    pub model_name: String,
}

impl NativeAdapterConfig {
    #[must_use]
    pub fn new() -> Self {
        Self {
            base: AdapterConfig::new("native", PathBuf::from("builtin-native"))
                .with_timeout(Duration::from_secs(300)),
            native: NativeRuntimeConfig::default(),
            scripted_directives: vec![
                "ACT:tool:list_files:{\"path\":\".\",\"recursive\":false}".to_string(),
                "DONE:native runtime completed deterministically".to_string(),
            ],
            provider_name: "mock".to_string(),
            model_name: "native-mock-v1".to_string(),
        }
    }
}

impl Default for NativeAdapterConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Native runtime adapter for deterministic loop execution.
pub struct NativeRuntimeAdapter {
    config: NativeAdapterConfig,
    prepared: bool,
    worktree: Option<PathBuf>,
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

    fn render_stdout(result: &crate::native::AgentLoopResult) -> String {
        let mut lines = Vec::new();
        for turn in &result.turns {
            let directive = match &turn.directive {
                crate::native::ModelDirective::Think { message } => format!("THINK {message}"),
                crate::native::ModelDirective::Act { action } => format!("ACT {action}"),
                crate::native::ModelDirective::Done { summary } => format!("DONE {summary}"),
            };
            lines.push(format!(
                "native turn {}: {} -> {} | {directive}",
                turn.turn_index,
                turn.from_state.as_str(),
                turn.to_state.as_str(),
            ));
        }
        if let Some(summary) = result.final_summary.as_ref() {
            lines.push(format!("native summary: {summary}"));
        }
        lines.join("\n")
    }

    fn capture_mode(config: &NativeRuntimeConfig) -> NativePayloadCaptureMode {
        if config.capture_full_payloads {
            NativePayloadCaptureMode::FullPayload
        } else {
            NativePayloadCaptureMode::MetadataOnly
        }
    }

    fn trace_from_success(
        invocation_id: &str,
        config: &NativeAdapterConfig,
        result: &crate::native::AgentLoopResult,
        tool_engine: &NativeToolEngine,
        tool_context: &ToolExecutionContext<'_>,
    ) -> NativeInvocationTrace {
        let turns: Vec<NativeTurnTrace> = result
            .turns
            .iter()
            .map(|turn| {
                let model_request = serde_json::to_string(&turn.request).unwrap_or_else(|_| {
                    format!(
                        "turn={} state={} prompt={}",
                        turn.request.turn_index,
                        turn.request.state.as_str(),
                        turn.request.prompt
                    )
                });

                let tool_calls = match &turn.directive {
                    ModelDirective::Act { action } => Self::tool_trace_for_act(
                        invocation_id,
                        turn.turn_index,
                        action,
                        tool_engine,
                        tool_context,
                    ),
                    _ => Vec::new(),
                };

                let turn_summary = if let ModelDirective::Done { summary } = &turn.directive {
                    Some(summary.clone())
                } else {
                    None
                };

                NativeTurnTrace {
                    turn_index: turn.turn_index,
                    from_state: turn.from_state.as_str().to_string(),
                    to_state: turn.to_state.as_str().to_string(),
                    model_request,
                    model_response: turn.raw_output.clone(),
                    tool_calls,
                    turn_summary,
                }
            })
            .collect();

        let failure = turns
            .iter()
            .flat_map(|turn| turn.tool_calls.iter())
            .find_map(|call| {
                call.failure
                    .as_ref()
                    .map(|failure| NativeInvocationFailure {
                        code: failure.code.clone(),
                        message: failure.message.clone(),
                        recoverable: failure.recoverable,
                        recovery_hint: None,
                    })
            });

        NativeInvocationTrace {
            invocation_id: invocation_id.to_string(),
            provider: config.provider_name.clone(),
            model: config.model_name.clone(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            capture_mode: Self::capture_mode(&config.native),
            turns,
            final_state: result.final_state.as_str().to_string(),
            final_summary: result.final_summary.clone(),
            failure,
        }
    }

    fn trace_from_failure(
        invocation_id: &str,
        config: &NativeAdapterConfig,
        final_state: crate::native::AgentLoopState,
        err: &crate::native::NativeRuntimeError,
    ) -> NativeInvocationTrace {
        NativeInvocationTrace {
            invocation_id: invocation_id.to_string(),
            provider: config.provider_name.clone(),
            model: config.model_name.clone(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            capture_mode: Self::capture_mode(&config.native),
            turns: Vec::new(),
            final_state: final_state.as_str().to_string(),
            final_summary: None,
            failure: Some(NativeInvocationFailure {
                code: err.code().to_string(),
                message: err.message(),
                recoverable: err.recoverable(),
                recovery_hint: err.recovery_hint(),
            }),
        }
    }

    fn tool_trace_for_act(
        invocation_id: &str,
        turn_index: u32,
        action: &str,
        tool_engine: &NativeToolEngine,
        tool_context: &ToolExecutionContext<'_>,
    ) -> Vec<NativeToolCallTrace> {
        let call_id = format!("{invocation_id}:turn:{turn_index}:tool:0");
        if let Some(message) = action.strip_prefix("fail_tool:") {
            return vec![NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                response: None,
                failure: Some(NativeToolCallFailure {
                    code: "native_tool_call_failed".to_string(),
                    message: message.trim().to_string(),
                    recoverable: false,
                }),
            }];
        }

        match NativeToolAction::parse(action) {
            Ok(Some(tool_action)) => {
                vec![tool_engine.execute_action_trace(call_id, &tool_action, tool_context)]
            }
            Ok(None) => vec![NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                response: Some(format!("completed:{action}")),
                failure: None,
            }],
            Err(error) => vec![NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                response: None,
                failure: Some(NativeToolCallFailure {
                    code: error.code,
                    message: error.message,
                    recoverable: error.recoverable,
                }),
            }],
        }
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
        let scope = Self::scope_from_env(&self.config.base.env)?;
        let command_policy = NativeCommandPolicy::from_env(&self.config.base.env);
        let tool_context = ToolExecutionContext {
            worktree,
            scope: scope.as_ref(),
            command_policy: &command_policy,
            env: &self.config.base.env,
        };
        let tool_engine = NativeToolEngine::default();

        let started_at = Instant::now();
        let invocation_id = Uuid::new_v4().to_string();
        let model = MockModelClient::from_outputs(self.config.scripted_directives.clone());
        let mut loop_harness = AgentLoop::new(self.config.native.clone(), model);
        let prompt = format!(
            "Task: {}\nSuccess Criteria: {}",
            input.task_description, input.success_criteria
        );
        let run = loop_harness.run(prompt, input.context.as_deref());
        let duration = started_at.elapsed();

        match run {
            Ok(result) => {
                let trace = Self::trace_from_success(
                    &invocation_id,
                    &self.config,
                    &result,
                    &tool_engine,
                    &tool_context,
                );
                let stdout = Self::render_stdout(&result);
                if let Some(failure) = trace.failure.clone() {
                    Ok(ExecutionReport::failure_with_output(
                        1,
                        duration,
                        RuntimeError::new(failure.code, failure.message, failure.recoverable),
                        stdout,
                        String::new(),
                    )
                    .with_native_invocation(trace))
                } else {
                    Ok(ExecutionReport::success(duration, stdout, String::new())
                        .with_native_invocation(trace))
                }
            }
            Err(err) => {
                let trace = Self::trace_from_failure(
                    &invocation_id,
                    &self.config,
                    loop_harness.state(),
                    &err,
                );
                Ok(ExecutionReport::failure_with_output(
                    1,
                    duration,
                    err.to_runtime_error(),
                    String::new(),
                    err.message(),
                )
                .with_native_invocation(trace))
            }
        }
    }

    fn terminate(&mut self) -> Result<(), RuntimeError> {
        self.prepared = false;
        self.worktree = None;
        Ok(())
    }

    fn config(&self) -> &AdapterConfig {
        &self.config.base
    }
}
