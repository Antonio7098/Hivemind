use super::*;

#[test]
fn budget_compaction_preserves_recent_turn_and_latest_tool_result_context() {
    let items = vec![
        user_input_item("inv-preserve", 0, "task", "Investigate".to_string(), "test"),
        assistant_item(
            "inv-preserve",
            0,
            1,
            &ModelDirective::Act {
                action: "read the file".to_string(),
            },
        ),
        TurnItem {
            id: "inv-preserve:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-preserve".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-0".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-0".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "old tool result".to_string(),
            },
        },
        assistant_item(
            "inv-preserve",
            1,
            3,
            &ModelDirective::Act {
                action: "run the tests".to_string(),
            },
        ),
        TurnItem {
            id: "inv-preserve:1:4".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 4,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-preserve".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-1".to_string(),
                tool_name: "run_command".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "latest tool result".to_string(),
            },
        },
        assistant_item(
            "inv-preserve",
            2,
            5,
            &ModelDirective::Think {
                message: "summarize what changed".to_string(),
            },
        ),
    ];

    let compacted = compact_history_for_budget_pressure("inv-preserve", 3, &items)
        .expect("expected compaction to produce a summarized history");

    assert!(compacted
        .iter()
        .any(|item| matches!(item.kind, TurnItemKind::CompactedSummary { .. })));
    assert!(compacted.iter().any(|item| {
        matches!(
            &item.kind,
            TurnItemKind::ToolResult { call_id, content, .. }
                if call_id == "call-1" && content == "latest tool result"
        )
    }));
    assert!(compacted.iter().any(|item| {
        item.provenance.turn_index == Some(2)
            && matches!(
                &item.kind,
                TurnItemKind::AssistantText { content, .. } if content == "summarize what changed"
            )
    }));
}

#[test]
fn budget_compaction_stabilizes_for_large_tool_result_history() {
    let mut history = vec![
        user_input_item(
            "inv-compact-stable",
            0,
            "objective",
            "Investigate note.txt and summarize the result".to_string(),
            "test",
        ),
        assistant_item(
            "inv-compact-stable",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"note.txt\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-compact-stable:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-compact-stable".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-1".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-1".to_string(),
                tool_name: "read_file".to_string(),
                request: "tool:read_file:{\"path\":\"note.txt\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-compact-stable:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-compact-stable".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-1".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "A".repeat(1_600),
            },
        },
    ];

    let mut compactions = 0;
    let mut snapshots = vec![history
        .iter()
        .filter(|item| item.model_visible)
        .map(TurnItem::render_for_prompt)
        .collect::<Vec<_>>()];
    while let Some(compacted) =
        compact_history_for_hard_budget_limit("inv-compact-stable", 1, &history)
    {
        if compacted == history {
            break;
        }
        history = compacted;
        compactions += 1;
        snapshots.push(
            history
                .iter()
                .filter(|item| item.model_visible)
                .map(TurnItem::render_for_prompt)
                .collect::<Vec<_>>(),
        );
        if compactions > 8 {
            panic!("history compaction did not stabilize in time: {snapshots:#?}");
        }
    }

    let prompt = history
        .iter()
        .filter(|item| item.model_visible)
        .map(TurnItem::render_for_prompt)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(compactions <= 3, "expected compaction to stabilize quickly");
    assert!(
        prompt.chars().count() < 1_000,
        "expected compacted prompt to fit budget"
    );
}
