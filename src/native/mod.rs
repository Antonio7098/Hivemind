//! Native runtime contracts and deterministic harness primitives.
//!
//! Sprint 42 introduces a native runtime contract layer that is provider-agnostic
//! and deterministic under test. The initial implementation intentionally keeps
//! behavior simple and explicit:
//! - strict finite-state machine (`init -> think -> act -> done`)
//! - bounded turn/time/token budgets
//! - structured, actionable runtime errors
//! - scripted mock model client for deterministic harness tests

pub mod adapter;
mod openrouter;
pub mod runtime_hardening;
pub mod startup_hardening;
pub mod tool_engine;

pub use openrouter::OpenRouterModelClient;

use crate::adapters::runtime::{NativeTransportTelemetry, RuntimeError};
use crate::core::error::{ErrorCategory, HivemindError};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Native runtime loop state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLoopState {
    Init,
    Think,
    Act,
    Done,
}

/// Runtime budget configuration for the native loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeRuntimeConfig {
    /// Maximum number of model turns before hard-failing.
    pub max_turns: u32,
    /// Wall-clock budget for one invocation.
    pub timeout_budget: Duration,
    /// Approximate token budget (UTF-8 char count in v1 harness).
    pub token_budget: usize,
    /// Whether native runtime events should inline full payload text.
    pub capture_full_payloads: bool,
}

/// Model request for one loop turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelTurnRequest {
    pub turn_index: u32,
    pub state: AgentLoopState,
    pub prompt: String,
    #[serde(default)]
    pub context: Option<String>,
}

/// Provider-agnostic model contract used by native runtime.
pub trait ModelClient: Send + Sync {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError>;

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        NativeTransportTelemetry::default()
    }
}

/// Parsed model directive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelDirective {
    Think { message: String },
    Act { action: String },
    Done { summary: String },
}

/// One deterministic loop transition record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLoopTurn {
    pub turn_index: u32,
    pub from_state: AgentLoopState,
    pub to_state: AgentLoopState,
    pub request: ModelTurnRequest,
    pub directive: ModelDirective,
    pub raw_output: String,
}

/// Native loop result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLoopResult {
    pub final_state: AgentLoopState,
    #[serde(default)]
    pub final_summary: Option<String>,
    pub turns: Vec<AgentLoopTurn>,
}

/// Structured native runtime error taxonomy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NativeRuntimeError {
    InvalidTransition {
        from: AgentLoopState,
        to: AgentLoopState,
    },
    TurnBudgetExceeded {
        max_turns: u32,
    },
    TimeoutBudgetExceeded {
        budget_ms: u64,
    },
    TokenBudgetExceeded {
        budget: usize,
        used: usize,
    },
    ModelRequestFailed {
        code: String,
        message: String,
        recoverable: bool,
    },
    MalformedModelOutput {
        raw_output: String,
        expected: String,
        recovery_hint: String,
    },
}

/// Deterministic native loop harness.
pub struct AgentLoop<M: ModelClient> {
    config: NativeRuntimeConfig,
    model_client: M,
    state: AgentLoopState,
    started_at: Instant,
    next_turn_index: u32,
    used_tokens: usize,
}

mod agent_loop;
mod error;
mod mock;
mod support;

pub use self::mock::MockModelClient;

#[cfg(test)]
mod tests;
