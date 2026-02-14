//! Claude Code adapter - wrapper for Claude Code CLI runtime.

use super::opencode::{OpenCodeAdapter, OpenCodeConfig};
use super::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, InteractiveAdapterEvent,
    InteractiveExecutionResult, RuntimeAdapter, RuntimeError,
};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Claude Code adapter configuration.
#[derive(Debug, Clone)]
pub struct ClaudeCodeConfig {
    /// Base adapter config.
    pub base: AdapterConfig,
    /// Optional model identifier.
    pub model: Option<String>,
}

impl ClaudeCodeConfig {
    /// Creates a new Claude Code config.
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            base: AdapterConfig::new("claude-code", binary_path)
                .with_timeout(Duration::from_secs(600)),
            model: None,
        }
    }

    /// Sets the model.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        let mut cfg = Self::new(PathBuf::from("claude"));
        cfg.base.args = vec!["-p".to_string()];
        cfg
    }
}

/// Claude Code runtime adapter.
pub struct ClaudeCodeAdapter {
    inner: OpenCodeAdapter,
}

impl ClaudeCodeAdapter {
    /// Creates a new adapter.
    pub fn new(config: ClaudeCodeConfig) -> Self {
        let mut inner = OpenCodeConfig::new(config.base.binary_path.clone());
        inner.base.name = "claude-code".to_string();
        inner.base.args = config.base.args;
        inner.base.env = config.base.env;
        inner.base.timeout = config.base.timeout;
        inner.model.clone_from(&config.model);

        if let Some(model) = config.model {
            let has_model_flag = inner
                .base
                .args
                .iter()
                .any(|a| a == "--model" || a.starts_with("--model="));
            if !has_model_flag {
                inner.base.args.extend(["--model".to_string(), model]);
            }
        }

        Self {
            inner: OpenCodeAdapter::new(inner),
        }
    }

    /// Creates an adapter with defaults.
    pub fn with_defaults() -> Self {
        Self::new(ClaudeCodeConfig::default())
    }

    pub fn execute_interactive<F>(
        &mut self,
        input: &ExecutionInput,
        on_event: F,
    ) -> Result<InteractiveExecutionResult, RuntimeError>
    where
        F: FnMut(InteractiveAdapterEvent) -> std::result::Result<(), String>,
    {
        self.inner.execute_interactive(input, on_event)
    }
}

impl RuntimeAdapter for ClaudeCodeAdapter {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn initialize(&mut self) -> Result<(), RuntimeError> {
        self.inner.initialize()
    }

    fn prepare(&mut self, task_id: Uuid, worktree: &std::path::Path) -> Result<(), RuntimeError> {
        self.inner.prepare(task_id, worktree)
    }

    fn execute(&mut self, input: ExecutionInput) -> Result<ExecutionReport, RuntimeError> {
        self.inner.execute(input)
    }

    fn terminate(&mut self) -> Result<(), RuntimeError> {
        self.inner.terminate()
    }

    fn config(&self) -> &AdapterConfig {
        self.inner.config()
    }
}
