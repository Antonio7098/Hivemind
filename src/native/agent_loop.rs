use super::*;

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

    #[must_use]
    pub fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        self.model_client.take_transport_telemetry()
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
                request,
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
