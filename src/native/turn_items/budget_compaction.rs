use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BudgetCompactionMode {
    SoftPressure,
    HardLimit,
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

// ARCH_DEBT: legacy oversized function
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
                if item.model_visible && item.provenance.turn_index == Some(latest_visible_turn_index)
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
