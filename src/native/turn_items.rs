use super::*;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TurnItem {
    pub id: String,
    pub model_visible: bool,
    pub correlation: TurnItemCorrelation,
    pub provenance: TurnItemProvenance,
    pub kind: TurnItemKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TurnItemCorrelation {
    #[serde(default)]
    pub turn_index: Option<u32>,
    pub item_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TurnItemProvenance {
    pub invocation_id: String,
    #[serde(default)]
    pub turn_index: Option<u32>,
    pub source: String,
    #[serde(default)]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum TurnItemKind {
    UserInput {
        label: String,
        content: String,
    },
    AssistantText {
        directive: String,
        content: String,
    },
    ToolCall {
        call_id: String,
        tool_name: String,
        request: String,
    },
    ToolResult {
        call_id: String,
        tool_name: String,
        outcome: TurnItemOutcome,
        content: String,
    },
    CodeNavigation {
        call_id: String,
        tool_name: String,
        summary: String,
    },
    CompactedSummary {
        summary: String,
        source_item_ids: Vec<String>,
        summary_hash: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TurnItemOutcome {
    Success,
    Failure,
    Missing,
}

impl TurnItemKind {
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Self::UserInput { .. } => "user_input",
            Self::AssistantText { .. } => "assistant_text",
            Self::ToolCall { .. } => "tool_call",
            Self::ToolResult { .. } => "tool_result",
            Self::CodeNavigation { .. } => "code_navigation",
            Self::CompactedSummary { .. } => "compacted_summary",
        }
    }
}

impl TurnItem {
    pub(crate) fn render_for_prompt(&self) -> String {
        match &self.kind {
            TurnItemKind::UserInput { label, content } => format!("[user:{label}] {content}"),
            TurnItemKind::AssistantText { directive, content } => {
                format!("[assistant:{directive}] {content}")
            }
            TurnItemKind::ToolCall {
                tool_name, request, ..
            } => format!("[tool_call:{tool_name}] {request}"),
            TurnItemKind::ToolResult {
                tool_name,
                outcome,
                content,
                ..
            } => format!(
                "[tool_result:{tool_name}:{:?}] {}",
                outcome,
                content.replace('\n', " ")
            ),
            TurnItemKind::CodeNavigation {
                tool_name, summary, ..
            } => format!("[code_navigation:{tool_name}] {summary}"),
            TurnItemKind::CompactedSummary { summary, .. } => {
                format!("[summary] {}", summary.replace('\n', " "))
            }
        }
    }

    pub(crate) fn apply_prompt_truncation(&mut self, max_chars: usize) {
        match &mut self.kind {
            TurnItemKind::UserInput { content, .. }
            | TurnItemKind::AssistantText { content, .. }
            | TurnItemKind::ToolResult { content, .. } => {
                *content = truncate_with_marker(content, max_chars);
            }
            TurnItemKind::ToolCall { request, .. } => {
                *request = truncate_with_marker(request, max_chars);
            }
            TurnItemKind::CodeNavigation { summary, .. }
            | TurnItemKind::CompactedSummary { summary, .. } => {
                *summary = truncate_with_marker(summary, max_chars);
            }
        }
    }
}

pub(crate) fn normalize_turn_items(items: &[TurnItem]) -> Vec<TurnItem> {
    let mut normalized = Vec::new();
    let mut seen_calls = HashSet::new();
    let mut call_result_counts = HashMap::<String, usize>::new();
    let mut synthetic_index = 0u32;

    for item in items {
        match &item.kind {
            TurnItemKind::ToolCall { call_id, .. } => {
                seen_calls.insert(call_id.clone());
            }
            TurnItemKind::ToolResult {
                call_id, tool_name, ..
            } if !seen_calls.contains(call_id) => {
                synthetic_index = synthetic_index.saturating_add(1);
                normalized.push(synthetic_tool_call(
                    item,
                    synthetic_index,
                    call_id,
                    tool_name,
                ));
                seen_calls.insert(call_id.clone());
            }
            _ => {}
        }
        if let TurnItemKind::ToolResult { call_id, .. } = &item.kind {
            *call_result_counts.entry(call_id.clone()).or_default() += 1;
        }
        normalized.push(item.clone());
    }

    for item in items {
        if let TurnItemKind::ToolCall {
            call_id, tool_name, ..
        } = &item.kind
        {
            if call_result_counts.get(call_id).copied().unwrap_or_default() == 0 {
                synthetic_index = synthetic_index.saturating_add(1);
                normalized.push(synthetic_missing_tool_result(
                    item,
                    synthetic_index,
                    call_id,
                    tool_name,
                ));
            }
        }
    }

    normalized
}

#[cfg(test)]
pub(crate) fn replay_turn_history(result: &AgentLoopResult) -> Vec<TurnItem> {
    let mut history = result.initial_items.clone();
    for turn in &result.turns {
        history.push(assistant_item_from_turn(&result.invocation_id, turn));
        for tool_call in &turn.tool_calls {
            history.extend(items_from_tool_trace(
                &result.invocation_id,
                turn,
                tool_call,
            ));
        }
    }
    normalize_turn_items(&history)
}

pub(crate) fn compacted_summary_item(
    invocation_id: &str,
    item_index: u32,
    turn_index: Option<u32>,
    summary: String,
    source_item_ids: Vec<String>,
) -> TurnItem {
    let summary_hash = digest_hex(summary.as_bytes());
    TurnItem {
        id: item_id(invocation_id, turn_index, item_index),
        model_visible: true,
        correlation: TurnItemCorrelation {
            turn_index,
            item_index,
        },
        provenance: TurnItemProvenance {
            invocation_id: invocation_id.to_string(),
            turn_index,
            source: "runtime.compacted_summary".to_string(),
            reference: None,
        },
        kind: TurnItemKind::CompactedSummary {
            summary,
            source_item_ids,
            summary_hash,
        },
    }
}

pub(crate) fn compact_history_for_budget_pressure(
    invocation_id: &str,
    next_turn_index: u32,
    items: &[TurnItem],
) -> Option<Vec<TurnItem>> {
    compact_history_for_budget_mode(
        invocation_id,
        next_turn_index,
        items,
        BudgetCompactionMode::SoftPressure,
    )
}

pub(crate) fn compact_history_for_hard_budget_limit(
    invocation_id: &str,
    next_turn_index: u32,
    items: &[TurnItem],
) -> Option<Vec<TurnItem>> {
    compact_history_for_budget_mode(
        invocation_id,
        next_turn_index,
        items,
        BudgetCompactionMode::HardLimit,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BudgetCompactionMode {
    SoftPressure,
    HardLimit,
}

#[allow(clippy::too_many_lines)]
fn compact_history_for_budget_mode(
    invocation_id: &str,
    next_turn_index: u32,
    items: &[TurnItem],
    mode: BudgetCompactionMode,
) -> Option<Vec<TurnItem>> {
    const MAX_SUMMARY_CHARS: usize = 512;
    const RECENT_VISIBLE_ITEMS_TO_PIN: usize = 4;
    const MIN_COMPACTABLE_ITEMS_AFTER_OPTIONAL_PINNING: usize = 2;

    let visible_positions = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| item.model_visible.then_some(index))
        .collect::<Vec<_>>();
    if visible_positions.len() <= 2 {
        return None;
    }

    let mut pinned_positions = HashSet::new();
    for (index, item) in items.iter().enumerate() {
        if item.model_visible
            && item.provenance.turn_index.is_none()
            && matches!(
                item.kind,
                TurnItemKind::UserInput { .. } | TurnItemKind::CompactedSummary { .. }
            )
        {
            pinned_positions.insert(index);
        }
    }

    let mut optional_pin_groups = Vec::new();

    if mode == BudgetCompactionMode::SoftPressure {
        if let Some(latest_visible_turn_index) = items
            .iter()
            .filter(|item| item.model_visible)
            .filter_map(|item| item.provenance.turn_index)
            .max()
        {
            for (index, item) in items.iter().enumerate() {
                if item.model_visible
                    && item.provenance.turn_index == Some(latest_visible_turn_index)
                {
                    pinned_positions.insert(index);
                }
            }
        }

        if let Some(latest_tool_result_turn_index) = items
            .iter()
            .filter(|item| item.model_visible)
            .rev()
            .find_map(|item| match item.kind {
                TurnItemKind::ToolResult { .. } => item.provenance.turn_index,
                _ => None,
            })
        {
            for (index, item) in items.iter().enumerate() {
                if item.model_visible
                    && item.provenance.turn_index == Some(latest_tool_result_turn_index)
                {
                    pinned_positions.insert(index);
                }
            }
        }

        for index in items
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, item)| item.model_visible.then_some(index))
            .take(RECENT_VISIBLE_ITEMS_TO_PIN)
        {
            optional_pin_groups.push(vec![index]);
        }
    }

    for group in optional_pin_groups {
        let newly_pinned = group
            .into_iter()
            .filter(|index| !pinned_positions.contains(index))
            .collect::<Vec<_>>();
        if newly_pinned.is_empty() {
            continue;
        }

        let remaining_compactable_items = visible_positions
            .iter()
            .filter(|index| {
                !pinned_positions.contains(index)
                    && !newly_pinned.iter().any(|candidate| candidate == *index)
            })
            .count();
        if remaining_compactable_items >= MIN_COMPACTABLE_ITEMS_AFTER_OPTIONAL_PINNING {
            pinned_positions.extend(newly_pinned);
        }
    }

    let compactable_positions = visible_positions
        .into_iter()
        .filter(|index| !pinned_positions.contains(index))
        .collect::<Vec<_>>();
    if compactable_positions.is_empty() {
        return None;
    }

    let compacted_call_ids = compactable_positions
        .iter()
        .filter_map(|index| items.get(*index))
        .filter_map(|item| match &item.kind {
            TurnItemKind::ToolCall { call_id, .. }
            | TurnItemKind::ToolResult { call_id, .. }
            | TurnItemKind::CodeNavigation { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let compactable_positions = if compacted_call_ids.is_empty() {
        compactable_positions
    } else {
        items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let is_initially_compactable = compactable_positions.contains(&index);
                let is_related_tool_artifact = match &item.kind {
                    TurnItemKind::ToolCall { call_id, .. }
                    | TurnItemKind::ToolResult { call_id, .. }
                    | TurnItemKind::CodeNavigation { call_id, .. } => {
                        compacted_call_ids.contains(call_id)
                    }
                    _ => false,
                };
                (is_initially_compactable || is_related_tool_artifact).then_some(index)
            })
            .collect::<Vec<_>>()
    };

    let summary_lines = compactable_positions
        .iter()
        .filter_map(|index| items.get(*index))
        .map(|item| format!("- {}", truncate_with_marker(&item.render_for_prompt(), 120)))
        .collect::<Vec<_>>();
    let summary_text = truncate_with_marker(
        &format!(
            "Earlier runtime context was compacted to keep the active task within budget:\n{}",
            summary_lines.join("\n")
        ),
        MAX_SUMMARY_CHARS,
    );
    let source_item_ids = compactable_positions
        .iter()
        .filter_map(|index| items.get(*index))
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let first_compacted_position = *compactable_positions.first()?;
    let summary_item = compacted_summary_item(
        invocation_id,
        90_000u32.saturating_add(next_turn_index),
        None,
        summary_text,
        source_item_ids,
    );

    let mut compacted =
        Vec::with_capacity(items.len().saturating_sub(compactable_positions.len()) + 1);
    let compactable_set = compactable_positions.into_iter().collect::<HashSet<_>>();
    let mut inserted_summary = false;
    for (index, item) in items.iter().enumerate() {
        if index == first_compacted_position && !inserted_summary {
            compacted.push(summary_item.clone());
            inserted_summary = true;
        }
        if compactable_set.contains(&index) {
            continue;
        }
        compacted.push(item.clone());
    }

    Some(normalize_turn_items(&compacted))
}

pub(crate) fn user_input_item(
    invocation_id: &str,
    item_index: u32,
    label: &str,
    content: String,
    source: &str,
) -> TurnItem {
    TurnItem {
        id: item_id(invocation_id, None, item_index),
        model_visible: true,
        correlation: TurnItemCorrelation {
            turn_index: None,
            item_index,
        },
        provenance: TurnItemProvenance {
            invocation_id: invocation_id.to_string(),
            turn_index: None,
            source: source.to_string(),
            reference: None,
        },
        kind: TurnItemKind::UserInput {
            label: label.to_string(),
            content,
        },
    }
}

pub(crate) fn assistant_item(
    invocation_id: &str,
    turn_index: u32,
    item_index: u32,
    directive: &ModelDirective,
) -> TurnItem {
    let (directive_name, content) = match directive {
        ModelDirective::Think { message } => ("think", message.clone()),
        ModelDirective::Act { action } => ("act", action.clone()),
        ModelDirective::Done { summary } => ("done", summary.clone()),
    };
    TurnItem {
        id: item_id(invocation_id, Some(turn_index), item_index),
        model_visible: true,
        correlation: TurnItemCorrelation {
            turn_index: Some(turn_index),
            item_index,
        },
        provenance: TurnItemProvenance {
            invocation_id: invocation_id.to_string(),
            turn_index: Some(turn_index),
            source: "model.directive".to_string(),
            reference: None,
        },
        kind: TurnItemKind::AssistantText {
            directive: directive_name.to_string(),
            content,
        },
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn items_from_tool_trace(
    invocation_id: &str,
    turn: &AgentLoopTurn,
    trace: &NativeToolCallTrace,
) -> Vec<TurnItem> {
    let mut items = Vec::new();
    let base_index = turn.turn_index.saturating_mul(10).saturating_add(2);
    items.push(TurnItem {
        id: item_id(invocation_id, Some(turn.turn_index), base_index),
        model_visible: true,
        correlation: TurnItemCorrelation {
            turn_index: Some(turn.turn_index),
            item_index: base_index,
        },
        provenance: TurnItemProvenance {
            invocation_id: invocation_id.to_string(),
            turn_index: Some(turn.turn_index),
            source: "tool.call".to_string(),
            reference: Some(trace.call_id.clone()),
        },
        kind: TurnItemKind::ToolCall {
            call_id: trace.call_id.clone(),
            tool_name: trace.tool_name.clone(),
            request: trace.request.clone(),
        },
    });
    if let Some(response) = trace.response.as_ref() {
        items.push(TurnItem {
            id: item_id(
                invocation_id,
                Some(turn.turn_index),
                base_index.saturating_add(1),
            ),
            model_visible: trace.tool_name != "graph_query",
            correlation: TurnItemCorrelation {
                turn_index: Some(turn.turn_index),
                item_index: base_index.saturating_add(1),
            },
            provenance: TurnItemProvenance {
                invocation_id: invocation_id.to_string(),
                turn_index: Some(turn.turn_index),
                source: "tool.result".to_string(),
                reference: Some(trace.call_id.clone()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: trace.call_id.clone(),
                tool_name: trace.tool_name.clone(),
                outcome: TurnItemOutcome::Success,
                content: response.clone(),
            },
        });
        if trace.tool_name == "graph_query" {
            items.push(TurnItem {
                id: item_id(
                    invocation_id,
                    Some(turn.turn_index),
                    base_index.saturating_add(2),
                ),
                model_visible: true,
                correlation: TurnItemCorrelation {
                    turn_index: Some(turn.turn_index),
                    item_index: base_index.saturating_add(2),
                },
                provenance: TurnItemProvenance {
                    invocation_id: invocation_id.to_string(),
                    turn_index: Some(turn.turn_index),
                    source: "tool.code_navigation".to_string(),
                    reference: Some(trace.call_id.clone()),
                },
                kind: TurnItemKind::CodeNavigation {
                    call_id: trace.call_id.clone(),
                    tool_name: trace.tool_name.clone(),
                    summary: truncate_with_marker(response, 512),
                },
            });
        }
    }
    if let Some(failure) = trace.failure.as_ref() {
        items.push(TurnItem {
            id: item_id(
                invocation_id,
                Some(turn.turn_index),
                base_index.saturating_add(1),
            ),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(turn.turn_index),
                item_index: base_index.saturating_add(1),
            },
            provenance: TurnItemProvenance {
                invocation_id: invocation_id.to_string(),
                turn_index: Some(turn.turn_index),
                source: "tool.result".to_string(),
                reference: Some(trace.call_id.clone()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: trace.call_id.clone(),
                tool_name: trace.tool_name.clone(),
                outcome: TurnItemOutcome::Failure,
                content: format!(
                    "code={} recoverable={} message={}",
                    failure.code, failure.recoverable, failure.message
                ),
            },
        });
    }
    items
}

#[cfg(test)]
fn assistant_item_from_turn(invocation_id: &str, turn: &AgentLoopTurn) -> TurnItem {
    assistant_item(
        invocation_id,
        turn.turn_index,
        turn.turn_index.saturating_mul(10).saturating_add(1),
        &turn.directive,
    )
}

fn synthetic_tool_call(
    item: &TurnItem,
    item_index: u32,
    call_id: &str,
    tool_name: &str,
) -> TurnItem {
    TurnItem {
        id: format!(
            "{}:normalized-call:{item_index}",
            item.provenance.invocation_id
        ),
        model_visible: false,
        correlation: TurnItemCorrelation {
            turn_index: item.correlation.turn_index,
            item_index,
        },
        provenance: TurnItemProvenance {
            invocation_id: item.provenance.invocation_id.clone(),
            turn_index: item.correlation.turn_index,
            source: "runtime.normalization".to_string(),
            reference: Some(call_id.to_string()),
        },
        kind: TurnItemKind::ToolCall {
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            request: "{\"normalized\":\"missing_tool_call\"}".to_string(),
        },
    }
}

fn synthetic_missing_tool_result(
    item: &TurnItem,
    item_index: u32,
    call_id: &str,
    tool_name: &str,
) -> TurnItem {
    TurnItem {
        id: format!(
            "{}:normalized-result:{item_index}",
            item.provenance.invocation_id
        ),
        model_visible: true,
        correlation: TurnItemCorrelation {
            turn_index: item.correlation.turn_index,
            item_index,
        },
        provenance: TurnItemProvenance {
            invocation_id: item.provenance.invocation_id.clone(),
            turn_index: item.correlation.turn_index,
            source: "runtime.normalization".to_string(),
            reference: Some(call_id.to_string()),
        },
        kind: TurnItemKind::ToolResult {
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            outcome: TurnItemOutcome::Missing,
            content: "tool result unavailable during replay".to_string(),
        },
    }
}

fn item_id(invocation_id: &str, turn_index: Option<u32>, item_index: u32) -> String {
    turn_index.map_or_else(
        || format!("{invocation_id}:seed:item:{item_index}"),
        |turn_index| format!("{invocation_id}:turn:{turn_index}:item:{item_index}"),
    )
}

fn truncate_with_marker(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = input.chars().count();
    if count <= max_chars {
        return input.to_string();
    }
    let keep = max_chars.saturating_sub(14);
    let mut truncated = input.chars().take(keep).collect::<String>();
    truncated.push_str("...(truncated)");
    truncated
}

fn digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
