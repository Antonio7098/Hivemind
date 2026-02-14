//! Kilo adapter - OpenCode-compatible runtime wrapper.

use super::opencode::{OpenCodeAdapter, OpenCodeConfig};
use super::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, InteractiveAdapterEvent,
    InteractiveExecutionResult, RuntimeAdapter, RuntimeError,
};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Kilo adapter configuration.
#[derive(Debug, Clone)]
pub struct KiloConfig {
    /// Base adapter config.
    pub base: AdapterConfig,
    /// Optional model identifier.
    pub model: Option<String>,
    /// Whether to enable verbose output.
    pub verbose: bool,
}

impl KiloConfig {
    /// Creates a new Kilo config.
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            base: AdapterConfig::new("kilo", binary_path).with_timeout(Duration::from_secs(600)),
            model: None,
            verbose: false,
        }
    }

    /// Sets the model.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Enables verbose mode.
    #[must_use]
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}

impl Default for KiloConfig {
    fn default() -> Self {
        Self::new(PathBuf::from("kilo"))
    }
}

/// Kilo runtime adapter.
pub struct KiloAdapter {
    inner: OpenCodeAdapter,
}

impl KiloAdapter {
    /// Creates a new adapter.
    pub fn new(config: KiloConfig) -> Self {
        let mut inner = OpenCodeConfig::new(config.base.binary_path);
        inner.base.name = "kilo".to_string();
        inner.base.args = config.base.args;
        inner.base.env = config.base.env;
        inner.base.timeout = config.base.timeout;
        inner.model = config.model;
        inner.verbose = config.verbose;
        Self {
            inner: OpenCodeAdapter::new(inner),
        }
    }

    /// Creates an adapter with defaults.
    pub fn with_defaults() -> Self {
        Self::new(KiloConfig::default())
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

impl RuntimeAdapter for KiloAdapter {
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
