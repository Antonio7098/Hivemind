//! Native runtime adapter implementation.

use crate::adapters::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, InteractiveAdapterEvent,
    InteractiveExecutionResult, RuntimeAdapter, RuntimeError,
};
use crate::native::{AgentLoop, MockModelClient, NativeRuntimeConfig};
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
        let model = MockModelClient::from_outputs(self.config.scripted_directives.clone());
        let mut loop_harness = AgentLoop::new(self.config.native.clone(), model);
        let prompt = format!(
            "Task: {}\nSuccess Criteria: {}",
            input.task_description, input.success_criteria
        );
        let run = loop_harness.run(prompt, input.context.as_deref());
        let duration = started_at.elapsed();

        match run {
            Ok(result) => Ok(ExecutionReport::success(
                duration,
                Self::render_stdout(&result),
                String::new(),
            )),
            Err(err) => Err(err.to_runtime_error()),
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
