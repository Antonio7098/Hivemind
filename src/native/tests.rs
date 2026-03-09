#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::manual_assert)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::significant_drop_tightening)]

use super::*;
use crate::adapters::runtime::{
    ExecutionInput, NativePromptMetadata, NativeToolCallFailure, NativeToolCallTrace,
};
use crate::core::scope::{ExecutionScope, FilePermission, FilesystemScope, PathRule, Scope};
use crate::native::tool_engine::{
    NativeApprovalCache, NativeApprovalPolicy, NativeCommandPolicy, NativeExecPolicyManager,
    NativeNetworkApprovalCache, NativeNetworkPolicy, NativeSandboxPolicy, NativeToolAction,
    NativeToolEngine, ToolExecutionContext,
};
use crate::native::turn_items::{
    TurnItemCorrelation, TurnItemKind, TurnItemOutcome, TurnItemProvenance,
};
use serde_json::json;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

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
fn agent_loop_allows_consecutive_think_directives() {
    let model = MockModelClient::from_outputs(vec![
        "THINK:still thinking".to_string(),
        "THINK planning step two".to_string(),
        "DONE:all good".to_string(),
    ]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let result = loop_harness
        .run("test prompt", None)
        .expect("expected repeated THINK to recover");

    assert_eq!(result.turns.len(), 3);
    assert_eq!(result.turns[0].to_state, AgentLoopState::Think);
    assert_eq!(result.turns[1].to_state, AgentLoopState::Think);
    assert_eq!(result.turns[2].to_state, AgentLoopState::Done);
}

#[test]
fn agent_loop_fails_loud_on_malformed_model_output() {
    let model = MockModelClient::from_outputs(vec![
        "oops not structured".to_string(),
        "still not valid".to_string(),
        "no directive here either".to_string(),
        "still drifting away".to_string(),
        "final malformed output".to_string(),
    ]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let err = loop_harness
        .run("test prompt", None)
        .expect_err("expected malformed output");
    let err_message = err.message();

    let NativeRuntimeError::MalformedModelOutput {
        ref raw_output,
        ref recovery_hint,
        ..
    } = err
    else {
        panic!("expected malformed output error");
    };
    assert_eq!(raw_output, "final malformed output");
    assert!(
        recovery_hint.contains("THINK/ACT/DONE"),
        "recovery hint should be explicit"
    );
    assert!(
        err_message.contains("Raw preview: final malformed output"),
        "error message should surface a raw preview"
    );
}

#[test]
fn agent_loop_recovers_after_one_malformed_model_output() {
    let model = MockModelClient::from_outputs(vec![
        "status=inspect_then_act".to_string(),
        "ACT tool:read_file:{\"path\":\"note.txt\"}".to_string(),
        "DONE all good".to_string(),
    ]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let result = loop_harness
        .run("test prompt", None)
        .expect("expected malformed output repair to recover");

    assert_eq!(result.turns.len(), 2);
    assert!(matches!(
        result.turns[0].directive,
        ModelDirective::Act { .. }
    ));
    assert!(result
        .history_items
        .iter()
        .any(|item| item.render_for_prompt().contains("controller_repair")));
    assert_eq!(result.final_summary.as_deref(), Some("all good"));
}

#[test]
fn parse_relaxed_infers_freeform_first_person_planning_as_think() {
    let directive = ModelDirective::parse_relaxed("I'll inspect the repo status first.")
        .expect("freeform planning text should infer THINK");

    assert_eq!(
        directive,
        ModelDirective::Think {
            message: "I'll inspect the repo status first.".to_string(),
        }
    );
}

#[test]
fn agent_loop_recovers_after_multiple_malformed_model_outputs() {
    let model = MockModelClient::from_outputs(vec![
        "status=inspect_query_pipeline".to_string(),
        "next_step=choose_read".to_string(),
        "ACT tool:read_file:{\"path\":\"src/tests/test_query_experience.py\"}".to_string(),
        "DONE feature path recovered".to_string(),
    ]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let result = loop_harness
        .run("test prompt", None)
        .expect("expected malformed output repair to recover after several retries");

    assert_eq!(result.turns.len(), 2);
    assert!(
        result
            .history_items
            .iter()
            .filter(|item| item.render_for_prompt().contains("controller_repair"))
            .count()
            >= 2
    );
    assert_eq!(
        result.final_summary.as_deref(),
        Some("feature path recovered")
    );
}

#[test]
fn agent_loop_compacts_history_to_avoid_token_budget_overflow() {
    let model = MockModelClient::from_outputs(vec![
        "ACT:tool:read_file:{\"path\":\"note.txt\"}".to_string(),
        "DONE:budget recovered".to_string(),
    ]);
    let mut config = NativeRuntimeConfig::default();
    config.token_budget = 1_000;
    config.prompt_headroom = 64;
    let mut loop_harness = AgentLoop::new(config, model);
    let large_tool_response = "A".repeat(1_600);

    let result = loop_harness
        .run_with_history(
            "inv-budget",
            vec![user_input_item(
                "inv-budget",
                1,
                "objective",
                "Investigate note.txt and summarize the result".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let prompt = history
                    .iter()
                    .filter(|item| item.model_visible)
                    .map(TurnItem::render_for_prompt)
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: AgentMode::TaskExecutor,
                    prompt,
                    context: None,
                    prompt_assembly: None,
                })
            },
            move |_turn_index, action| {
                vec![crate::adapters::runtime::NativeToolCallTrace {
                    call_id: "call-1".to_string(),
                    tool_name: "read_file".to_string(),
                    request: action.to_string(),
                    duration_ms: Some(1),
                    response: Some(large_tool_response.clone()),
                    response_original_bytes: Some(large_tool_response.len()),
                    response_stored_bytes: Some(large_tool_response.len()),
                    response_truncated: false,
                    failure: None,
                    policy_tags: Vec::new(),
                }]
            },
        )
        .expect("expected budget recovery to complete");

    assert_eq!(result.final_summary.as_deref(), Some("budget recovered"));
    assert!(result.history_items.iter().any(|item| {
        matches!(item.kind, TurnItemKind::CompactedSummary { .. })
            && item.provenance.source == "runtime.compacted_summary"
    }));
}

#[test]
fn agent_loop_executes_tools_inside_the_model_loop() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("note.txt"), "hello from tool\n").expect("seed file");

    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "ACT:tool:read_file:{\"path\":\"note.txt\"}".to_string(),
            "DONE:all good".to_string(),
        ],
        prompts.clone(),
    );

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let input = native_input(None);
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-1",
            vec![user_input_item(
                "inv-1",
                1,
                "objective",
                "read note.txt".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-1:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should complete");

    assert_eq!(result.turns.len(), 2);
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert!(result.turns[0].tool_calls[0]
        .response
        .as_deref()
        .is_some_and(|response| response.contains("hello from tool")));

    let captured = prompts.lock().expect("prompts lock");
    assert_eq!(captured.len(), 2);
    assert!(captured[1].contains("tool_result:read_file"));
    assert!(captured[1].contains("hello from tool"));
    let second_assembly = result.turns[1]
        .request
        .prompt_assembly
        .as_ref()
        .expect("second turn assembly");
    assert_eq!(second_assembly.tool_result_items_visible, 1);
    assert_eq!(second_assembly.latest_tool_result_turn_index, Some(0));
    assert!(second_assembly
        .latest_tool_names_visible
        .iter()
        .any(|tool| tool == "read_file"));
}

#[test]
fn prompt_assembly_reports_lane_accounting_and_overflow_state() {
    let mut config = NativeRuntimeConfig::default();
    config.token_budget = 700;
    config.prompt_headroom = 96;
    let input = native_input(None);
    let engine = NativeToolEngine::default();
    let contracts = engine.contracts_for_mode(config.agent_mode);
    let history = vec![
        user_input_item(
            "inv-overflow",
            0,
            "objective",
            "Investigate a large tool output and summarize the important findings".to_string(),
            "test",
        ),
        TurnItem {
            id: "inv-overflow:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-overflow".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-1".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "A".repeat(1_800),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert!(rendered.assembly.objective_state_chars > 0);
    assert_ne!(rendered.assembly.overflow_classification, "within_budget");
}

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

#[test]
fn assemble_native_prompt_deduplicates_repeated_identical_read_results() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let repeated_content = "fn same_file_body() {}".repeat(400);
    let history = vec![
        user_input_item(
            "inv-repeat-read",
            0,
            "objective",
            "Inspect the same file twice".to_string(),
            "test",
        ),
        assistant_item(
            "inv-repeat-read",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-repeat-read:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-repeat-read".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-1".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-1".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-repeat-read:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-repeat-read".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-1".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: repeated_content.clone(),
            },
        },
        assistant_item(
            "inv-repeat-read",
            1,
            4,
            &ModelDirective::Think {
                message: "Read it again to confirm nothing changed".to_string(),
            },
        ),
        assistant_item(
            "inv-repeat-read",
            2,
            5,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-repeat-read:2:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-repeat-read".to_string(),
                turn_index: Some(2),
                source: "tool.call".to_string(),
                reference: Some("call-read-2".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-2".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-repeat-read:2:7".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 7,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-repeat-read".to_string(),
                turn_index: Some(2),
                source: "tool.result".to_string(),
                reference: Some("call-read-2".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-2".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: repeated_content.clone(),
            },
        },
        assistant_item(
            "inv-repeat-read",
            3,
            8,
            &ModelDirective::Think {
                message: "Proceed with the latest context only".to_string(),
            },
        ),
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.prompt.matches(&repeated_content).count(), 1);
    assert_eq!(rendered.assembly.tool_result_items_visible, 0);
    assert_eq!(rendered.assembly.active_code_window_count, 1);
}

#[test]
fn assemble_native_prompt_summarizes_large_write_payloads() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let large_content = "let rewritten = true;\n".repeat(512);
    let assistant_action = format!(
        "tool:write_file:{{\"path\":\"src/native/tests.rs\",\"content\":{content:?},\"append\":false}}",
        content = large_content,
    );
    let tool_request = format!(
        "{{\"path\":\"src/native/tests.rs\",\"content\":{content:?},\"append\":false}}",
        content = large_content,
    );
    let history = vec![
        user_input_item(
            "inv-write-summary",
            0,
            "objective",
            "Rewrite a file through the write tool".to_string(),
            "test",
        ),
        assistant_item(
            "inv-write-summary",
            0,
            1,
            &ModelDirective::Act {
                action: assistant_action,
            },
        ),
        TurnItem {
            id: "inv-write-summary:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-write-summary".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-write-1".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-1".to_string(),
                tool_name: "write_file".to_string(),
                request: tool_request,
            },
        },
        TurnItem {
            id: "inv-write-summary:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-write-summary".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-write-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-1".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "wrote file".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.prompt.matches(&large_content).count(), 1);
    assert!(rendered.prompt.contains("Active Code Windows"));
    assert!(rendered.prompt.contains("[code_window:dirty:write_file]"));
    assert!(rendered.prompt.contains("path=src/native/tests.rs"));
    assert!(rendered.prompt.contains("call_id=call-write-1"));
    assert!(!rendered.prompt.contains("[tool_call:write_file]"));
    assert!(!rendered.prompt.contains("[assistant:act] tool:write_file:"));
}

#[test]
fn assemble_native_prompt_unifies_read_and_write_into_active_code_windows() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let initial_content = "fn before() {}\n".repeat(64);
    let rewritten_content = "fn after() {}\n".repeat(64);
    let tool_request = format!(
        "{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
        content = rewritten_content,
    );
    let history = vec![
        user_input_item(
            "inv-windowed",
            0,
            "objective",
            "Open and then rewrite a file".to_string(),
            "test",
        ),
        assistant_item(
            "inv-windowed",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-windowed:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-windowed".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-windowed".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-windowed".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-windowed:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-windowed".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-windowed".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-windowed".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: initial_content,
            },
        },
        assistant_item(
            "inv-windowed",
            1,
            4,
            &ModelDirective::Act {
                action: format!(
                    "tool:write_file:{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = rewritten_content,
                ),
            },
        ),
        TurnItem {
            id: "inv-windowed:1:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-windowed".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-write-windowed".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-windowed".to_string(),
                tool_name: "write_file".to_string(),
                request: tool_request,
            },
        },
        TurnItem {
            id: "inv-windowed:1:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-windowed".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-write-windowed".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-windowed".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "wrote file".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.assembly.active_code_window_count, 1);
    assert!(rendered.prompt.contains("Active Code Windows"));
    assert!(rendered.prompt.contains("[code_window:dirty:write_file]"));
    assert!(rendered.prompt.contains("call_id=call-write-windowed"));
    assert!(rendered.prompt.contains("changed_lines=1-64"));
    assert!(rendered.prompt.contains(&rewritten_content));
    assert!(!rendered.prompt.contains("[tool_call:read_file]"));
    assert!(!rendered.prompt.contains("[tool_call:write_file]"));
}

#[test]
fn active_code_window_budget_prefers_dirty_windows_under_pressure() {
    let config = NativeRuntimeConfig {
        token_budget: 3_000,
        prompt_headroom: 1_000,
        ..NativeRuntimeConfig::default()
    };
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let clean_content = "fn clean() {}\n".repeat(40);
    let dirty_content = "fn dirty() {}\n".repeat(40);
    let history = vec![
        user_input_item(
            "inv-window-budget",
            0,
            "objective",
            "Keep only the most relevant code window".to_string(),
            "test",
        ),
        assistant_item(
            "inv-window-budget",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/tests.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-window-budget:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-window-budget".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-budget".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-budget".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/tests.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-window-budget:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-window-budget".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-budget".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-budget".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: clean_content,
            },
        },
        assistant_item(
            "inv-window-budget",
            1,
            4,
            &ModelDirective::Act {
                action: format!(
                    "tool:write_file:{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = dirty_content,
                ),
            },
        ),
        TurnItem {
            id: "inv-window-budget:1:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-window-budget".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-write-budget".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-budget".to_string(),
                tool_name: "write_file".to_string(),
                request: format!(
                    "{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = dirty_content,
                ),
            },
        },
        TurnItem {
            id: "inv-window-budget:1:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-window-budget".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-write-budget".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-budget".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "wrote file".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.assembly.active_code_window_count, 1);
    assert!(rendered
        .prompt
        .contains("path=src/native/prompt_assembly.rs"));
    assert!(!rendered.prompt.contains("path=src/native/tests.rs"));
}

#[test]
#[cfg(unix)]
fn agent_loop_repairs_done_until_checkpoint_complete_succeeds() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "DONE:ready to finish".to_string(),
            "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"done\"}"
                .to_string(),
            "DONE:all good".to_string(),
        ],
        prompts.clone(),
    );

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-checkpoint",
            vec![user_input_item(
                "inv-checkpoint",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-checkpoint:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should complete after checkpoint repair");

    assert_eq!(result.turns.len(), 2);
    assert!(matches!(
        result.turns[0].directive,
        ModelDirective::Act { .. }
    ));
    assert!(result.history_items.iter().any(|item| item
        .render_for_prompt()
        .contains("returned DONE before the active execution checkpoint was completed")));
    assert_eq!(result.final_summary.as_deref(), Some("all good"));
}

#[test]
#[cfg(unix)]
fn agent_loop_auto_completes_checkpoint_after_repeated_done_outputs() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "DONE:first summary".to_string(),
            "DONE:second summary".to_string(),
            "DONE:third summary".to_string(),
            "DONE:final summary".to_string(),
        ],
        prompts.clone(),
    );

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-checkpoint-auto",
            vec![user_input_item(
                "inv-checkpoint-auto",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-checkpoint-auto:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should auto-complete checkpoint after repeated DONE outputs");

    assert_eq!(result.turns.len(), 1);
    assert!(matches!(
        result.turns[0].directive,
        ModelDirective::Done { .. }
    ));
    assert_eq!(result.final_summary.as_deref(), Some("final summary"));
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert_eq!(
        result.turns[0].tool_calls[0].tool_name,
        "checkpoint_complete"
    );
    assert!(result.turns[0].tool_calls[0].failure.is_none());
}

#[test]
#[cfg(unix)]
fn agent_loop_accepts_done_after_checkpoint_was_already_completed() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let model = MockModelClient::from_outputs(vec![
        "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"finished work\"}"
            .to_string(),
        "DONE:all good".to_string(),
    ]);

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-checkpoint-done",
            vec![user_input_item(
                "inv-checkpoint-done",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-checkpoint-done:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should accept DONE after prior checkpoint completion");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("all good"));
    assert_eq!(result.turns.len(), 2);
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert_eq!(result.turns[1].tool_calls.len(), 0);
}

#[test]
#[cfg(unix)]
fn agent_loop_auto_finishes_after_redundant_checkpoint_completion() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let model = MockModelClient::from_outputs(vec![
        "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"finished work\"}"
            .to_string(),
        "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"finished work\"}"
            .to_string(),
    ]);

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-redundant-checkpoint",
            vec![user_input_item(
                "inv-redundant-checkpoint",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-redundant-checkpoint:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should auto-finish after redundant checkpoint completion");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("finished work"));
    assert_eq!(result.turns.len(), 2);
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert!(matches!(
        result.turns[1].directive,
        ModelDirective::Done { .. }
    ));
    assert_eq!(result.turns[1].tool_calls.len(), 0);
}

#[test]
#[cfg(unix)]
fn agent_loop_auto_finishes_after_post_checkpoint_non_done_loop() {
    let tmp = tempdir().expect("tempdir");
    #[cfg(unix)]
    fn write_executable(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, content).expect("write executable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
    let model = MockModelClient::from_outputs(vec![
        "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"finished work\"}"
            .to_string(),
        "THINK:double-checking final state".to_string(),
        "ACT:tool:list_files:{}".to_string(),
    ]);

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let script = tmp.path().join("fake-hivemind");
    write_executable(&script, "#!/bin/sh\nexit 0\n");
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-post-checkpoint-done",
            vec![user_input_item(
                "inv-post-checkpoint-done",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-post-checkpoint-done:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should auto-finish after post-checkpoint non-DONE outputs");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("finished work"));
    assert_eq!(result.turns.len(), 2);
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert!(matches!(
        result.turns[1].directive,
        ModelDirective::Done { .. }
    ));
    assert_eq!(result.turns[1].tool_calls.len(), 0);
}

#[test]
fn agent_loop_treats_checkpoint_already_completed_auto_completion_as_benign() {
    let model = MockModelClient::from_outputs(vec![
        "DONE:first summary".to_string(),
        "DONE:second summary".to_string(),
        "DONE:third summary".to_string(),
        "DONE:final summary".to_string(),
    ]);
    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let engine = NativeToolEngine::default();
    let allowed_contracts = engine.contracts_for_mode(config.agent_mode);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-checkpoint-benign",
            vec![user_input_item(
                "inv-checkpoint-benign",
                1,
                "objective",
                "complete the task".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &allowed_contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, _action| {
                vec![NativeToolCallTrace {
                    call_id: format!("inv-checkpoint-benign:turn:{turn_index}:tool:0"),
                    tool_name: "checkpoint_complete".to_string(),
                    request: "{}".to_string(),
                    duration_ms: None,
                    response: None,
                    response_original_bytes: None,
                    response_stored_bytes: None,
                    response_truncated: false,
                    failure: Some(NativeToolCallFailure {
                        code: "native_tool_execution_failed".to_string(),
                        message: "checkpoint completion failed with checkpoint_already_completed"
                            .to_string(),
                        recoverable: false,
                        policy_source: None,
                        denial_reason: None,
                        recovery_hint: None,
                    }),
                    policy_tags: Vec::new(),
                }]
            },
        )
        .expect("loop should treat already-completed checkpoint auto-completion as benign");

    assert_eq!(result.final_state, AgentLoopState::Done);
    assert_eq!(result.final_summary.as_deref(), Some("final summary"));
}

#[test]
fn prompt_assembly_is_deterministic_and_carries_manifest_hashes() {
    let mut config = NativeRuntimeConfig::default();
    config.token_budget = 512;
    config.prompt_headroom = 64;
    let input = native_input(Some(NativePromptMetadata {
        manifest_hash: Some("manifest-1".to_string()),
        inputs_hash: Some("inputs-1".to_string()),
        delivered_context_hash: Some("ctx-1".to_string()),
        rendered_context_hash: Some("ctx-rendered-1".to_string()),
        context_window_state_hash: Some("window-1".to_string()),
        delivery_target: Some("runtime_execution_input".to_string()),
        runtime_context_bytes: 42,
    }));
    let history = vec![
        user_input_item(
            "inv-2",
            1,
            "objective",
            "complete the task".to_string(),
            "test",
        ),
        user_input_item("inv-2", 2, "context", "x".repeat(400), "test"),
    ];
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);

    let first = assemble_native_prompt(&config, &input, &history, &contracts);
    let second = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(first.prompt, second.prompt);
    assert_eq!(
        first.assembly.rendered_prompt_hash,
        second.assembly.rendered_prompt_hash
    );
    assert_eq!(first.assembly.available_budget, 448);
    assert_eq!(first.assembly.manifest_hash.as_deref(), Some("manifest-1"));
    assert!(first.assembly.mode_contract_hash.len() >= 16);
    assert_eq!(first.assembly.runtime_context_bytes, 42);
    assert!(first.assembly.selected_item_count >= first.assembly.selected_history_count);
    assert_eq!(first.assembly.tool_contract_count, contracts.len());
    assert_eq!(
        first.assembly.delivered_context_hash.as_deref(),
        Some("ctx-1")
    );
    assert!(first.prompt.contains("Mode Contract"));
    assert!(first.prompt.contains("Context Manifest"));
    assert!(first.prompt.contains("input_schema="));
    assert!(first.prompt.contains("\"kind\""));
    assert!(first.prompt.contains("\"filter\""));
}

#[test]
fn task_executor_objective_state_stays_task_oriented_when_checkpoints_exist() {
    let config = NativeRuntimeConfig::default();
    let mut input = native_input(None);
    input.context = Some(
        "Execution checkpoints (in order): checkpoint-1\nComplete checkpoints from the runtime with the built-in tool: ACT:tool:checkpoint_complete:{\"id\":\"<checkpoint-id>\",\"summary\":\"optional progress summary\"}".to_string(),
    );
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);

    let rendered = assemble_native_prompt(&config, &input, &[], &contracts);

    assert!(rendered.assembly.objective_state.starts_with("task\n"));
    assert!(rendered
        .assembly
        .objective_state
        .contains("Checkpoint Handling: keep making task progress"));
}

#[test]
fn turn_item_normalization_marks_missing_and_orphaned_tool_history() {
    use super::turn_items::{
        TurnItemCorrelation, TurnItemKind, TurnItemOutcome, TurnItemProvenance,
    };

    let call_only = TurnItem {
        id: "inv-3:seed:item:1".to_string(),
        model_visible: true,
        correlation: TurnItemCorrelation {
            turn_index: Some(0),
            item_index: 1,
        },
        provenance: TurnItemProvenance {
            invocation_id: "inv-3".to_string(),
            turn_index: Some(0),
            source: "test".to_string(),
            reference: Some("call-1".to_string()),
        },
        kind: TurnItemKind::ToolCall {
            call_id: "call-1".to_string(),
            tool_name: "read_file".to_string(),
            request: "{}".to_string(),
        },
    };
    let orphan_result = TurnItem {
        id: "inv-3:seed:item:2".to_string(),
        model_visible: true,
        correlation: TurnItemCorrelation {
            turn_index: Some(1),
            item_index: 2,
        },
        provenance: TurnItemProvenance {
            invocation_id: "inv-3".to_string(),
            turn_index: Some(1),
            source: "test".to_string(),
            reference: Some("call-2".to_string()),
        },
        kind: TurnItemKind::ToolResult {
            call_id: "call-2".to_string(),
            tool_name: "list_files".to_string(),
            outcome: TurnItemOutcome::Success,
            content: "{}".to_string(),
        },
    };

    let normalized = super::turn_items::normalize_turn_items(&[call_only, orphan_result]);
    assert!(normalized.iter().any(|item| {
        matches!(
            item.kind,
            TurnItemKind::ToolResult {
                outcome: TurnItemOutcome::Missing,
                ..
            }
        )
    }));
    assert!(normalized.iter().any(|item| {
        matches!(
            item.kind,
            TurnItemKind::ToolCall {
                ref call_id,
                ref request,
                ..
            } if call_id == "call-2" && request.contains("missing_tool_call")
        )
    }));
}

#[test]
fn replayed_history_matches_recorded_history() {
    let model = MockModelClient::from_outputs(vec![
        "ACT:tool:list_files:{\"path\":\".\",\"recursive\":false}".to_string(),
        "DONE:done".to_string(),
    ]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let tmp = tempdir().expect("tempdir");
    let engine = NativeToolEngine::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = engine.contracts_for_mode(config.agent_mode);
    let result = loop_harness
        .run_with_history(
            "inv-4",
            vec![user_input_item(
                "inv-4",
                1,
                "objective",
                "list files".to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt(&config, &input, history, &contracts);
                Ok(ModelTurnRequest {
                    turn_index,
                    state,
                    agent_mode: config.agent_mode,
                    prompt: rendered.prompt,
                    context: input.context.clone(),
                    prompt_assembly: Some(rendered.assembly),
                })
            },
            |turn_index, action| {
                let tool_action = NativeToolAction::parse(action)
                    .expect("tool action parse")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-4:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should complete");

    let replayed = super::turn_items::replay_turn_history(&result);
    assert_eq!(replayed, result.history_items);
}

#[test]
fn planner_mode_denies_mutating_tools() {
    let tmp = tempdir().expect("tempdir");
    let engine = NativeToolEngine::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let write = NativeToolAction::parse(
        "tool:write_file:{\"path\":\"out.txt\",\"content\":\"hello\",\"append\":false}",
    )
    .expect("parse")
    .expect("tool action");

    let trace = engine.execute_action_trace_for_mode(
        AgentMode::Planner,
        "inv-5:turn:0:tool:0".to_string(),
        &write,
        &ctx,
    );

    assert_eq!(
        trace.failure.as_ref().map(|f| f.code.as_str()),
        Some("native_tool_mode_denied")
    );
    assert_eq!(
        trace
            .failure
            .as_ref()
            .and_then(|f| f.policy_source.as_deref()),
        Some("agent_mode_policy")
    );
    assert!(trace
        .failure
        .as_ref()
        .and_then(|f| f.denial_reason.as_deref())
        .is_some_and(|reason| reason.contains("planner")));
    assert!(trace
        .policy_tags
        .iter()
        .any(|tag| tag == "agent_mode:planner"));
}

#[test]
fn tool_action_parse_accepts_case_and_spacing_variants() {
    let action = NativeToolAction::parse(" TOOL :read_file:{\"path\":\"note.txt\"}")
        .expect("parse")
        .expect("tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(action.input, json!({"path": "note.txt"}));
}

#[test]
fn agent_loop_records_budget_pressure_and_timings() {
    let model = MockModelClient::from_outputs(vec!["DONE:done".to_string()]);
    let mut config = NativeRuntimeConfig::default();
    config.token_budget = 80;
    config.prompt_headroom = 1;
    let mut loop_harness = AgentLoop::new(config, model);
    let result = loop_harness
        .run(&"x".repeat(60), None)
        .expect("loop should complete");

    assert!(result.turns[0].budget_used_after >= result.turns[0].budget_used_before);
    assert!(!result.turns[0].budget_thresholds_crossed.is_empty());
    assert!(result.turns[0].turn_duration_ms >= result.turns[0].model_latency_ms);
}

fn native_input(native_prompt_metadata: Option<NativePromptMetadata>) -> ExecutionInput {
    ExecutionInput {
        task_description: "test task".to_string(),
        success_criteria: "done".to_string(),
        context: Some("test context".to_string()),
        prior_attempts: Vec::new(),
        verifier_feedback: None,
        native_prompt_metadata,
    }
}

fn allow_all_scope() -> Scope {
    Scope::new()
        .with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("*", FilePermission::Write)),
        )
        .with_execution(ExecutionScope::new().allow("*"))
}

fn test_tool_context<'a>(
    worktree: &'a Path,
    scope: &'a Scope,
    env: &'a HashMap<String, String>,
) -> ToolExecutionContext<'a> {
    let policy = NativeCommandPolicy::default();
    ToolExecutionContext {
        worktree,
        scope: Some(scope),
        sandbox_policy: NativeSandboxPolicy::default(),
        approval_policy: NativeApprovalPolicy::default(),
        network_policy: NativeNetworkPolicy::default(),
        command_policy: policy.clone(),
        exec_policy_manager: NativeExecPolicyManager {
            base: policy,
            ..NativeExecPolicyManager::default()
        },
        approval_cache: RefCell::new(NativeApprovalCache::default()),
        network_approval_cache: RefCell::new(NativeNetworkApprovalCache::default()),
        env,
    }
}

#[derive(Debug, Clone)]
struct RecordingModelClient {
    scripted: VecDeque<String>,
    prompts: Arc<Mutex<Vec<String>>>,
}

impl RecordingModelClient {
    fn new(scripted: Vec<String>, prompts: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            scripted: VecDeque::from(scripted),
            prompts,
        }
    }
}

impl ModelClient for RecordingModelClient {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        self.prompts
            .lock()
            .expect("prompts lock")
            .push(request.prompt.clone());
        Ok(self
            .scripted
            .pop_front()
            .unwrap_or_else(|| "DONE:recording model exhausted".to_string()))
    }
}
