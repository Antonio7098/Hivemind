use super::*;
use crate::adapters::runtime::NativeToolCallFailure;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const PROMPT_TOOL_PAYLOAD_PREVIEW_CHARS: usize = 160;

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
                let rendered = if directive == "act" {
                    summarize_assistant_action_for_prompt(content)
                        .unwrap_or_else(|| content.clone())
                } else {
                    content.clone()
                };
                format!("[assistant:{directive}] {rendered}")
            }
            TurnItemKind::ToolCall {
                tool_name, request, ..
            } => format!(
                "[tool_call:{tool_name}] {}",
                summarize_tool_request_for_prompt(tool_name, request)
                    .unwrap_or_else(|| request.clone())
            ),
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

fn summarize_assistant_action_for_prompt(action: &str) -> Option<String> {
    let mut parts = action.splitn(3, ':');
    let prefix = parts.next()?;
    if prefix != "tool" {
        return None;
    }
    let tool_name = parts.next()?;
    let payload = parts.next().unwrap_or_default();
    summarize_tool_request_for_prompt(tool_name, payload)
        .map(|summary| format!("tool:{tool_name} {summary}"))
}

fn summarize_tool_request_for_prompt(tool_name: &str, request: &str) -> Option<String> {
    let request_chars = request.chars().count();
    let request_digest = short_digest(request.as_bytes());
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(request) {
        if let Some(summary) = summarize_known_tool_request(tool_name, &value, request_chars) {
            return Some(format!("{summary} request_digest={request_digest}"));
        }
    }
    (request_chars > PROMPT_TOOL_PAYLOAD_PREVIEW_CHARS)
        .then(|| format!("request_chars={request_chars} request_digest={request_digest}"))
}

fn summarize_known_tool_request(
    tool_name: &str,
    value: &serde_json::Value,
    request_chars: usize,
) -> Option<String> {
    match tool_name {
        "write_file" => Some(format!(
            "path={} append={} content_chars={} request_chars={request_chars}",
            json_string_field(value, "path").unwrap_or_else(|| "<unknown>".to_string()),
            json_bool_field(value, "append").unwrap_or(false),
            json_string_field(value, "content").map_or(0, |content| content.chars().count()),
        )),
        "str_replace_editor" => Some(format!(
            "path={} old_chars={} new_chars={} request_chars={request_chars}",
            json_string_field(value, "path").unwrap_or_else(|| "<unknown>".to_string()),
            json_string_field(value, "old_str").map_or(0, |content| content.chars().count()),
            json_string_field(value, "new_str").map_or(0, |content| content.chars().count()),
        )),
        "apply_patch" => Some(format!(
            "patch_chars={} request_chars={request_chars}",
            json_string_field(value, "patch").map_or(0, |content| content.chars().count()),
        )),
        "run_command" => Some(format!(
            "command={} request_chars={request_chars}",
            json_string_field(value, "command")
                .map(|command| truncate_with_marker(&command, PROMPT_TOOL_PAYLOAD_PREVIEW_CHARS))
                .unwrap_or_else(|| "<unknown>".to_string()),
        )),
        "retain_context" => Some(format!(
            "path={} turns={} floor={} request_chars={request_chars}",
            json_string_field(value, "path").unwrap_or_else(|| "<unknown>".to_string()),
            value
                .get("turns")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(3),
            json_string_field(value, "floor").unwrap_or_else(|| "graph_card".to_string()),
        )),
        _ => None,
    }
}

fn json_string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .or_else(|| value.get("input").and_then(|input| input.get(key)))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn json_bool_field(value: &serde_json::Value, key: &str) -> Option<bool> {
    value
        .get(key)
        .or_else(|| value.get("input").and_then(|input| input.get(key)))
        .and_then(serde_json::Value::as_bool)
}

fn short_digest(bytes: &[u8]) -> String {
    digest_hex(bytes).chars().take(12).collect()
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

fn render_graph_query_navigation_summary(trace: &NativeToolCallTrace, response: &str) -> String {
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(response) else {
        return truncate_with_marker(response, 512);
    };
    if payload
        .get("output_truncated")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        let original = payload
            .get("original_bytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default();
        let stored = payload
            .get("stored_bytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default();
        return format!(
            "GraphCode navigation trace truncated at record time (original_bytes={original}, stored_bytes={stored}). Re-run with tighter bounds if you need more detail."
        );
    }
    let Some(output) = payload.get("output") else {
        return truncate_with_marker(response, 512);
    };
    let Ok(result) =
        serde_json::from_value::<crate::core::graph_query::GraphQueryResult>(output.clone())
    else {
        return truncate_with_marker(response, 512);
    };
    let top_nodes = result
        .nodes
        .iter()
        .take(4)
        .map(|node| {
            node.path
                .clone()
                .or_else(|| Some(node.logical_key.clone()))
                .map(|path| format!("{}:{path}", node.node_class))
                .unwrap_or_else(|| format!("{}:{}", node.node_class, node.node_id))
        })
        .collect::<Vec<_>>();
    let mut summary = format!(
        "kind={} nodes={} edges={} truncated={} visited={}/{} fingerprint={}",
        result.query_kind,
        result.nodes.len(),
        result.edges.len(),
        result.truncated,
        result.cost.visited_nodes,
        result.cost.visited_edges,
        short_hash(&result.canonical_fingerprint),
    );
    if !top_nodes.is_empty() {
        summary.push_str(" top=[");
        summary.push_str(&top_nodes.join(", "));
        summary.push(']');
    }
    if result.query_kind == "python_query" {
        if let Some(repo_name) = result.python_repo_name.as_deref() {
            summary.push_str(&format!(" repo={repo_name}"));
        }
        if !result.selected_block_ids.is_empty() {
            summary.push_str(&format!(" selected={}", result.selected_block_ids.len()));
        }
        if let Some(python_result) = result.python_result.as_ref() {
            if let Some(preview) = summarize_python_query_result_preview(python_result) {
                summary.push_str(&preview);
            }
        }
        if let Some(usage) = result.python_usage.as_ref() {
            if let Some(operations) = usage
                .get("operation_count")
                .and_then(serde_json::Value::as_u64)
            {
                summary.push_str(&format!(" ops={operations}"));
            }
        }
        if let Some(stdout) = result.python_stdout.as_deref() {
            if !stdout.is_empty() {
                summary.push_str(" stdout=");
                summary.push_str(&truncate_with_marker(stdout, 96));
            }
        }
    }
    if trace.response_truncated {
        summary.push_str(" record_truncated=true");
    }
    truncate_with_marker(&summary, 512)
}

fn summarize_python_query_result_preview(result: &serde_json::Value) -> Option<String> {
    if let Some(ranked) = result.get("ranked").and_then(serde_json::Value::as_array) {
        let ranked_preview = ranked
            .iter()
            .take(2)
            .filter_map(|entry| {
                let logical_key = entry.get("logical_key")?.as_str()?;
                let logical_key = truncate_with_marker(logical_key, 160);
                let score = entry
                    .get("score")
                    .and_then(|value| value.as_i64().or_else(|| value.as_u64().map(|v| v as i64)));
                Some(match score {
                    Some(score) => format!("{logical_key}({score})"),
                    None => logical_key,
                })
            })
            .collect::<Vec<_>>();
        if !ranked_preview.is_empty() {
            return Some(format!(" ranked=[{}]", ranked_preview.join(", ")));
        }
    }

    for field in ["files", "symbols", "logical_keys"] {
        let preview = result
            .get(field)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .take(3)
            .map(|value| truncate_with_marker(value, 80))
            .collect::<Vec<_>>();
        if !preview.is_empty() {
            return Some(format!(" {field}=[{}]", preview.join(", ")));
        }
    }

    None
}

fn render_failed_tool_result_content(failure: &NativeToolCallFailure) -> String {
    let mut content = format!(
        "code={} recoverable={} message={}",
        failure.code, failure.recoverable, failure.message
    );
    if let Some(recovery_hint) = failure.recovery_hint.as_deref() {
        content.push_str(" recovery_hint=");
        content.push_str(recovery_hint);
    }
    content
}

fn short_hash(value: &str) -> String {
    value.chars().take(12).collect()
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
    if let Some(response) = trace.prompt_response.as_ref().or(trace.response.as_ref()) {
        let prompt_content = render_successful_tool_result_content(trace, response);
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
                outcome: TurnItemOutcome::Success,
                content: prompt_content,
            },
        });
        if trace.tool_name == "graph_query" {
            items.push(TurnItem {
                id: item_id(
                    invocation_id,
                    Some(turn.turn_index),
                    base_index.saturating_add(2),
                ),
                model_visible: false,
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
                    summary: render_graph_query_navigation_summary(trace, response),
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
                content: render_failed_tool_result_content(failure),
            },
        });
    }
    items
}

fn render_successful_tool_result_content(trace: &NativeToolCallTrace, response: &str) -> String {
    match trace.tool_name.as_str() {
        "graph_query" => render_graph_query_navigation_summary(trace, response),
        "list_files" => render_list_files_result_summary(response),
        "run_command" | "exec_command" => render_command_result_summary(response),
        "retain_context" => render_retain_context_result_summary(response),
        _ => response.to_string(),
    }
}

fn render_retain_context_result_summary(response: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(response) else {
        return truncate_with_marker(response, 256);
    };
    format!(
        "granted={} path={} turns={} floor={} reason={}",
        value
            .get("granted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>"),
        value.get("turns").and_then(Value::as_u64).unwrap_or(0),
        value
            .get("floor")
            .and_then(Value::as_str)
            .unwrap_or("graph_card"),
        value
            .get("reason")
            .and_then(Value::as_str)
            .map(|reason| truncate_with_marker(reason, 80))
            .unwrap_or_else(|| "<none>".to_string())
    )
}

fn render_list_files_result_summary(response: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(response) else {
        return truncate_with_marker(response, 512);
    };
    let output = value.get("output").and_then(Value::as_object);
    let base_path = output
        .and_then(|output| output.get("base_path"))
        .and_then(Value::as_str)
        .unwrap_or(".");
    let entries = output
        .and_then(|output| output.get("entries"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let dir_count = entries
        .iter()
        .filter(|entry| {
            entry
                .get("is_dir")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let file_count = entries.len().saturating_sub(dir_count);
    let preview = entries
        .iter()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .take(8)
        .collect::<Vec<_>>()
        .join(", ");
    let truncated = entries.len() > 8;
    format!(
        "ok=true base_path={} entries={} dirs={} files={} preview={}{}",
        base_path,
        entries.len(),
        dir_count,
        file_count,
        if preview.is_empty() {
            "(none)"
        } else {
            &preview
        },
        if truncated { ", …" } else { "" },
    )
}

fn render_command_result_summary(response: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(response) else {
        return truncate_with_marker(response, 512);
    };
    let output = value.get("output").and_then(Value::as_object);
    let exit_code = output
        .and_then(|output| output.get("exit_code"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let stdout = output
        .and_then(|output| output.get("stdout"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let stderr = output
        .and_then(|output| output.get("stderr"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let preview_source = if !stderr.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    let preview = truncate_with_marker(&preview_source.replace('\n', " | "), 200);
    format!(
        "ok=true exit_code={} stdout_chars={} stderr_chars={} preview={}",
        exit_code,
        stdout.chars().count(),
        stderr.chars().count(),
        if preview.is_empty() {
            "(none)"
        } else {
            &preview
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_failed_tool_result_content_includes_recovery_hint() {
        let failure = NativeToolCallFailure {
            code: "native_policy_violation".to_string(),
            message: "run_command blocked by execution policy: grep -r foo src".to_string(),
            recoverable: false,
            policy_source: None,
            denial_reason: None,
            recovery_hint: Some(
                "Use `rg` instead of `grep`, and prefer repository-relative paths under `src/`."
                    .to_string(),
            ),
        };

        let rendered = render_failed_tool_result_content(&failure);

        assert!(rendered.contains("code=native_policy_violation"));
        assert!(rendered.contains("message=run_command blocked by execution policy"));
        assert!(rendered.contains("recovery_hint=Use `rg` instead of `grep`"));
    }

    #[test]
    fn render_successful_list_files_tool_result_is_summarized_for_prompt() {
        let trace = NativeToolCallTrace {
            call_id: "call-list-files".to_string(),
            tool_name: "list_files".to_string(),
            request: "{\"path\":\"src/native\",\"recursive\":false}".to_string(),
            duration_ms: Some(12),
            response: None,
            prompt_response: None,
            response_original_bytes: None,
            response_stored_bytes: None,
            response_truncated: false,
            failure: None,
            policy_tags: Vec::new(),
        };

        let rendered = render_successful_tool_result_content(
            &trace,
            r#"{"ok":true,"output":{"base_path":"src/native","entries":[{"path":"src/native/prompt_assembly.rs","is_dir":false},{"path":"src/native/turn_items.rs","is_dir":false},{"path":"src/native/tests.rs","is_dir":false}]}}"#,
        );

        assert!(rendered.contains("base_path=src/native"));
        assert!(rendered.contains("entries=3"));
        assert!(rendered.contains("preview=src/native/prompt_assembly.rs"));
        assert!(!rendered.contains("\"entries\""));
    }

    #[test]
    fn render_successful_run_command_tool_result_is_summarized_for_prompt() {
        let trace = NativeToolCallTrace {
            call_id: "call-run-command".to_string(),
            tool_name: "run_command".to_string(),
            request: "{\"command\":[\"cargo\",\"test\"]}".to_string(),
            duration_ms: Some(24),
            response: None,
            prompt_response: None,
            response_original_bytes: None,
            response_stored_bytes: None,
            response_truncated: false,
            failure: None,
            policy_tags: Vec::new(),
        };

        let rendered = render_successful_tool_result_content(
            &trace,
            r#"{"ok":true,"output":{"exit_code":101,"stdout":"","stderr":"error: test failed\nhelp: re-run with --nocapture"}}"#,
        );

        assert!(rendered.contains("exit_code=101"));
        assert!(rendered.contains("stderr_chars="));
        assert!(rendered.contains("preview=error: test failed | help: re-run with --nocapture"));
        assert!(!rendered.contains("\"stderr\""));
    }
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

pub(crate) fn truncate_with_marker(input: &str, max_chars: usize) -> String {
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
