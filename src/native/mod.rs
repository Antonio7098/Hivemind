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

use crate::adapters::runtime::{NativeToolCallTrace, NativeTransportTelemetry, RuntimeError};
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

/// Explicit native harness mode separate from orchestration runtime role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    Planner,
    Freeflow,
    #[default]
    TaskExecutor,
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
    /// Reserved prompt budget for the active turn and tool results.
    #[serde(default = "crate::native::support::default_prompt_headroom")]
    pub prompt_headroom: usize,
    /// Explicit native harness mode.
    #[serde(default)]
    pub agent_mode: AgentMode,
    /// Whether native runtime events should inline full payload text.
    pub capture_full_payloads: bool,
}

/// Model request for one loop turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelTurnRequest {
    pub turn_index: u32,
    pub state: AgentLoopState,
    #[serde(default)]
    pub agent_mode: AgentMode,
    pub prompt: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub prompt_assembly: Option<NativePromptAssembly>,
}

/// Provider-agnostic model contract used by native runtime.
pub trait ModelClient: Send + Sync {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError>;

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        NativeTransportTelemetry::default()
    }
}

pub trait AgentLoopObserver {
    fn on_turn_request_prepared(
        &mut self,
        _request: &ModelTurnRequest,
    ) -> Result<(), NativeRuntimeError> {
        Ok(())
    }

    fn on_model_request_started(
        &mut self,
        _request: &ModelTurnRequest,
    ) -> Result<(), NativeRuntimeError> {
        Ok(())
    }

    fn on_model_response_received(
        &mut self,
        _request: &ModelTurnRequest,
        _response: &str,
    ) -> Result<(), NativeRuntimeError> {
        Ok(())
    }

    fn on_model_request_failed(
        &mut self,
        _request: &ModelTurnRequest,
        _error: &NativeRuntimeError,
    ) -> Result<(), NativeRuntimeError> {
        Ok(())
    }

    fn on_tool_action_started(
        &mut self,
        _turn_index: u32,
        _action: &str,
    ) -> Result<(), NativeRuntimeError> {
        Ok(())
    }

    fn on_tool_action_completed(
        &mut self,
        _turn_index: u32,
        _tool_call_count: usize,
    ) -> Result<(), NativeRuntimeError> {
        Ok(())
    }

    fn on_turn_completed(&mut self, _turn: &AgentLoopTurn) -> Result<(), NativeRuntimeError> {
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn on_history_compacted(
        &mut self,
        _turn_index: u32,
        _reason: &str,
        _rendered_prompt_bytes_before: usize,
        _selected_history_count_before: usize,
        _selected_history_chars_before: usize,
        _visible_items_before: usize,
        _visible_items_after: usize,
        _prompt_tokens_before: usize,
        _projected_budget_used: usize,
        _token_budget: usize,
        _elapsed_since_invocation_ms: u64,
    ) -> Result<(), NativeRuntimeError> {
        Ok(())
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
    #[serde(default)]
    pub budget_used_before: usize,
    #[serde(default)]
    pub budget_used_after: usize,
    #[serde(default)]
    pub budget_remaining: usize,
    #[serde(default)]
    pub budget_thresholds_crossed: Vec<u8>,
    #[serde(default)]
    pub model_latency_ms: u64,
    #[serde(default)]
    pub tool_latency_ms: u64,
    #[serde(default)]
    pub turn_duration_ms: u64,
    #[serde(default)]
    pub elapsed_since_invocation_ms: u64,
    #[serde(default)]
    pub request_tokens: usize,
    #[serde(default)]
    pub response_tokens: usize,
    #[serde(default)]
    pub tool_calls: Vec<NativeToolCallTrace>,
}

/// Native loop result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLoopResult {
    pub invocation_id: String,
    pub final_state: AgentLoopState,
    #[serde(default)]
    pub final_summary: Option<String>,
    #[serde(default)]
    pub(crate) initial_items: Vec<TurnItem>,
    #[serde(default)]
    pub(crate) history_items: Vec<TurnItem>,
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
    emitted_budget_thresholds: Vec<u8>,
    initial_items: Vec<TurnItem>,
    history_items: Vec<TurnItem>,
    completed_turns: Vec<AgentLoopTurn>,
}

mod agent_loop;
mod error;
mod mock;
mod prompt_assembly;
mod support;
mod turn_items;

pub use self::mock::MockModelClient;
pub(crate) use self::prompt_assembly::{assemble_native_prompt, NativePromptAssembly};
pub(crate) use self::turn_items::{
    assistant_item, compact_history_for_budget_pressure, compact_history_for_hard_budget_limit,
    compacted_summary_item, items_from_tool_trace, normalize_turn_items, user_input_item, TurnItem,
};

#[cfg(test)]
mod tests;
