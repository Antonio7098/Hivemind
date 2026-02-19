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

use crate::adapters::runtime::RuntimeError;
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

impl AgentLoopState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Think => "think",
            Self::Act => "act",
            Self::Done => "done",
        }
    }
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
}

impl Default for NativeRuntimeConfig {
    fn default() -> Self {
        Self {
            max_turns: 8,
            timeout_budget: Duration::from_secs(300),
            token_budget: 32_000,
        }
    }
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
}

/// Parsed model directive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelDirective {
    Think { message: String },
    Act { action: String },
    Done { summary: String },
}

impl ModelDirective {
    fn target_state(&self) -> AgentLoopState {
        match self {
            Self::Think { .. } => AgentLoopState::Think,
            Self::Act { .. } => AgentLoopState::Act,
            Self::Done { .. } => AgentLoopState::Done,
        }
    }
}

/// One deterministic loop transition record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLoopTurn {
    pub turn_index: u32,
    pub from_state: AgentLoopState,
    pub to_state: AgentLoopState,
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
        message: String,
        recoverable: bool,
    },
    MalformedModelOutput {
        raw_output: String,
        expected: String,
        recovery_hint: String,
    },
}

impl NativeRuntimeError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidTransition { .. } => "native_invalid_transition",
            Self::TurnBudgetExceeded { .. } => "native_turn_budget_exceeded",
            Self::TimeoutBudgetExceeded { .. } => "native_timeout_budget_exceeded",
            Self::TokenBudgetExceeded { .. } => "native_token_budget_exceeded",
            Self::ModelRequestFailed { .. } => "native_model_request_failed",
            Self::MalformedModelOutput { .. } => "native_malformed_model_output",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::InvalidTransition { from, to } => {
                format!(
                    "Invalid native loop transition: {} -> {}",
                    from.as_str(),
                    to.as_str()
                )
            }
            Self::TurnBudgetExceeded { max_turns } => {
                format!("Native loop exceeded turn budget ({max_turns})")
            }
            Self::TimeoutBudgetExceeded { budget_ms } => {
                format!("Native loop exceeded timeout budget ({budget_ms}ms)")
            }
            Self::TokenBudgetExceeded { budget, used } => {
                format!("Native loop exceeded token budget ({used}/{budget})")
            }
            Self::ModelRequestFailed { message, .. } => {
                format!("Native model request failed: {message}")
            }
            Self::MalformedModelOutput { expected, .. } => {
                format!("Malformed native model output. Expected {expected}")
            }
        }
    }

    #[must_use]
    pub const fn recoverable(&self) -> bool {
        match self {
            Self::InvalidTransition { .. }
            | Self::TurnBudgetExceeded { .. }
            | Self::TimeoutBudgetExceeded { .. }
            | Self::TokenBudgetExceeded { .. }
            | Self::MalformedModelOutput { .. } => false,
            Self::ModelRequestFailed { recoverable, .. } => *recoverable,
        }
    }

    #[must_use]
    pub fn recovery_hint(&self) -> Option<String> {
        match self {
            Self::MalformedModelOutput { recovery_hint, .. } => Some(recovery_hint.clone()),
            Self::TurnBudgetExceeded { .. } => {
                Some("Increase native turn budget or simplify prompt objectives".to_string())
            }
            Self::TimeoutBudgetExceeded { .. } => {
                Some("Increase timeout budget or investigate model/runtime latency".to_string())
            }
            Self::TokenBudgetExceeded { .. } => Some(
                "Reduce context size or raise native token budget in configuration".to_string(),
            ),
            Self::ModelRequestFailed { .. } => {
                Some("Check provider connectivity and credentials, then retry".to_string())
            }
            Self::InvalidTransition { .. } => Some(
                "Inspect model directives and FSM transition rules for native runtime".to_string(),
            ),
        }
    }

    #[must_use]
    pub fn to_hivemind_error(&self, origin: &'static str) -> HivemindError {
        let mut err =
            HivemindError::new(ErrorCategory::Runtime, self.code(), self.message(), origin)
                .recoverable(self.recoverable());
        if let Some(hint) = self.recovery_hint() {
            err = err.with_hint(hint);
        }
        err
    }

    #[must_use]
    pub fn to_runtime_error(&self) -> RuntimeError {
        RuntimeError::new(self.code(), self.message(), self.recoverable())
    }
}

/// Deterministic scripted model implementation for tests and harnessing.
#[derive(Debug, Clone, Default)]
pub struct MockModelClient {
    scripted: VecDeque<Result<String, NativeRuntimeError>>,
}

impl MockModelClient {
    #[must_use]
    pub fn new(scripted: Vec<Result<String, NativeRuntimeError>>) -> Self {
        Self {
            scripted: VecDeque::from(scripted),
        }
    }

    #[must_use]
    pub fn from_outputs(scripted: Vec<String>) -> Self {
        let turns = scripted.into_iter().map(Ok).collect::<Vec<_>>();
        Self::new(turns)
    }

    #[must_use]
    pub fn deterministic_default() -> Self {
        Self::from_outputs(vec![
            "ACT:emit deterministic runtime marker".to_string(),
            "DONE:native runtime completed deterministically".to_string(),
        ])
    }
}

impl ModelClient for MockModelClient {
    fn complete_turn(&mut self, _request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        self.scripted.pop_front().unwrap_or_else(|| {
            Ok("DONE:mock model exhausted scripted outputs; auto-completing".to_string())
        })
    }
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

impl<M: ModelClient> AgentLoop<M> {
    #[must_use]
    pub fn new(config: NativeRuntimeConfig, model_client: M) -> Self {
        Self {
            config,
            model_client,
            state: AgentLoopState::Init,
            started_at: Instant::now(),
            next_turn_index: 0,
            used_tokens: 0,
        }
    }

    #[must_use]
    pub const fn state(&self) -> AgentLoopState {
        self.state
    }

    fn transition_to(&mut self, to: AgentLoopState) -> Result<(), NativeRuntimeError> {
        let allowed = matches!(
            (self.state, to),
            (
                AgentLoopState::Init | AgentLoopState::Act,
                AgentLoopState::Think
            ) | (
                AgentLoopState::Think | AgentLoopState::Act,
                AgentLoopState::Act
            ) | (
                AgentLoopState::Think | AgentLoopState::Act | AgentLoopState::Done,
                AgentLoopState::Done
            )
        );
        if !allowed {
            return Err(NativeRuntimeError::InvalidTransition {
                from: self.state,
                to,
            });
        }
        self.state = to;
        Ok(())
    }

    fn enforce_budgets(&self) -> Result<(), NativeRuntimeError> {
        if self.next_turn_index >= self.config.max_turns {
            return Err(NativeRuntimeError::TurnBudgetExceeded {
                max_turns: self.config.max_turns,
            });
        }

        if self.started_at.elapsed() > self.config.timeout_budget {
            let budget_ms = u64::try_from(
                self.config
                    .timeout_budget
                    .as_millis()
                    .min(u128::from(u64::MAX)),
            )
            .unwrap_or(u64::MAX);
            return Err(NativeRuntimeError::TimeoutBudgetExceeded { budget_ms });
        }

        if self.used_tokens > self.config.token_budget {
            return Err(NativeRuntimeError::TokenBudgetExceeded {
                budget: self.config.token_budget,
                used: self.used_tokens,
            });
        }

        Ok(())
    }

    fn parse_directive(raw: &str) -> Result<ModelDirective, NativeRuntimeError> {
        let raw = raw.trim();
        if let Some(message) = raw.strip_prefix("THINK:") {
            return Ok(ModelDirective::Think {
                message: message.trim().to_string(),
            });
        }
        if let Some(action) = raw.strip_prefix("ACT:") {
            return Ok(ModelDirective::Act {
                action: action.trim().to_string(),
            });
        }
        if let Some(summary) = raw.strip_prefix("DONE:") {
            return Ok(ModelDirective::Done {
                summary: summary.trim().to_string(),
            });
        }
        Err(NativeRuntimeError::MalformedModelOutput {
            raw_output: raw.to_string(),
            expected: "THINK:<message> | ACT:<action> | DONE:<summary>".to_string(),
            recovery_hint: "Return one explicit directive with a known prefix (THINK/ACT/DONE)"
                .to_string(),
        })
    }

    /// Execute the loop deterministically until `done` or a hard budget/error boundary.
    pub fn run(
        &mut self,
        prompt: impl Into<String>,
        context: Option<&str>,
    ) -> Result<AgentLoopResult, NativeRuntimeError> {
        self.transition_to(AgentLoopState::Think)?;

        let prompt = prompt.into();
        let mut turns = Vec::new();

        while self.state != AgentLoopState::Done {
            self.enforce_budgets()?;
            let from_state = self.state;
            let request = ModelTurnRequest {
                turn_index: self.next_turn_index,
                state: from_state,
                prompt: prompt.clone(),
                context: context.map(ToString::to_string),
            };
            let raw_output = self.model_client.complete_turn(&request)?;
            self.used_tokens = self.used_tokens.saturating_add(raw_output.chars().count());
            self.enforce_budgets()?;
            let directive = Self::parse_directive(&raw_output)?;
            let to_state = directive.target_state();
            self.transition_to(to_state)?;

            turns.push(AgentLoopTurn {
                turn_index: self.next_turn_index,
                from_state,
                to_state,
                directive,
                raw_output,
            });
            self.next_turn_index = self.next_turn_index.saturating_add(1);
        }

        let final_summary = turns.iter().rev().find_map(|turn| {
            if let ModelDirective::Done { summary } = &turn.directive {
                Some(summary.clone())
            } else {
                None
            }
        });

        Ok(AgentLoopResult {
            final_state: self.state,
            final_summary,
            turns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_loop_transitions_think_act_done() {
        let model = MockModelClient::from_outputs(vec![
            "ACT:run deterministic step".to_string(),
            "DONE:all good".to_string(),
        ]);
        let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
        let result = loop_harness
            .run("test prompt", Some("context"))
            .expect("loop should complete");

        assert_eq!(result.final_state, AgentLoopState::Done);
        assert_eq!(result.turns.len(), 2);
        assert_eq!(result.turns[0].from_state, AgentLoopState::Think);
        assert_eq!(result.turns[0].to_state, AgentLoopState::Act);
        assert_eq!(result.turns[1].from_state, AgentLoopState::Act);
        assert_eq!(result.turns[1].to_state, AgentLoopState::Done);
        assert_eq!(result.final_summary.as_deref(), Some("all good"));
    }

    #[test]
    fn agent_loop_fails_loud_on_invalid_transition() {
        let model = MockModelClient::from_outputs(vec!["THINK:still thinking".to_string()]);
        let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
        let err = loop_harness
            .run("test prompt", None)
            .expect_err("expected invalid transition");

        assert!(matches!(
            err,
            NativeRuntimeError::InvalidTransition {
                from: AgentLoopState::Think,
                to: AgentLoopState::Think
            }
        ));
        assert_eq!(err.code(), "native_invalid_transition");
    }

    #[test]
    fn agent_loop_fails_loud_on_malformed_model_output() {
        let model = MockModelClient::from_outputs(vec!["oops not structured".to_string()]);
        let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
        let err = loop_harness
            .run("test prompt", None)
            .expect_err("expected malformed output");

        let NativeRuntimeError::MalformedModelOutput {
            raw_output,
            recovery_hint,
            ..
        } = err
        else {
            panic!("expected malformed output error");
        };
        assert_eq!(raw_output, "oops not structured");
        assert!(
            recovery_hint.contains("THINK/ACT/DONE"),
            "recovery hint should be explicit"
        );
    }
}
