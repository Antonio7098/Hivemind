//! Native runtime adapter implementation.

use crate::adapters::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, InteractiveAdapterEvent,
    InteractiveExecutionResult, NativeInvocationFailure, NativeInvocationTrace,
    NativePayloadCaptureMode, NativeToolCallFailure, NativeToolCallTrace, NativeTurnTrace,
    RuntimeAdapter, RuntimeError,
};
use crate::native::{AgentLoop, MockModelClient, ModelDirective, NativeRuntimeConfig};
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
                "ACT:emit deterministic runtime marker".to_string(),
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
}

impl NativeRuntimeAdapter {
    #[must_use]
    pub fn new(config: NativeAdapterConfig) -> Self {
        Self {
            config,
            prepared: false,
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
    ) -> NativeInvocationTrace {
        let turns = result
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
                    ModelDirective::Act { action } => {
                        let call_id = format!("{invocation_id}:turn:{}:tool:0", turn.turn_index);
                        if let Some(message) = action.strip_prefix("fail_tool:") {
                            vec![NativeToolCallTrace {
                                call_id,
                                tool_name: "native_action".to_string(),
                                request: action.clone(),
                                response: None,
                                failure: Some(NativeToolCallFailure {
                                    code: "native_tool_call_failed".to_string(),
                                    message: message.trim().to_string(),
                                    recoverable: false,
                                }),
                            }]
                        } else {
                            vec![NativeToolCallTrace {
                                call_id,
                                tool_name: "native_action".to_string(),
                                request: action.clone(),
                                response: Some(format!("completed:{action}")),
                                failure: None,
                            }]
                        }
                    }
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

        NativeInvocationTrace {
            invocation_id: invocation_id.to_string(),
            provider: config.provider_name.clone(),
            model: config.model_name.clone(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            capture_mode: Self::capture_mode(&config.native),
            turns,
            final_state: result.final_state.as_str().to_string(),
            final_summary: result.final_summary.clone(),
            failure: None,
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
}

impl RuntimeAdapter for NativeRuntimeAdapter {
    fn name(&self) -> &str {
        &self.config.base.name
    }

    fn initialize(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn prepare(&mut self, _task_id: Uuid, _worktree: &Path) -> Result<(), RuntimeError> {
        self.prepared = true;
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
                let trace = Self::trace_from_success(&invocation_id, &self.config, &result);
                Ok(
                    ExecutionReport::success(duration, Self::render_stdout(&result), String::new())
                        .with_native_invocation(trace),
                )
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
        Ok(())
    }

    fn config(&self) -> &AdapterConfig {
        &self.config.base
    }
}
