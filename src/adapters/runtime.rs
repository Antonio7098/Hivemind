//! Runtime adapter interface for execution backends.
//!
//! Runtime adapters are the only components that interact directly with
//! execution runtimes (e.g., Claude Code, `OpenCode`, Codex CLI). They provide
//! a uniform interface for task execution regardless of the underlying agent.
//!
//! # Core Types
//!
//! - [`RuntimeAdapter`] - Trait that all adapters must implement
//! - [`ExecutionInput`] - Input for a task execution (description, criteria, context)
//! - [`ExecutionReport`] - Result of an execution (exit code, output, file changes)
//! - [`AdapterConfig`] - Configuration for an adapter (binary path, timeout, env)
//!
//! # Adapter Lifecycle
//!
//! ```text
//! initialize() → prepare(task_id, worktree) → execute(input) → terminate()
//! ```
//!
//! 1. **Initialize**: Set up the adapter (validate binary, configure environment)
//! 2. **Prepare**: Set up for a specific task (create worktree, configure scope)
//! 3. **Execute**: Run the task and collect results
//! 4. **Terminate**: Clean up resources
//!
//! # Example: Using an Adapter
//!
//! ```no_run
//! use hivemind::adapters::runtime::{AdapterConfig, ExecutionInput, RuntimeAdapter};
//! use std::path::PathBuf;
//! use std::time::Duration;
//!
//! let config = AdapterConfig::new("opencode", PathBuf::from("/usr/local/bin/opencode"))
//!     .with_timeout(Duration::from_secs(300))
//!     .with_env("MODEL", "claude-3-opus");
//!
//! let input = ExecutionInput {
//!     task_description: "Implement user authentication".to_string(),
//!     success_criteria: "Users can log in and log out".to_string(),
//!     context: Some("Use JWT tokens".to_string()),
//!     prior_attempts: vec![],
//!     verifier_feedback: None,
//!     native_prompt_metadata: None,
//! };
//! ```
//!
//! # Built-in Adapters
//!
//! See the adapter implementations:
//! - [`opencode`](super::opencode) - `OpenCode` adapter (primary)
//! - [`claude_code`](super::claude_code) - Claude Code adapter
//! - [`codex`](super::codex) - Codex CLI adapter
//! - [`kilo`](super::kilo) - Kilo adapter

mod adapter;
mod env;
mod mock;
mod prompt;
mod telemetry;

pub use adapter::{
    AdapterConfig, ExecutionReport, InteractiveAdapterEvent, InteractiveExecutionResult,
    RuntimeAdapter, RuntimeError,
};
pub use env::{
    build_protected_runtime_environment, deterministic_env_pairs, ProtectedRuntimeEnvironment,
    RuntimeEnvBuildError, RuntimeEnvInheritMode, RuntimeEnvironmentProvenance,
};
pub use mock::MockAdapter;
pub use prompt::{format_execution_prompt, AttemptSummary, ExecutionInput, NativePromptMetadata};
pub use telemetry::{
    NativeInvocationFailure, NativeInvocationTrace, NativePayloadCaptureMode,
    NativeReadinessTransition, NativeHistoryCompactionTrace, NativeRuntimeStateTelemetry, NativeToolCallFailure,
    NativeToolCallTrace, NativeTransportAttemptTrace, NativeTransportFallbackTrace,
    NativeTransportTelemetry, NativeTurnTrace,
};

#[cfg(test)]
mod tests;
