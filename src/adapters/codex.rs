//! Codex adapter - wrapper for Codex CLI runtime.

use super::opencode::{OpenCodeAdapter, OpenCodeConfig};
use super::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, InteractiveAdapterEvent,
    InteractiveExecutionResult, RuntimeAdapter, RuntimeError,
};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Codex adapter configuration.
#[derive(Debug, Clone)]
pub struct CodexConfig {
    /// Base adapter config.
    pub base: AdapterConfig,
    /// Optional model identifier.
    pub model: Option<String>,
}

impl CodexConfig {
    /// Creates a new codex config.
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            base: AdapterConfig::new("codex", binary_path).with_timeout(Duration::from_secs(600)),
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

impl Default for CodexConfig {
    fn default() -> Self {
        let mut cfg = Self::new(PathBuf::from("codex"));
        cfg.base.args = vec![
            "exec".to_string(),
            "--skip-git-repo-check".to_string(),
            "-".to_string(),
        ];
        cfg
    }
}

/// Codex runtime adapter.
pub struct CodexAdapter {
    inner: OpenCodeAdapter,
}

impl CodexAdapter {
    /// Creates a new adapter.
    pub fn new(config: CodexConfig) -> Self {
        let mut inner = OpenCodeConfig::new(config.base.binary_path.clone());
        inner.base.name = "codex".to_string();
        inner.base.args = config.base.args;
        inner.base.env = config.base.env;
        inner.base.timeout = config.base.timeout;
        inner.model.clone_from(&config.model);

        // Codex does not use OpenCode model flag semantics; append as explicit args when requested.
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
        Self::new(CodexConfig::default())
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

impl RuntimeAdapter for CodexAdapter {
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
