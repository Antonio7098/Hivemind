use super::*;
use crate::adapters::runtime::NativeTransportTelemetry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryCompactionEvent {
    pub turn_index: u32,
    pub reason: String,
    pub rendered_prompt_bytes_before: usize,
    pub selected_history_count_before: usize,
    pub selected_history_chars_before: usize,
    pub visible_items_before: usize,
    pub visible_items_after: usize,
    pub prompt_tokens_before: usize,
    pub projected_budget_used: usize,
    pub token_budget: usize,
    pub elapsed_since_invocation_ms: u64,
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

    fn on_history_compacted(
        &mut self,
        _event: &HistoryCompactionEvent,
    ) -> Result<(), NativeRuntimeError> {
        Ok(())
    }
}
