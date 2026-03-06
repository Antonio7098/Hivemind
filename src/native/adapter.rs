//! Native runtime adapter implementation.

use crate::adapters::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, InteractiveAdapterEvent,
    InteractiveExecutionResult, NativeInvocationFailure, NativeInvocationTrace,
    NativePayloadCaptureMode, NativeReadinessTransition, NativeRuntimeStateTelemetry,
    NativeToolCallFailure, NativeToolCallTrace, NativeTransportTelemetry, NativeTurnTrace,
    RuntimeAdapter, RuntimeError,
};
use crate::core::scope::Scope;
use crate::native::runtime_hardening::NativeRuntimeSupport;
use crate::native::tool_engine::{
    cleanup_exec_sessions, NativeApprovalCache, NativeApprovalPolicy, NativeCommandPolicy,
    NativeExecPolicyManager, NativeNetworkApprovalCache, NativeNetworkPolicy, NativeSandboxPolicy,
    NativeToolAction, NativeToolEngine, ToolExecutionContext,
};
use crate::native::{
    AgentLoop, MockModelClient, ModelClient, ModelDirective, NativeRuntimeConfig,
    OpenRouterModelClient,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

mod config;
mod runtime;
mod trace;

#[cfg(test)]
mod tests;

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

/// Native runtime adapter for deterministic loop execution.
pub struct NativeRuntimeAdapter {
    config: NativeAdapterConfig,
    prepared: bool,
    worktree: Option<PathBuf>,
}
