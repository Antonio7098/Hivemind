use super::*;

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

impl Default for NativeRuntimeConfig {
    fn default() -> Self {
        Self {
            max_turns: 8,
            timeout_budget: Duration::from_secs(300),
            token_budget: 32_000,
            capture_full_payloads: false,
        }
    }
}

impl<T: ModelClient + ?Sized> ModelClient for Box<T> {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        (**self).complete_turn(request)
    }

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        (**self).take_transport_telemetry()
    }
}

impl ModelDirective {
    pub(crate) fn target_state(&self) -> AgentLoopState {
        match self {
            Self::Think { .. } => AgentLoopState::Think,
            Self::Act { .. } => AgentLoopState::Act,
            Self::Done { .. } => AgentLoopState::Done,
        }
    }
}
