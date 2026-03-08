//! `OpenCode` adapter - wrapper for `OpenCode` CLI runtime.
//!
//! This adapter wraps the `OpenCode` CLI to provide task execution
//! capabilities within Hivemind's orchestration framework.

use super::runtime::{
    deterministic_env_pairs, format_execution_prompt, AdapterConfig, ExecutionInput,
    ExecutionReport, InteractiveAdapterEvent, InteractiveExecutionResult, RuntimeAdapter,
    RuntimeError,
};
use std::env;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;

mod config;
mod interactive;
pub use config::*;
mod runtime_impl;

/// `OpenCode` runtime adapter.
pub struct OpenCodeAdapter {
    config: OpenCodeConfig,
    worktree: Option<PathBuf>,
    task_id: Option<Uuid>,
    process: Option<Child>,
}
impl OpenCodeAdapter {
    /// Creates a new `OpenCode` adapter.
    pub fn new(config: OpenCodeConfig) -> Self {
        Self {
            config,
            worktree: None,
            task_id: None,
            process: None,
        }
    }

    /// Creates an adapter with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(OpenCodeConfig::default())
    }

    /// Formats the input for the runtime.
    #[allow(clippy::unused_self)]
    fn format_input(&self, input: &ExecutionInput) -> String {
        format_execution_prompt(input)
    }
}

#[cfg(test)]
mod tests;
