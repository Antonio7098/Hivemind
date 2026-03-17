use super::*;
use crate::adapters::runtime::NativeHistoryCompactionTrace;
use crate::native::{AgentLoopObserver, NativeRuntimeError};
use std::time::Instant;

pub(super) type ProgressEmitter<'a> = Box<dyn FnMut(String) -> Result<(), RuntimeError> + 'a>;

pub(super) fn compact_progress_value(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn progress_callback_error(error: &RuntimeError) -> NativeRuntimeError {
    NativeRuntimeError::ModelRequestFailed {
        code: "native_observability_callback_failed".to_string(),
        message: format!(
            "Native progress callback failed: {}",
            compact_progress_value(&error.message, 200)
        ),
        recoverable: false,
    }
}

pub(super) struct NativeProgressObserver<'a> {
    emit: Option<ProgressEmitter<'a>>,
    started_at: Instant,
    history_compactions: Vec<NativeHistoryCompactionTrace>,
}

impl<'a> NativeProgressObserver<'a> {
    pub(super) fn new(emit: Option<ProgressEmitter<'a>>) -> Self {
        Self {
            emit,
            started_at: Instant::now(),
            history_compactions: Vec::new(),
        }
    }

    pub(super) fn take_history_compactions(&mut self) -> Vec<NativeHistoryCompactionTrace> {
        std::mem::take(&mut self.history_compactions)
    }

    pub(super) fn emit_line(&mut self, line: impl Into<String>) -> Result<(), NativeRuntimeError> {
        let line = line.into();
        let line = if let Some(stripped) = line.strip_prefix("[native-progress] ") {
            format!(
                "[native-progress] elapsed_ms={} {stripped}",
                self.started_at.elapsed().as_millis()
            )
        } else {
            line
        };
        if let Some(emit) = self.emit.as_mut() {
            emit(format!("{line}\n")).map_err(|error| progress_callback_error(&error))?;
        }
        Ok(())
    }
}

impl AgentLoopObserver for NativeProgressObserver<'_> {
    fn on_turn_request_prepared(
        &mut self,
        request: &ModelTurnRequest,
    ) -> Result<(), NativeRuntimeError> {
        let assembly = request.prompt_assembly.as_ref();
        self.emit_line(format!(
            "[native-progress] stage=turn_request_prepared turn={} state={} prompt_bytes={} context_bytes={} prompt_headroom={} available_budget={} rendered_prompt_bytes={} runtime_context_bytes={} visible_items={} selected_history_count={} selected_history_chars={} compacted_summary_count={} compacted_summary_chars={} assembly_latency_ms={}",
            request.turn_index,
            request.state.as_str(),
            request.prompt.len(),
            request.context.as_ref().map_or(0, String::len),
            assembly.map_or(0, |value| value.prompt_headroom),
            assembly.map_or(0, |value| value.available_budget),
            assembly.map_or(0, |value| value.rendered_prompt_bytes),
            assembly.map_or(0, |value| value.runtime_context_bytes),
            assembly.map_or(0, |value| value.selected_item_count),
            assembly.map_or(0, |value| value.selected_history_count),
            assembly.map_or(0, |value| value.selected_history_chars),
            assembly.map_or(0, |value| value.compacted_summary_count),
            assembly.map_or(0, |value| value.compacted_summary_chars),
            assembly.map_or(0, |value| value.assembly_duration_ms),
        ))
    }

    fn on_model_request_started(
        &mut self,
        request: &ModelTurnRequest,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=model_request_started turn={} state={} agent_mode={}",
            request.turn_index,
            request.state.as_str(),
            request.agent_mode.as_str(),
        ))
    }

    fn on_model_response_received(
        &mut self,
        request: &ModelTurnRequest,
        response: &str,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=model_response_received turn={} response_bytes={} preview={}",
            request.turn_index,
            response.len(),
            compact_progress_value(response, 120),
        ))
    }

    fn on_model_request_failed(
        &mut self,
        request: &ModelTurnRequest,
        error: &NativeRuntimeError,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=model_request_failed turn={} code={} recoverable={} message={}",
            request.turn_index,
            error.code(),
            error.recoverable(),
            compact_progress_value(&error.message(), 160),
        ))
    }

    fn on_tool_action_started(
        &mut self,
        turn_index: u32,
        action: &str,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=tool_action_started turn={} action={}",
            turn_index,
            compact_progress_value(action, 160),
        ))
    }

    fn on_tool_action_completed(
        &mut self,
        turn_index: u32,
        tool_call_count: usize,
    ) -> Result<(), NativeRuntimeError> {
        self.emit_line(format!(
            "[native-progress] stage=tool_action_completed turn={turn_index} tool_call_count={tool_call_count}",
        ))
    }

    fn on_history_compacted(
        &mut self,
        turn_index: u32,
        reason: &str,
        rendered_prompt_bytes_before: usize,
        selected_history_count_before: usize,
        selected_history_chars_before: usize,
        visible_items_before: usize,
        visible_items_after: usize,
        prompt_tokens_before: usize,
        projected_budget_used: usize,
        token_budget: usize,
        elapsed_since_invocation_ms: u64,
    ) -> Result<(), NativeRuntimeError> {
        self.history_compactions.push(NativeHistoryCompactionTrace {
            turn_index,
            reason: reason.to_string(),
            rendered_prompt_bytes_before,
            selected_history_count_before,
            selected_history_chars_before,
            visible_items_before,
            visible_items_after,
            prompt_tokens_before,
            projected_budget_used,
            token_budget,
            elapsed_since_invocation_ms,
        });
        self.emit_line(format!(
            "[native-progress] stage=history_compacted turn={turn_index} reason={reason} rendered_prompt_bytes_before={rendered_prompt_bytes_before} selected_history_count_before={selected_history_count_before} selected_history_chars_before={selected_history_chars_before} visible_items_before={visible_items_before} visible_items_after={visible_items_after} prompt_tokens_before={prompt_tokens_before} projected_budget_used={projected_budget_used} token_budget={token_budget}",
        ))
    }
}
