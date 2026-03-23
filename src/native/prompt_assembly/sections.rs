use super::*;
use crate::native::tool_engine::ToolContract;
use crate::native::turn_items::TurnItem;
use sha2::{Digest, Sha256};

pub(super) fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(super) fn render_metadata_section(metadata: &NativePromptMetadata) -> String {
    section(
        "Context Manifest",
        &join_lines(
            &[
                optional_line("manifest_hash", metadata.manifest_hash.as_deref()),
                optional_line("inputs_hash", metadata.inputs_hash.as_deref()),
                optional_line(
                    "delivered_context_hash",
                    metadata.delivered_context_hash.as_deref(),
                ),
                optional_line(
                    "rendered_context_hash",
                    metadata.rendered_context_hash.as_deref(),
                ),
                optional_line(
                    "context_window_state_hash",
                    metadata.context_window_state_hash.as_deref(),
                ),
                optional_line("delivery_target", metadata.delivery_target.as_deref()),
                Some(format!(
                    "runtime_context_bytes={}",
                    metadata.runtime_context_bytes
                )),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
            "(no context manifest metadata)",
        ),
    )
}

pub(super) fn render_tool_contract(contract: &ToolContract) -> String {
    let permissions = contract
        .required_permissions
        .iter()
        .map(|permission| format!("{permission:?}").to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(",");
    let input_schema = serde_json::to_string(&contract.input_schema)
        .unwrap_or_else(|_| "{\"error\":\"input_schema_unavailable\"}".to_string());
    format!(
        "- {}@{} scope={} permissions={} cancellable={} timeout_ms={} input_schema={}",
        contract.name,
        contract.version,
        contract.required_scope,
        permissions,
        contract.cancellable,
        contract.timeout_ms,
        input_schema,
    )
}

pub(super) fn select_items_with_budget(items: &[TurnItem], budget: usize) -> NativePromptSelection {
    if budget == 0 {
        return NativePromptSelection {
            items: Vec::new(),
            skipped_item_count: items.iter().filter(|item| item.model_visible).count(),
            truncated_item_count: 0,
        };
    }

    let mut selected = Vec::new();
    let mut used = 0usize;
    let mut skipped_item_count = 0usize;
    let mut truncated_item_count = 0usize;
    for item in items.iter().rev().filter(|item| item.model_visible) {
        let rendered = item.render_for_prompt();
        let chars = rendered.chars().count();
        if used + chars <= budget {
            selected.push(item.clone());
            used += chars;
            continue;
        }
        if selected.is_empty() {
            let mut truncated = item.clone();
            truncated.apply_prompt_truncation(budget);
            if !truncated.render_for_prompt().trim().is_empty() {
                selected.push(truncated);
                truncated_item_count = 1;
            }
        }
        skipped_item_count += 1;
        break;
    }
    selected.reverse();
    NativePromptSelection {
        items: selected,
        skipped_item_count,
        truncated_item_count,
    }
}

#[allow(clippy::manual_map, clippy::option_if_let_else)]
fn optional_line(key: &str, value: Option<&str>) -> Option<String> {
    match value {
        Some(value) => Some(format!("{key}={value}")),
        None => None,
    }
}

pub(super) fn section(title: &str, body: &str) -> String {
    format!("{title}:\n{body}")
}

pub(super) fn join_lines(lines: &[String], fallback: &str) -> String {
    if lines.is_empty() {
        fallback.to_string()
    } else {
        lines.join("\n")
    }
}
