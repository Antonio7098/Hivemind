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
fn parse_relaxed_accepts_bracketed_tool_call_variants() {
    let directive = ModelDirective::parse_relaxed(
        "[tool_call:list_files:{\"include_hidden\":false,\"path\":\"src\",\"recursive\":true}]",
    )
    .expect("bracketed tool call should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action:
                "tool:list_files:{\"include_hidden\":false,\"path\":\"src\",\"recursive\":true}"
                    .to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_uppercase_bracketed_tool_call_with_shell_flags_after_think() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:I need to inspect the remaining files before editing.\n[TOOL_CALL]tool:read_file --path \"/tmp/hm-deep-prompt-provenance/home/hivemind/worktrees/flow/task/src/native/turn_items.rs\"[/TOOL_CALL]\n[TOOL_CALL]tool:list_files --path \"/tmp/hm-deep-prompt-provenance/home/hivemind/worktrees/flow/task/src/core/events\"[/TOOL_CALL]",
    )
    .expect("uppercase bracketed tool call should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"/tmp/hm-deep-prompt-provenance/home/hivemind/worktrees/flow/task/src/native/turn_items.rs\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_xml_named_tool_wrapper_after_think() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:Exploring the repository structure.\n</think>\n<tool:list_files>\n<path>/tmp/worktree/src</path>\n<recursive>false</recursive>\n</tool:list_files>",
    )
    .expect("xml named tool wrapper should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:list_files:{\"path\":\"/tmp/worktree/src\",\"recursive\":false}"
                .to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_bracketed_named_tool_wrapper_with_json_body_after_think() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:Exploring the repository structure.\n[tool:list_files]\n{\"path\":\"/tmp/worktree/src\",\"recursive\":false}\n[/tool]",
    )
    .expect("bracketed named tool wrapper should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:list_files:{\"path\":\"/tmp/worktree/src\",\"recursive\":false}"
                .to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_bracketed_named_tool_wrapper_with_args_without_closing_tag() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:Need turn_items.rs for structure.\n[tool:read_file]\n[args: {\n  --path \"src/native/turn_items.rs\"\n}\n[/tool_call]",
    )
    .expect("bracketed named tool wrapper with args should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_bracketed_tool_call_with_named_tool_and_json_body() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:Need telemetry.rs next.\n[tool_call:read_file] {\"path\": \"src/adapters/runtime/telemetry.rs\"}",
    )
    .expect("bracketed tool call with named tool and json body should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\": \"src/adapters/runtime/telemetry.rs\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_uppercase_bracketed_tool_call_without_closing_tag() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:Checking current state.\n[TOOL_CALL] {tool: \"read_file\", version: \"1.0.0\", input: {\"path\": \"src/native/prompt_assembly.rs\"}}",
    )
    .expect("uppercase bracketed tool call without closing tag should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:{tool: \"read_file\", version: \"1.0.0\", input: {\"path\": \"src/native/prompt_assembly.rs\"}}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_bracketed_assistant_act_variants() {
    let directive = ModelDirective::parse_relaxed(
        "[assistant:act] tool:read_file:{\"path\":\"src/core/runtime_event_projection/parser.rs\"}",
    )
    .expect("assistant act wrapper should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"src/core/runtime_event_projection/parser.rs\"}"
                .to_string(),
        }
    );
}

#[test]
fn parse_relaxed_skips_bracketed_assistant_act_metadata_and_uses_later_valid_act() {
    let directive = ModelDirective::parse_relaxed(
        "<think>continuing exploration</think>\n[assistant:act] tool:list_files request_chars=170 request_digest=891585b78d1b\n[tool_result:list_files:Success] {\"ok\":true}\nACT:tool:list_files:{\"path\":\"src/adapters/runtime\",\"recursive\":true}",
    )
    .expect("later valid ACT should be selected");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:list_files:{\"path\":\"src/adapters/runtime\",\"recursive\":true}"
                .to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_xml_tool_call_variants() {
    let directive = ModelDirective::parse_relaxed(
        r#"<tool_call> <function=ACT> <parameter=tool> read_file </parameter> <parameter=arguments> {"path":"note.txt"} </parameter> </tool_call>"#,
    )
    .expect("xml-style tool call should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"note.txt\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_xml_function_tool_variants() {
    let directive = ModelDirective::parse_relaxed(
        r#"<tool_call> <function=read_file:{"path":"src/core/events/payload.rs"}>"#,
    )
    .expect("xml function tool wrapper should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"src/core/events/payload.rs\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_leading_think_tag_freeform_reasoning() {
    let directive = ModelDirective::parse_relaxed("<think> Let me inspect the file list first")
        .expect("leading think tag should parse as THINK");

    assert_eq!(
        directive,
        ModelDirective::Think {
            message: "Let me inspect the file list first".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_think_preface_with_embedded_act_variant() {
    let directive = ModelDirective::parse_relaxed(
        "<think>Let me explore the prompt inputs first.</think> ACT tool:read_file {\"path\":\"src/native/prompt_assembly.rs\"}",
    )
    .expect("embedded ACT after think preface should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file {\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_prefers_minimax_tool_call_wrapper_after_leading_think() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:From the previous exploration, I can see NativeTurnSummaryRow already has several hash fields. Let me inspect the native events directory.\n<minimax:tool_call>\n<invoke name=\"list_files\">\n<parameter name=\"path\">src/core/registry/runtime/native_events</parameter>\n<parameter name=\"recursive\">false</parameter>\n</invoke>\n</minimax:tool_call>",
    )
    .expect("MiniMax wrapper after THINK should parse as ACT");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action:
                "tool:list_files:{\"path\":\"src/core/registry/runtime/native_events\",\"recursive\":false}"
                    .to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_minimax_tool_wrapper_with_native_tool_lines() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:I need to read the remaining target files before implementing the telemetry changes.\n<minimax:tool_call>\ntool:read_file:{\"path\":\"src/native/turn_items.rs\"}\n[/tool_call]",
    )
    .expect("MiniMax native tool wrapper should parse as ACT");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_minimax_tool_wrapper_with_inline_colon_native_tool_action() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:I've reviewed the existing codebase. Let me start by modifying turn_items.rs to add the provenance structure.\n<minimax:tool_call>:tool:read_file:{\"path\":\"src/native/turn_items.rs\"}",
    )
    .expect("MiniMax inline colon wrapper should parse as ACT");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_minimax_tool_wrapper_without_explicit_tool_prefix() {
    let directive = ModelDirective::parse_relaxed(
        "<minimax:tool_call>:read_file{\"path\":\"src/native/turn_items.rs\"}",
    )
    .expect("MiniMax wrapper without explicit tool prefix should parse as ACT");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
        }
    );
}

#[test]
fn parse_relaxed_accepts_minimax_tool_wrapper_with_inline_json_tag_tool_call() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:Checking the actual path structure.\n<minimax:tool_call>\n<tool_call>tool:list_files <json>{\"path\":\"src/core/registry\",\"recursive\":true}</tool_call>[/tool_call]",
    )
    .expect("MiniMax inline json-tag tool call should parse");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:list_files:{\"path\":\"src/core/registry\",\"recursive\":true}"
                .to_string(),
        }
    );
}

#[test]
fn parse_relaxed_recovers_wrapper_leaked_structured_tool_payload() {
    let directive = ModelDirective::parse_relaxed(
        r#"ACT:tool:ACT] {"tool":"write_file","args":{"path":"src/native/turn_items.rs","content":"patched","append":false}}[/tool_call]"#,
    )
    .expect("wrapper-leaked structured tool payload should parse");

    let ModelDirective::Act { action } = directive else {
        panic!("expected ACT directive");
    };
    let action = NativeToolAction::parse(&action)
        .expect("parse recovered action")
        .expect("tool action");

    assert_eq!(action.name, "write_file");
    assert_eq!(
        action.input,
        json!({"path": "src/native/turn_items.rs", "content": "patched", "append": false})
    );
}

#[test]
fn parse_relaxed_uses_first_valid_tool_from_minimax_multi_tool_wrapper() {
    let directive = ModelDirective::parse_relaxed(
        "THINK:I need to read the remaining target files in parallel.\n<minimax:tool_call>\ntool:read_file:{\"path\":\"src/native/turn_items.rs\"}\ntool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}\n[/tool_call]",
    )
    .expect("MiniMax multi-tool wrapper should yield the first valid ACT");

    assert_eq!(
        directive,
        ModelDirective::Act {
            action: "tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
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
fn agent_loop_repairs_excessive_noop_think_streaks() {
    let model = MockModelClient::from_outputs(vec![
        "THINK:inspect the task first".to_string(),
        "THINK:review likely files".to_string(),
        "THINK:confirm I need more context".to_string(),
        "THINK:one more planning step".to_string(),
        "ACT:tool:read_file:{\"path\":\"Cargo.toml\"}".to_string(),
        "DONE:recovered after acting".to_string(),
    ]);
    let mut loop_harness = AgentLoop::new(NativeRuntimeConfig::default(), model);
    let result = loop_harness
        .run("test prompt", None)
        .expect("expected no-op THINK repair to recover");

    assert_eq!(
        result.final_summary.as_deref(),
        Some("recovered after acting")
    );
    assert!(result.history_items.iter().any(|item| item
        .render_for_prompt()
        .contains("consecutive turns in THINK")));
    assert!(matches!(
        result
            .turns
            .iter()
            .find(|turn| matches!(turn.directive, ModelDirective::Act { .. })),
        Some(_)
    ));
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
                    prompt_response: None,
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
    let note_content = format!("hello from tool\n{}", "line from tool\n".repeat(1_200));
    fs::write(tmp.path().join("note.txt"), &note_content).expect("seed file");

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
    assert!(result.turns[0].tool_calls[0].response_truncated);
    assert!(result.turns[0].tool_calls[0]
        .response
        .as_deref()
        .is_some_and(|response| response.contains("hello from tool")));
    let prompt_response = result.turns[0].tool_calls[0]
        .prompt_response
        .as_deref()
        .expect("full prompt-visible tool response");
    let prompt_response = serde_json::from_str::<serde_json::Value>(prompt_response)
        .expect("prompt response should be valid json");
    assert_eq!(
        prompt_response
            .get("output")
            .and_then(|output| output.get("content"))
            .and_then(serde_json::Value::as_str),
        Some(note_content.as_str())
    );

    let captured = prompts.lock().expect("prompts lock");
    assert_eq!(captured.len(), 2);
    assert!(captured[1].contains("Active Code Windows"));
    assert!(captured[1].contains("[code_window:clean:read_file]"));
    assert!(!captured[1].contains("tool_result:read_file"));
    assert!(captured[1].contains("hello from tool"));
    let second_assembly = result.turns[1]
        .request
        .prompt_assembly
        .as_ref()
        .expect("second turn assembly");
    assert_eq!(second_assembly.active_code_window_count, 1);
    assert_eq!(second_assembly.tool_result_items_visible, 0);
    assert_eq!(second_assembly.latest_tool_result_turn_index, None);
    assert!(second_assembly.latest_tool_names_visible.is_empty());
    assert_eq!(second_assembly.active_code_window_trace.len(), 1);
    assert_eq!(second_assembly.active_code_window_trace[0].path, "note.txt");
    assert_eq!(
        second_assembly.active_code_window_trace[0].delivery_lane,
        "active_code_windows"
    );
    assert_eq!(
        second_assembly.active_code_window_trace[0].content_source,
        "decoded_output_content"
    );
}

#[test]
fn agent_loop_reads_write_target_before_allowing_write_file() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("note.txt"), "before\n").expect("seed file");

    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "ACT:tool:write_file:{\"path\":\"note.txt\",\"content\":\"after\\n\",\"append\":false}"
                .to_string(),
            "ACT:tool:write_file:{\"path\":\"note.txt\",\"content\":\"after\\n\",\"append\":false}"
                .to_string(),
            "DONE:wrote after reading".to_string(),
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
            "inv-write-guard",
            vec![user_input_item(
                "inv-write-guard",
                1,
                "objective",
                "update note.txt safely".to_string(),
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
                    format!("inv-write-guard:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should succeed");

    assert_eq!(result.turns.len(), 3);
    assert_eq!(result.turns[0].tool_calls.len(), 1);
    assert_eq!(result.turns[0].tool_calls[0].tool_name, "read_file");
    assert_eq!(result.turns[1].tool_calls.len(), 1);
    assert_eq!(result.turns[1].tool_calls[0].tool_name, "write_file");
    assert_eq!(
        fs::read_to_string(tmp.path().join("note.txt")).expect("read updated file"),
        "after\n"
    );

    let second_prompt = result.turns[1]
        .request
        .prompt_assembly
        .as_ref()
        .expect("second prompt assembly");
    assert!(second_prompt
        .active_code_windows
        .iter()
        .any(|item| item.item_id == "code-window:note.txt"));

    let captured = prompts.lock().expect("prompts lock");
    assert_eq!(captured.len(), 3);
    assert!(captured[1].contains("path=note.txt"));
}

#[test]
fn agent_loop_surfaces_graph_query_results_into_the_next_prompt() {
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "ACT:tool:graph_query:{\"kind\":\"filter\",\"path_prefix\":\"src/native\",\"max_results\":4}".to_string(),
            "DONE:graph query observed".to_string(),
        ],
        prompts.clone(),
    );

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let mut loop_harness = AgentLoop::new(config.clone(), model);
    let graph_query_response = json!({
        "output": {
            "query_kind": "python_query",
            "canonical_fingerprint": "abc123def4567890",
            "max_results": 4,
            "truncated": false,
            "duration_ms": 7,
            "cost": {"visited_nodes": 3, "visited_edges": 1},
            "nodes": [{
                "node_id": "node-1",
                "repo_name": "hivemind",
                "logical_key": "src/native/tool_engine.rs",
                "node_class": "file",
                "path": "src/native/tool_engine.rs",
                "partition": "code"
            }],
            "edges": [],
            "selected_block_ids": ["block-1"],
            "python_repo_name": "hivemind",
            "python_result": {
                "ranked": [
                    {
                        "logical_key": "symbol:src/native/tests.rs::graph_query_prompt_visibility",
                        "score": 2
                    },
                    {
                        "logical_key": "symbol:tests/integration.rs::graph_query_filter_results",
                        "score": 1
                    }
                ]
            },
            "python_usage": {"operation_count": 2},
            "python_stdout": "matched native runtime nodes"
        }
    })
    .to_string();

    let result = loop_harness
        .run_with_history(
            "inv-graph-query-visible",
            vec![user_input_item(
                "inv-graph-query-visible",
                0,
                "objective",
                "inspect native graph runtime state".to_string(),
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
            move |_turn_index, action| {
                vec![NativeToolCallTrace {
                    call_id: "call-graph-query-1".to_string(),
                    tool_name: "graph_query".to_string(),
                    request: action.to_string(),
                    duration_ms: Some(7),
                    response: Some(graph_query_response.clone()),
                    prompt_response: None,
                    response_original_bytes: Some(graph_query_response.len()),
                    response_stored_bytes: Some(graph_query_response.len()),
                    response_truncated: false,
                    failure: None,
                    policy_tags: Vec::new(),
                }]
            },
        )
        .expect("loop should complete");

    assert_eq!(result.turns.len(), 2);

    let captured = prompts.lock().expect("prompts lock");
    assert_eq!(captured.len(), 2);
    assert!(captured[1].contains("tool_result:graph_query"));
    assert!(captured[1].contains("kind=python_query nodes=1 edges=0"));
    assert!(captured[1].contains("graph_query_prompt_visibility(2)"));
    assert!(captured[1].contains("graph_query_filter_results(1)"));
    assert!(!captured[1].contains("Code Navigation Session"));
    let second_assembly = result.turns[1]
        .request
        .prompt_assembly
        .as_ref()
        .expect("second turn assembly");
    assert_eq!(second_assembly.tool_result_items_visible, 1);
    assert_eq!(second_assembly.code_navigation_count, 0);
    assert_eq!(second_assembly.latest_tool_result_turn_index, Some(0));
    assert!(second_assembly
        .latest_tool_names_visible
        .iter()
        .any(|tool| tool == "graph_query"));
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

    assert!(rendered
        .prompt
        .contains("[code_window:clean:read_file] path=src/native/prompt_assembly.rs"));
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
fn assemble_native_prompt_surfaces_context_lease_metadata_on_active_windows() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let history = vec![
        user_input_item(
            "inv-context-lease",
            0,
            "objective",
            "Keep a file available while finishing a refactor".to_string(),
            "user.input",
        ),
        assistant_item(
            "inv-context-lease",
            1,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-context-lease:1:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-context-lease".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-read-lease".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-lease".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-context-lease:1:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-context-lease".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-read-lease".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-lease".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "fn assemble_native_prompt() {}".to_string(),
            },
        },
        assistant_item(
            "inv-context-lease",
            1,
            4,
            &ModelDirective::Act {
                action: "tool:retain_context:{\"path\":\"src/native/prompt_assembly.rs\",\"turns\":4,\"floor\":\"exact_excerpt\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-context-lease:1:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-context-lease".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-retain-context".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-retain-context".to_string(),
                tool_name: "retain_context".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\",\"turns\":4,\"floor\":\"exact_excerpt\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-context-lease:1:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-context-lease".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-retain-context".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-retain-context".to_string(),
                tool_name: "retain_context".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "{\"granted\":true}".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);
    let trace = rendered
        .assembly
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/prompt_assembly.rs")
        .expect("active window trace should exist");

    assert!(rendered.prompt.contains("lease_remaining="));
    assert_eq!(trace.minimum_floor, "exact_excerpt");
    assert!(trace.lease_remaining_turns.unwrap_or_default() >= 1);
    assert!(trace
        .reason_codes
        .iter()
        .any(|reason| reason == "lease_active"));
}

#[test]
fn assemble_native_prompt_uses_graph_session_hints_to_boost_supporting_paths() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("runtime-state.sqlite3");
    let mut runtime_env = HashMap::new();
    runtime_env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        db_path.display().to_string(),
    );
    runtime_env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "proj-graph-boost".to_string(),
    );
    let store = crate::native::runtime_hardening::NativeRuntimeStateStore::open(
        &crate::native::runtime_hardening::RuntimeHardeningConfig::from_env(&runtime_env),
    )
    .expect("store should open");
    store
        .upsert_graphcode_artifact(&crate::native::runtime_hardening::GraphCodeArtifactUpsert {
            registry_key: "proj-graph-boost:codegraph".to_string(),
            project_id: "proj-graph-boost".to_string(),
            substrate_kind: "codegraph".to_string(),
            storage_backend: "sqlite".to_string(),
            storage_reference: "inline".to_string(),
            derivative_snapshot_path: None,
            constitution_path: None,
            canonical_fingerprint: "fingerprint-1".to_string(),
            profile_version: "test".to_string(),
            ucp_engine_version: "test".to_string(),
            extractor_version: "test".to_string(),
            runtime_version: "test".to_string(),
            freshness_state: "fresh".to_string(),
            repo_manifest_json: "{}".to_string(),
            active_session_ref: Some("proj-graph-boost:codegraph:session:1".to_string()),
            snapshot_json: "{}".to_string(),
        })
        .expect("artifact should upsert");
    store
        .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
            session_ref: "proj-graph-boost:codegraph:session:1".to_string(),
            registry_key: "proj-graph-boost:codegraph".to_string(),
            substrate_kind: "codegraph".to_string(),
            current_focus_json: "[\"node-a\"]".to_string(),
            pinned_nodes_json: "[]".to_string(),
            recent_traversals_json: "[{\"paths\":[\"src/native/prompt_assembly.rs\",\"src/native/turn_items.rs\"]}]".to_string(),
            working_set_refs_json: "[{\"node_id\":\"node-a\",\"path\":\"src/native/prompt_assembly.rs\"},{\"node_id\":\"node-b\",\"path\":\"src/native/turn_items.rs\"},{\"node_id\":\"node-c\",\"path\":\"src/native/tests.rs\"}]".to_string(),
            hydrated_excerpts_json: "[]".to_string(),
            path_artifacts_json: serde_json::to_string(&vec![crate::core::graph_query::GraphQueryEdge {
                source: "node-a".to_string(),
                target: "node-b".to_string(),
                edge_type: "calls".to_string(),
            }])
            .expect("serialize graph edge"),
            snapshot_fingerprint: "snapshot-1".to_string(),
            freshness_state: "fresh".to_string(),
        })
        .expect("session should upsert");
    let history = vec![
        user_input_item(
            "inv-graph-boost",
            0,
            "objective",
            "Finish prompt assembly changes and verify supporting code".to_string(),
            "user.input",
        ),
        assistant_item(
            "inv-graph-boost",
            1,
            1,
            &ModelDirective::Act {
                action: "tool:write_file:{\"path\":\"src/native/prompt_assembly.rs\",\"content\":\"updated\",\"append\":false}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-graph-boost:1:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-graph-boost".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-write-a".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-a".to_string(),
                tool_name: "write_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\",\"content\":\"updated\",\"append\":false}".to_string(),
            },
        },
        TurnItem {
            id: "inv-graph-boost:1:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-graph-boost".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-write-a".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-a".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "updated".to_string(),
            },
        },
        assistant_item(
            "inv-graph-boost",
            1,
            4,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-graph-boost:1:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-graph-boost".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-read-b".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-b".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/turn_items.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-graph-boost:1:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-graph-boost".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-read-b".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-b".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "fn render_successful_tool_result_content() {}".to_string(),
            },
        },
        assistant_item(
            "inv-graph-boost",
            1,
            7,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/tests.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-graph-boost:1:8".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 8,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-graph-boost".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-read-c".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-c".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/tests.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-graph-boost:1:9".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 9,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-graph-boost".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-read-c".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-c".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "fn helper() {}".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt_with_runtime_env(
        &config,
        &input,
        &history,
        &contracts,
        &runtime_env,
    );
    let supporting = rendered
        .assembly
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/turn_items.rs")
        .expect("supporting window should exist");
    let unrelated = rendered
        .assembly
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/tests.rs")
        .expect("unrelated window should exist");

    assert!(supporting.relevance_score > unrelated.relevance_score);
    assert!(supporting
        .reason_codes
        .iter()
        .any(|reason| reason == "graph_supports_dirty"));
    assert!(!unrelated
        .reason_codes
        .iter()
        .any(|reason| reason == "graph_supports_dirty"));
}

#[test]
fn agent_loop_smoke_simulates_navigation_editing_and_context_visibility() {
    let tmp = tempdir().expect("tempdir");
    let src_native = tmp.path().join("src/native");
    fs::create_dir_all(&src_native).expect("create src/native");
    fs::write(
        src_native.join("prompt_assembly.rs"),
        "fn assemble_native_prompt() {\n    \"before edit\"\n}\n",
    )
    .expect("seed prompt assembly file");
    fs::write(
        src_native.join("turn_items.rs"),
        "fn render_successful_tool_result_content() {\n    \"turn items support\"\n}\n",
    )
    .expect("seed turn items file");
    fs::write(
        src_native.join("tests.rs"),
        "fn unrelated_helper() {\n    \"tests file\"\n}\n",
    )
    .expect("seed unrelated file");

    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "ACT:tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            "ACT:tool:retain_context:{\"path\":\"src/native/prompt_assembly.rs\",\"turns\":4,\"floor\":\"exact_excerpt\"}".to_string(),
            "ACT:tool:write_file:{\"path\":\"src/native/prompt_assembly.rs\",\"content\":\"fn assemble_native_prompt() {\\n    \\\"after edit\\\"\\n}\\n\",\"append\":false}".to_string(),
            "ACT:tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
            "ACT:tool:read_file:{\"path\":\"src/native/tests.rs\"}".to_string(),
            "DONE:simulated navigation and edits complete".to_string(),
        ],
        prompts.clone(),
    );

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let input = ExecutionInput {
        task_description:
            "Update src/native/prompt_assembly.rs and consult supporting code nearby"
                .to_string(),
        success_criteria:
            "Keep the target file in context while checking src/native/turn_items.rs and ignoring unrelated files"
                .to_string(),
        context: Some(
            "When multiple files are visible, the graph-adjacent support file should outrank unrelated files."
                .to_string(),
        ),
        prior_attempts: Vec::new(),
        verifier_feedback: None,
        native_prompt_metadata: None,
    };
    let engine = NativeToolEngine::default();
    let contracts = engine.contracts_for_mode(config.agent_mode);
    let db_path = tmp.path().join("runtime-state.sqlite3");
    let mut runtime_env = HashMap::new();
    runtime_env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        db_path.display().to_string(),
    );
    runtime_env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "proj-smoke-context".to_string(),
    );
    let store = crate::native::runtime_hardening::NativeRuntimeStateStore::open(
        &crate::native::runtime_hardening::RuntimeHardeningConfig::from_env(&runtime_env),
    )
    .expect("store should open");
    store
        .upsert_graphcode_artifact(&crate::native::runtime_hardening::GraphCodeArtifactUpsert {
            registry_key: "proj-smoke-context:codegraph".to_string(),
            project_id: "proj-smoke-context".to_string(),
            substrate_kind: "codegraph".to_string(),
            storage_backend: "sqlite".to_string(),
            storage_reference: "inline".to_string(),
            derivative_snapshot_path: None,
            constitution_path: None,
            canonical_fingerprint: "fingerprint-smoke".to_string(),
            profile_version: "test".to_string(),
            ucp_engine_version: "test".to_string(),
            extractor_version: "test".to_string(),
            runtime_version: "test".to_string(),
            freshness_state: "fresh".to_string(),
            repo_manifest_json: "{}".to_string(),
            active_session_ref: Some("proj-smoke-context:codegraph:session:1".to_string()),
            snapshot_json: "{}".to_string(),
        })
        .expect("artifact should upsert");
    store
        .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
            session_ref: "proj-smoke-context:codegraph:session:1".to_string(),
            registry_key: "proj-smoke-context:codegraph".to_string(),
            substrate_kind: "codegraph".to_string(),
            current_focus_json: "[\"node-a\"]".to_string(),
            pinned_nodes_json: "[]".to_string(),
            recent_traversals_json:
                "[{\"paths\":[\"src/native/prompt_assembly.rs\",\"src/native/turn_items.rs\"]}]"
                    .to_string(),
            working_set_refs_json: "[{\"node_id\":\"node-a\",\"path\":\"src/native/prompt_assembly.rs\"},{\"node_id\":\"node-b\",\"path\":\"src/native/turn_items.rs\"},{\"node_id\":\"node-c\",\"path\":\"src/native/tests.rs\"}]".to_string(),
            hydrated_excerpts_json: "[]".to_string(),
            path_artifacts_json: serde_json::to_string(&vec![crate::core::graph_query::GraphQueryEdge {
                source: "node-a".to_string(),
                target: "node-b".to_string(),
                edge_type: "calls".to_string(),
            }])
            .expect("serialize graph edge"),
            snapshot_fingerprint: "snapshot-smoke".to_string(),
            freshness_state: "fresh".to_string(),
        })
        .expect("session should upsert");
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &runtime_env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-smoke-context",
            vec![user_input_item(
                "inv-smoke-context",
                0,
                "objective",
                "Simulate navigation, edit the target file, and inspect supporting files"
                    .to_string(),
                "test",
            )],
            |turn_index, state, history| {
                let rendered = assemble_native_prompt_with_runtime_env(
                    &config,
                    &input,
                    history,
                    &contracts,
                    &runtime_env,
                );
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
                    format!("inv-smoke-context:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should complete");

    for turn in &result.turns {
        if let Some(assembly) = turn.request.prompt_assembly.as_ref() {
            let windows = assembly
                .active_code_window_trace
                .iter()
                .map(|window| {
                    format!(
                        "{}:{}:{}:{}:lease={:?}:reasons={}",
                        window.path,
                        window.relevance_score,
                        window.desired_representation,
                        window.minimum_floor,
                        window.lease_remaining_turns,
                        window.reason_codes.join("|")
                    )
                })
                .collect::<Vec<_>>();
            eprintln!(
                "turn {} active windows => {}",
                turn.turn_index,
                windows.join(" ; ")
            );
        }
    }

    assert_eq!(result.turns.len(), 6);
    assert!(fs::read_to_string(src_native.join("prompt_assembly.rs"))
        .expect("read edited file")
        .contains("after edit"));

    let lease_assembly = result.turns[2]
        .request
        .prompt_assembly
        .as_ref()
        .expect("lease turn assembly");
    let lease_trace = lease_assembly
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/prompt_assembly.rs")
        .expect("leased target trace should exist");
    assert_eq!(lease_trace.minimum_floor, "exact_excerpt");
    assert!(lease_trace.lease_remaining_turns.unwrap_or_default() >= 1);
    assert!(lease_trace
        .reason_codes
        .iter()
        .any(|reason| reason == "lease_active"));

    let final_assembly = result.turns[5]
        .request
        .prompt_assembly
        .as_ref()
        .expect("final prompt assembly");
    let target = final_assembly
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/prompt_assembly.rs")
        .expect("dirty target trace should exist");
    let supporting = final_assembly
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/turn_items.rs")
        .expect("supporting trace should exist");
    let unrelated = final_assembly
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/tests.rs")
        .expect("unrelated trace should exist");

    assert_eq!(target.status, "dirty");
    assert_eq!(target.minimum_floor, "exact_excerpt");
    assert!(
        target.desired_representation == "exact_excerpt"
            || target.desired_representation == "expanded_excerpt"
    );
    assert!(supporting.relevance_score > unrelated.relevance_score);
    assert!(supporting
        .reason_codes
        .iter()
        .any(|reason| reason == "graph_supports_dirty"));
    assert!(!unrelated
        .reason_codes
        .iter()
        .any(|reason| reason == "graph_supports_dirty"));

    let captured = prompts.lock().expect("prompts lock");
    assert_eq!(captured.len(), 6);
    assert!(captured[2].contains("lease_remaining="));
    assert!(captured[5].contains("target_level="));
    assert!(captured[5].contains("next_downgrade="));
}

#[test]
fn agent_loop_smoke_observes_context_lease_expiry_across_turns() {
    let tmp = tempdir().expect("tempdir");
    let src_native = tmp.path().join("src/native");
    fs::create_dir_all(&src_native).expect("create src/native");
    fs::write(
        src_native.join("prompt_assembly.rs"),
        "fn assemble_native_prompt() {\n    \"lease target\"\n}\n",
    )
    .expect("seed target file");
    for (name, label) in [
        ("turn_items.rs", "support a"),
        ("tests.rs", "support b"),
        ("adapter.rs", "support c"),
        ("tool_engine.rs", "support d"),
    ] {
        fs::write(
            src_native.join(name),
            format!("fn helper() {{\n    \"{label}\"\n}}\n"),
        )
        .expect("seed support file");
    }

    let prompts = Arc::new(Mutex::new(Vec::new()));
    let model = RecordingModelClient::new(
        vec![
            "ACT:tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            "ACT:tool:retain_context:{\"path\":\"src/native/prompt_assembly.rs\",\"turns\":4,\"floor\":\"exact_excerpt\"}".to_string(),
            "ACT:tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
            "ACT:tool:read_file:{\"path\":\"src/native/tests.rs\"}".to_string(),
            "ACT:tool:read_file:{\"path\":\"src/native/adapter.rs\"}".to_string(),
            "ACT:tool:read_file:{\"path\":\"src/native/tool_engine.rs\"}".to_string(),
            "DONE:lease window naturally expired".to_string(),
        ],
        prompts.clone(),
    );

    let mut config = NativeRuntimeConfig::default();
    config.prompt_headroom = 256;
    let input = ExecutionInput {
        task_description:
            "Keep src/native/prompt_assembly.rs available while navigating nearby files".to_string(),
        success_criteria:
            "Observe lease countdown and eventual expiry without re-reading the target".to_string(),
        context: Some(
            "Continue navigating related files for several turns after requesting a lease."
                .to_string(),
        ),
        prior_attempts: Vec::new(),
        verifier_feedback: None,
        native_prompt_metadata: None,
    };
    let engine = NativeToolEngine::default();
    let contracts = engine.contracts_for_mode(config.agent_mode);
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let mut loop_harness = AgentLoop::new(config.clone(), model);

    let result = loop_harness
        .run_with_history(
            "inv-lease-expiry",
            vec![user_input_item(
                "inv-lease-expiry",
                0,
                "objective",
                "Lease the main target and keep working elsewhere until it expires".to_string(),
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
                    .expect("parse tool action")
                    .expect("tool action present");
                vec![engine.execute_action_trace_for_mode(
                    config.agent_mode,
                    format!("inv-lease-expiry:turn:{turn_index}:tool:0"),
                    &tool_action,
                    &ctx,
                )]
            },
        )
        .expect("loop should complete");

    let lease_remaining = [2usize, 3, 4, 5]
        .into_iter()
        .map(|turn_index| {
            result.turns[turn_index]
                .request
                .prompt_assembly
                .as_ref()
                .and_then(|assembly| {
                    assembly
                        .active_code_window_trace
                        .iter()
                        .find(|window| window.path == "src/native/prompt_assembly.rs")
                })
                .and_then(|window| window.lease_remaining_turns)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let expired_trace = result.turns[6]
        .request
        .prompt_assembly
        .as_ref()
        .expect("expiry turn assembly")
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/prompt_assembly.rs")
        .expect("target trace on expiry turn");

    eprintln!("lease countdown => {:?}", lease_remaining);
    eprintln!(
        "lease expiry turn => floor={} lease={:?} reasons={}",
        expired_trace.minimum_floor,
        expired_trace.lease_remaining_turns,
        expired_trace.reason_codes.join("|")
    );

    assert_eq!(result.turns.len(), 7);
    assert_eq!(lease_remaining, vec![3, 2, 1, 1]);
    assert_eq!(expired_trace.lease_remaining_turns, None);
    assert_eq!(expired_trace.minimum_floor, "handle_only");
    assert!(!expired_trace
        .reason_codes
        .iter()
        .any(|reason| reason == "lease_active"));

    let captured = prompts.lock().expect("prompts lock");
    assert_eq!(captured.len(), 7);
    assert!(captured[2].contains("lease_remaining=3"));
    assert!(captured[6].contains("lease_remaining=none"));
}

#[test]
fn agent_loop_smoke_observes_budget_pressure_with_many_active_windows() {
    let tmp = tempdir().expect("tempdir");
    let src_native = tmp.path().join("src/native");
    fs::create_dir_all(&src_native).expect("create src/native");
    let target_path = src_native.join("prompt_assembly.rs");
    fs::write(
        &target_path,
        format!(
            "fn assemble_native_prompt() {{\n    \"{}\"\n}}\n",
            "target before ".repeat(120)
        ),
    )
    .expect("seed target file");
    for index in 0..4 {
        fs::write(
            src_native.join(format!("support_{index}.rs")),
            format!(
                "fn support_{index}() {{\n    \"{}\"\n}}\n",
                format!("support-{index} ").repeat(120)
            ),
        )
        .expect("seed support file");
    }

    let mut config = NativeRuntimeConfig::default();
    config.token_budget = 2_200;
    config.prompt_headroom = 64;
    let input = ExecutionInput {
        task_description:
            "Edit src/native/prompt_assembly.rs, then navigate many nearby support files"
                .to_string(),
        success_criteria:
            "Under pressure, keep the dirty target visible while summarizing or omitting lower-priority windows"
                .to_string(),
        context: Some(
            "This smoke intentionally creates many active windows so prompt assembly must compact them."
                .to_string(),
        ),
        prior_attempts: Vec::new(),
        verifier_feedback: None,
        native_prompt_metadata: None,
    };
    let engine = NativeToolEngine::default();
    let contracts = engine.contracts_for_mode(config.agent_mode);
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), &scope, &env);
    let invocation_id = "inv-budget-windows";
    let mut history = vec![user_input_item(
        invocation_id,
        0,
        "objective",
        "Force many active windows while keeping the edited target visible".to_string(),
        "test",
    )];
    let actions = vec![
        "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
        "tool:write_file:{\"path\":\"src/native/prompt_assembly.rs\",\"content\":\"fn assemble_native_prompt() {\\n    \\\"target after {}\\\"\\n}\\n\",\"append\":false}".replace("{}", &"edited ".repeat(40)),
        "tool:read_file:{\"path\":\"src/native/support_0.rs\"}".to_string(),
        "tool:read_file:{\"path\":\"src/native/support_1.rs\"}".to_string(),
        "tool:read_file:{\"path\":\"src/native/support_2.rs\"}".to_string(),
        "tool:read_file:{\"path\":\"src/native/support_3.rs\"}".to_string(),
    ];
    for (turn_index, action) in actions.iter().enumerate() {
        let turn_index = u32::try_from(turn_index).expect("turn index");
        history.push(assistant_item(
            invocation_id,
            turn_index,
            1,
            &ModelDirective::Act {
                action: action.clone(),
            },
        ));
        let tool_action = NativeToolAction::parse(action)
            .expect("parse tool action")
            .expect("tool action present");
        let trace = engine.execute_action_trace_for_mode(
            config.agent_mode,
            format!("{invocation_id}:turn:{turn_index}:tool:0"),
            &tool_action,
            &ctx,
        );
        let turn = AgentLoopTurn {
            turn_index,
            from_state: AgentLoopState::Act,
            to_state: AgentLoopState::Act,
            request: ModelTurnRequest {
                turn_index,
                state: AgentLoopState::Act,
                agent_mode: config.agent_mode,
                prompt: String::new(),
                context: None,
                prompt_assembly: None,
            },
            directive: ModelDirective::Act {
                action: action.clone(),
            },
            raw_output: format!("ACT:{action}"),
            budget_used_before: 0,
            budget_used_after: 0,
            budget_remaining: config.token_budget,
            budget_thresholds_crossed: Vec::new(),
            model_latency_ms: 0,
            tool_latency_ms: 0,
            turn_duration_ms: 0,
            elapsed_since_invocation_ms: 0,
            request_tokens: 0,
            response_tokens: 0,
            tool_calls: vec![trace.clone()],
        };
        history.extend(crate::native::turn_items::items_from_tool_trace(
            invocation_id,
            &turn,
            &trace,
        ));
    }

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);
    let final_assembly = &rendered.assembly;
    let target = final_assembly
        .active_code_window_trace
        .iter()
        .find(|window| window.path == "src/native/prompt_assembly.rs")
        .expect("dirty target trace should exist");

    eprintln!(
        "budget pressure => active={} trace={} compacted={} omitted={} overflow={} skipped={} truncated={}",
        final_assembly.active_code_window_count,
        final_assembly.active_code_window_trace.len(),
        final_assembly.compacted_summary_count,
        final_assembly.omitted_active_code_window_count,
        final_assembly.overflow_classification,
        final_assembly.skipped_item_count,
        final_assembly.truncated_item_count
    );

    assert_eq!(target.status, "dirty");
    assert_eq!(target.minimum_floor, "exact_excerpt");
    assert!(final_assembly.active_code_window_trace.len() >= 5);
    assert!(
        final_assembly.active_code_window_count < final_assembly.active_code_window_trace.len()
    );
    assert!(final_assembly.omitted_active_code_window_count > 0);
    assert!(final_assembly.compacted_summary_count > 0);
    assert!(final_assembly
        .active_code_windows
        .iter()
        .any(|item| item.text.contains("path=src/native/prompt_assembly.rs")));
    assert!(final_assembly
        .compacted_summaries
        .iter()
        .any(|item| item.text.contains("[code_window_summary:")));
    assert!(rendered.prompt.contains("Compacted Summaries"));
    assert!(rendered.prompt.contains("[code_window_summary:"));
}

#[test]
fn retain_context_tool_grants_bounded_relative_path_lease() {
    let engine = NativeToolEngine::default();
    let version = engine
        .contracts_for_mode(NativeRuntimeConfig::default().agent_mode)
        .into_iter()
        .find(|contract| contract.name == "retain_context")
        .expect("retain_context contract should exist")
        .version;
    let temp = tempdir().expect("tempdir");
    let scope = allow_all_scope();
    let env = HashMap::new();
    let ctx = test_tool_context(temp.path(), &scope, &env);
    let output = engine
        .execute(
            &NativeToolAction {
                name: "retain_context".to_string(),
                version,
                input: json!({
                    "path": "src/native/prompt_assembly.rs",
                    "turns": 99,
                    "floor": "exact_excerpt"
                }),
            },
            &ctx,
        )
        .expect("retain_context should succeed");

    assert_eq!(
        output.get("granted").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        output.get("turns").and_then(|value| value.as_u64()),
        Some(6)
    );
    assert_eq!(
        output.get("floor").and_then(|value| value.as_str()),
        Some("exact_excerpt")
    );
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
fn assemble_native_prompt_includes_changed_excerpt_for_partial_dirty_windows() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let initial_content = [
        "fn alpha() {}",
        "let beta = 1;",
        "let gamma = 2;",
        "let delta = 3;",
        "let epsilon = 4;",
        "let zeta = 5;",
    ]
    .join("\n");
    let rewritten_content = [
        "fn alpha() {}",
        "let beta = 10;",
        "let gamma = 20;",
        "let delta = 3;",
        "let epsilon = 4;",
        "let zeta = 5;",
    ]
    .join("\n");
    let history = vec![
        user_input_item(
            "inv-partial-diff",
            0,
            "objective",
            "Capture a partial dirty edit".to_string(),
            "test",
        ),
        assistant_item(
            "inv-partial-diff",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-partial-diff:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-partial-diff".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-partial-diff".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-partial-diff".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-partial-diff:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-partial-diff".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-partial-diff".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-partial-diff".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: initial_content,
            },
        },
        assistant_item(
            "inv-partial-diff",
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
            id: "inv-partial-diff:1:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-partial-diff".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-write-partial-diff".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-partial-diff".to_string(),
                tool_name: "write_file".to_string(),
                request: format!(
                    "{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = rewritten_content,
                ),
            },
        },
        TurnItem {
            id: "inv-partial-diff:1:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-partial-diff".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-write-partial-diff".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-partial-diff".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "wrote file".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert!(rendered.prompt.contains("changed_lines=2-3"));
    assert!(rendered.prompt.contains("[changed_excerpt lines=2]"));
    assert!(rendered.prompt.contains("   2: let beta = 10;"));
    assert!(rendered.prompt.contains("   3: let gamma = 20;"));
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
    assert_eq!(rendered.assembly.omitted_active_code_window_count, 1);
    assert!(rendered
        .prompt
        .contains("path=src/native/prompt_assembly.rs"));
    assert!(rendered
        .prompt
        .contains("[code_window_summary:clean:read_file] path=src/native/tests.rs"));
    assert!(!rendered
        .prompt
        .contains("[tool_call:read_file] {\"path\":\"src/native/tests.rs\"}"));
}

#[test]
fn active_code_windows_truncate_large_clean_content_for_prompt() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let large_clean_content = format!(
        "{}needle-middle{}needle-tail",
        "A".repeat(4_500),
        "B".repeat(4_500)
    );
    let history = vec![
        user_input_item(
            "inv-window-truncate",
            0,
            "objective",
            "Keep a large clean window visible without replaying the whole file".to_string(),
            "test",
        ),
        assistant_item(
            "inv-window-truncate",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/turn_items.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-window-truncate:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-window-truncate".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-large".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-large".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/turn_items.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-window-truncate:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-window-truncate".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-large".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-large".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: large_clean_content.clone(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert!(rendered.prompt.contains("content_truncated=true"));
    assert!(rendered.prompt.contains("needle-tail"));
    assert!(!rendered.prompt.contains("needle-middle"));
    assert!(rendered.assembly.active_code_window_chars < large_clean_content.chars().count());
}

#[test]
fn prompt_assembly_reports_stale_tool_call_suppression() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let history = vec![
        user_input_item(
            "inv-stale-tool-call",
            0,
            "objective",
            "Keep the latest assistant context only".to_string(),
            "test",
        ),
        assistant_item(
            "inv-stale-tool-call",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:list_files:{\"path\":\".\",\"recursive\":false}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-stale-tool-call:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-stale-tool-call".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-stale-list-files".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-stale-list-files".to_string(),
                tool_name: "list_files".to_string(),
                request: "{\"path\":\".\",\"recursive\":false}".to_string(),
            },
        },
        assistant_item(
            "inv-stale-tool-call",
            1,
            3,
            &ModelDirective::Think {
                message: "Continue with the latest state".to_string(),
            },
        ),
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.assembly.suppressed_stale_tool_call_count, 1);
    assert!(!rendered.prompt.contains("[tool_call:list_files]"));
}

#[test]
fn prompt_assembly_suppresses_redundant_same_turn_tool_calls_and_stale_thinks() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let history = vec![
        user_input_item(
            "inv-redundant-tool-call",
            0,
            "objective",
            "Keep only the latest useful execution trace".to_string(),
            "test",
        ),
        assistant_item(
            "inv-redundant-tool-call",
            0,
            1,
            &ModelDirective::Think {
                message: "First thought".to_string(),
            },
        ),
        assistant_item(
            "inv-redundant-tool-call",
            1,
            2,
            &ModelDirective::Think {
                message: "Second thought".to_string(),
            },
        ),
        assistant_item(
            "inv-redundant-tool-call",
            2,
            3,
            &ModelDirective::Act {
                action: "tool:list_files:{\"path\":\"src/native\",\"recursive\":false}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-redundant-tool-call:2:4".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 4,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-redundant-tool-call".to_string(),
                turn_index: Some(2),
                source: "tool.call".to_string(),
                reference: Some("call-list-files-latest".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-list-files-latest".to_string(),
                tool_name: "list_files".to_string(),
                request: "{\"path\":\"src/native\",\"recursive\":false}".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert!(rendered.assembly.suppressed_stale_tool_call_count >= 1);
    assert!(!rendered.prompt.contains("[tool_call:list_files]"));
    assert!(rendered.prompt.contains("Second thought"));
    assert!(!rendered.prompt.contains("First thought"));
}

#[test]
fn prompt_assembly_suppresses_stale_graph_query_results() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let history = vec![
        user_input_item(
            "inv-stale-graph-query",
            0,
            "objective",
            "Keep only the latest successful graph query result visible".to_string(),
            "test",
        ),
        assistant_item(
            "inv-stale-graph-query",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:graph_query:{\"kind\":\"filter\",\"path_prefix\":\"src/native\",\"max_results\":4}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-stale-graph-query:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-stale-graph-query".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-graph-query-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-graph-query-1".to_string(),
                tool_name: "graph_query".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "kind=filter nodes=2 edges=0 top=[file:src/native/old.rs]".to_string(),
            },
        },
        assistant_item(
            "inv-stale-graph-query",
            1,
            3,
            &ModelDirective::Think {
                message: "Need a more precise ranked query".to_string(),
            },
        ),
        assistant_item(
            "inv-stale-graph-query",
            2,
            4,
            &ModelDirective::Act {
                action: "tool:graph_query:{\"kind\":\"python_query\",\"repo_name\":\"repo\",\"max_results\":4}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-stale-graph-query:2:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-stale-graph-query".to_string(),
                turn_index: Some(2),
                source: "tool.result".to_string(),
                reference: Some("call-graph-query-2".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-graph-query-2".to_string(),
                tool_name: "graph_query".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "kind=python_query nodes=1 edges=0 ranked=[symbol:src/native/tests.rs::latest_graph_query(2)]".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(
        rendered.assembly.suppressed_stale_graph_query_result_count,
        1
    );
    assert_eq!(rendered.assembly.tool_result_items_visible, 1);
    assert!(rendered
        .prompt
        .contains("kind=python_query nodes=1 edges=0"));
    assert!(!rendered.prompt.contains("kind=filter nodes=2 edges=0"));
}

#[test]
fn prompt_assembly_keeps_long_graph_query_history_within_budget() {
    let mut config = NativeRuntimeConfig::default();
    config.token_budget = 15_000;
    config.prompt_headroom = 128;
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let mut history = vec![user_input_item(
        "inv-long-graph-query-history",
        0,
        "objective",
        "Carry a long graph-query-heavy runtime history without blowing the prompt budget"
            .to_string(),
        "test",
    )];

    for turn_index in 0..6 {
        history.push(assistant_item(
            "inv-long-graph-query-history",
            turn_index,
            turn_index.saturating_mul(2).saturating_add(1),
            &ModelDirective::Act {
                action: format!(
                    "tool:graph_query:{{\"kind\":\"python_query\",\"repo_name\":\"repo\",\"max_results\":4,\"turn\":{turn_index}}}"
                ),
            },
        ));
        history.push(TurnItem {
            id: format!("inv-long-graph-query-history:{turn_index}:result"),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(turn_index),
                item_index: turn_index.saturating_mul(2).saturating_add(2),
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-long-graph-query-history".to_string(),
                turn_index: Some(turn_index),
                source: "tool.result".to_string(),
                reference: Some(format!("call-graph-query-{turn_index}")),
            },
            kind: TurnItemKind::ToolResult {
                call_id: format!("call-graph-query-{turn_index}"),
                tool_name: "graph_query".to_string(),
                outcome: TurnItemOutcome::Success,
                content: format!(
                    "kind=python_query nodes=1 edges=0 ranked=[symbol:src/native/tests.rs::graph_query_turn_{turn_index}(2)] notes={} ",
                    "X".repeat(1_400)
                ),
            },
        });
    }

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(
        rendered.assembly.suppressed_stale_graph_query_result_count,
        5
    );
    assert_eq!(rendered.assembly.tool_result_items_visible, 1);
    assert!(rendered.prompt.contains("graph_query_turn_5(2)"));
    assert!(!rendered.prompt.contains("graph_query_turn_0(2)"));
    assert!(rendered.prompt.chars().count() <= rendered.assembly.available_budget);
}

#[test]
fn prompt_assembly_reports_duplicate_and_window_suppression_counters() {
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
            "inv-suppression-counters",
            0,
            "objective",
            "Track prompt suppression counters".to_string(),
            "test",
        ),
        assistant_item(
            "inv-suppression-counters",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/tests.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-suppression-counters:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-suppression-counters".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-counter-1".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-counter-1".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/tests.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-suppression-counters:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-suppression-counters".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-counter-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-counter-1".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: clean_content.clone(),
            },
        },
        assistant_item(
            "inv-suppression-counters",
            1,
            4,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/tests.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-suppression-counters:1:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-suppression-counters".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-read-counter-2".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-counter-2".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/tests.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-suppression-counters:1:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-suppression-counters".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-read-counter-2".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-counter-2".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: clean_content,
            },
        },
        assistant_item(
            "inv-suppression-counters",
            2,
            7,
            &ModelDirective::Act {
                action: format!(
                    "tool:write_file:{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = dirty_content,
                ),
            },
        ),
        TurnItem {
            id: "inv-suppression-counters:2:8".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 8,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-suppression-counters".to_string(),
                turn_index: Some(2),
                source: "tool.call".to_string(),
                reference: Some("call-write-counter".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-counter".to_string(),
                tool_name: "write_file".to_string(),
                request: format!(
                    "{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = dirty_content,
                ),
            },
        },
        TurnItem {
            id: "inv-suppression-counters:2:9".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 9,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-suppression-counters".to_string(),
                turn_index: Some(2),
                source: "tool.result".to_string(),
                reference: Some("call-write-counter".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-counter".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "wrote file".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.assembly.active_code_window_count, 1);
    assert_eq!(rendered.assembly.omitted_active_code_window_count, 1);
    assert_eq!(rendered.assembly.suppressed_by_active_code_window_count, 9);
    assert_eq!(rendered.assembly.suppressed_duplicate_read_count, 0);
    assert_eq!(rendered.assembly.suppressed_stale_tool_call_count, 0);
    assert!(rendered
        .prompt
        .contains("[code_window_summary:clean:read_file] path=src/native/tests.rs"));
}

#[test]
fn assemble_native_prompt_keeps_failed_read_visible_when_no_window_exists() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let history = vec![
        user_input_item(
            "inv-read-failure",
            0,
            "objective",
            "Attempt a read that fails".to_string(),
            "test",
        ),
        assistant_item(
            "inv-read-failure",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-read-failure:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-read-failure".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-failure".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-failure".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-read-failure:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-read-failure".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-failure".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-failure".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Failure,
                content: "permission denied".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.assembly.active_code_window_count, 0);
    assert_eq!(rendered.assembly.tool_result_items_visible, 1);
    assert!(rendered.prompt.contains("permission denied"));
    assert!(rendered.prompt.contains("[tool_result:read_file:Failure]"));
}

#[test]
fn assemble_native_prompt_keeps_failed_write_visible_without_dirty_window() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let attempted_content = "fn not_written() {}\n".repeat(32);
    let tool_request = format!(
        "{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
        content = attempted_content,
    );
    let history = vec![
        user_input_item(
            "inv-write-failure",
            0,
            "objective",
            "Attempt a write that fails".to_string(),
            "test",
        ),
        assistant_item(
            "inv-write-failure",
            0,
            1,
            &ModelDirective::Act {
                action: format!(
                    "tool:write_file:{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = attempted_content,
                ),
            },
        ),
        TurnItem {
            id: "inv-write-failure:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-write-failure".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-write-failure".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-failure".to_string(),
                tool_name: "write_file".to_string(),
                request: tool_request,
            },
        },
        TurnItem {
            id: "inv-write-failure:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-write-failure".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-write-failure".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-failure".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Failure,
                content: "disk full".to_string(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.assembly.active_code_window_count, 0);
    assert!(rendered
        .prompt
        .contains("[tool_result:write_file:Failure] disk full"));
    assert!(!rendered.prompt.contains("Active Code Windows"));
    assert!(!rendered.prompt.contains(&attempted_content));
}

#[test]
fn assemble_native_prompt_reread_confirms_dirty_window() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let initial_content = "fn before() {}\n".repeat(32);
    let rewritten_content = "fn after() {}\n".repeat(32);
    let history = vec![
        user_input_item(
            "inv-confirmed",
            0,
            "objective",
            "Write then confirm the file contents".to_string(),
            "test",
        ),
        assistant_item(
            "inv-confirmed",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-confirmed:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-confirmed".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-confirmed-1".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-confirmed-1".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-confirmed:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-confirmed".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-confirmed-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-confirmed-1".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: initial_content,
            },
        },
        assistant_item(
            "inv-confirmed",
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
            id: "inv-confirmed:1:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-confirmed".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-write-confirmed".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-confirmed".to_string(),
                tool_name: "write_file".to_string(),
                request: format!(
                    "{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = rewritten_content,
                ),
            },
        },
        TurnItem {
            id: "inv-confirmed:1:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-confirmed".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-write-confirmed".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-confirmed".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "wrote file".to_string(),
            },
        },
        assistant_item(
            "inv-confirmed",
            2,
            7,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-confirmed:2:8".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 8,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-confirmed".to_string(),
                turn_index: Some(2),
                source: "tool.call".to_string(),
                reference: Some("call-read-confirmed-2".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-confirmed-2".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-confirmed:2:9".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 9,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-confirmed".to_string(),
                turn_index: Some(2),
                source: "tool.result".to_string(),
                reference: Some("call-read-confirmed-2".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-confirmed-2".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: rewritten_content.clone(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.assembly.active_code_window_count, 1);
    assert!(rendered.prompt.contains("[code_window:clean:read_file]"));
    assert!(rendered.prompt.contains("call_id=call-read-confirmed-2"));
    assert!(rendered.prompt.contains("freshness=confirmed_write"));
    assert_eq!(rendered.prompt.matches(&rewritten_content).count(), 1);
    assert!(!rendered.prompt.contains("[tool_call:write_file]"));
    assert!(!rendered.prompt.contains("[tool_result:write_file:Success]"));
}

#[test]
fn assemble_native_prompt_reread_changed_supersedes_prior_dirty_window() {
    let config = NativeRuntimeConfig::default();
    let input = native_input(None);
    let contracts = NativeToolEngine::default().contracts_for_mode(config.agent_mode);
    let initial_content = "fn before() {}\n".repeat(24);
    let rewritten_content = "fn interim() {}\n".repeat(24);
    let final_content = "fn external() {}\n".repeat(24);
    let history = vec![
        user_input_item(
            "inv-reread-changed",
            0,
            "objective",
            "Observe a reread that supersedes a dirty window".to_string(),
            "test",
        ),
        assistant_item(
            "inv-reread-changed",
            0,
            1,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-reread-changed:0:2".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 2,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-reread-changed".to_string(),
                turn_index: Some(0),
                source: "tool.call".to_string(),
                reference: Some("call-read-reread-changed-1".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-reread-changed-1".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-reread-changed:0:3".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(0),
                item_index: 3,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-reread-changed".to_string(),
                turn_index: Some(0),
                source: "tool.result".to_string(),
                reference: Some("call-read-reread-changed-1".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-reread-changed-1".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: initial_content,
            },
        },
        assistant_item(
            "inv-reread-changed",
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
            id: "inv-reread-changed:1:5".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 5,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-reread-changed".to_string(),
                turn_index: Some(1),
                source: "tool.call".to_string(),
                reference: Some("call-write-reread-changed".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-write-reread-changed".to_string(),
                tool_name: "write_file".to_string(),
                request: format!(
                    "{{\"path\":\"src/native/prompt_assembly.rs\",\"content\":{content:?},\"append\":false}}",
                    content = rewritten_content,
                ),
            },
        },
        TurnItem {
            id: "inv-reread-changed:1:6".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(1),
                item_index: 6,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-reread-changed".to_string(),
                turn_index: Some(1),
                source: "tool.result".to_string(),
                reference: Some("call-write-reread-changed".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-write-reread-changed".to_string(),
                tool_name: "write_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: "wrote file".to_string(),
            },
        },
        assistant_item(
            "inv-reread-changed",
            2,
            7,
            &ModelDirective::Act {
                action: "tool:read_file:{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        ),
        TurnItem {
            id: "inv-reread-changed:2:8".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 8,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-reread-changed".to_string(),
                turn_index: Some(2),
                source: "tool.call".to_string(),
                reference: Some("call-read-reread-changed-2".to_string()),
            },
            kind: TurnItemKind::ToolCall {
                call_id: "call-read-reread-changed-2".to_string(),
                tool_name: "read_file".to_string(),
                request: "{\"path\":\"src/native/prompt_assembly.rs\"}".to_string(),
            },
        },
        TurnItem {
            id: "inv-reread-changed:2:9".to_string(),
            model_visible: true,
            correlation: TurnItemCorrelation {
                turn_index: Some(2),
                item_index: 9,
            },
            provenance: TurnItemProvenance {
                invocation_id: "inv-reread-changed".to_string(),
                turn_index: Some(2),
                source: "tool.result".to_string(),
                reference: Some("call-read-reread-changed-2".to_string()),
            },
            kind: TurnItemKind::ToolResult {
                call_id: "call-read-reread-changed-2".to_string(),
                tool_name: "read_file".to_string(),
                outcome: TurnItemOutcome::Success,
                content: final_content.clone(),
            },
        },
    ];

    let rendered = assemble_native_prompt(&config, &input, &history, &contracts);

    assert_eq!(rendered.assembly.active_code_window_count, 1);
    assert!(rendered.prompt.contains("[code_window:clean:read_file]"));
    assert!(rendered
        .prompt
        .contains("call_id=call-read-reread-changed-2"));
    assert!(rendered.prompt.contains("freshness=reread_changed"));
    assert!(rendered.prompt.contains(&final_content));
    assert!(!rendered.prompt.contains(&rewritten_content));
    assert!(!rendered.prompt.contains("[tool_call:write_file]"));
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
                    prompt_response: None,
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
fn tool_action_parse_ignores_trailing_annotations_after_json_payload() {
    let action = NativeToolAction::parse(
        r#"tool:list_files:{"path":"src","recursive":true} [tool_call:list_files] {"input":{"path":"src"}}"#,
    )
    .expect("parse")
    .expect("tool action");

    assert_eq!(action.name, "list_files");
    assert_eq!(action.input, json!({"path": "src", "recursive": true}));
}

#[test]
fn tool_action_parse_accepts_whitespace_separator_before_json_payload() {
    let action = NativeToolAction::parse(r#"tool:read_file {"path":"note.txt"}"#)
        .expect("parse")
        .expect("tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(action.input, json!({"path": "note.txt"}));
}

#[test]
fn tool_action_parse_accepts_inline_json_payload_without_separator() {
    let action = NativeToolAction::parse(r#"tool:read_file{"path":"note.txt"}"#)
        .expect("parse")
        .expect("tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(action.input, json!({"path": "note.txt"}));
}

#[test]
fn tool_action_parse_accepts_xml_parameter_payload() {
    let action = NativeToolAction::parse(
        "tool:list_files:\n<path>/tmp/worktree/src</path>\n<recursive>false</recursive>\n<include_hidden>false</include_hidden>\n<tool_name>list_files</tool_name>",
    )
    .expect("xml parameter payload should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "list_files");
    assert_eq!(
        action.input,
        json!({
            "path": "/tmp/worktree/src",
            "recursive": false,
            "include_hidden": false,
        })
    );
}

#[test]
fn tool_action_parse_accepts_act_with_param_name_tool_and_payload() {
    let action = NativeToolAction::parse(
        "tool:ACT\n<param name=\"tool\">list_files</param>\n<param name=\"payload\">{\"path\":\"src/adapters\",\"recursive\":true}</param>",
    )
    .expect("param-wrapped ACT should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "list_files");
    assert_eq!(
        action.input,
        json!({"path": "src/adapters", "recursive": true})
    );
}

#[test]
fn tool_action_parse_accepts_param_name_xml_payload_for_inline_tool_name() {
    let action = NativeToolAction::parse(
        "tool:run_command\n<param name=\"command\">ls -la src/native</param>\n<param name=\"args\">[]</param>",
    )
    .expect("param-name payload should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "run_command");
    assert_eq!(
        action.input,
        json!({"command": "ls -la src/native", "args": []})
    );
}

#[test]
fn tool_action_parse_accepts_ruby_hash_style_structured_tool_action() {
    let action = NativeToolAction::parse(
        "tool:{tool => \"read_file\", args => { --path \"src/native/prompt_assembly.rs\" }}",
    )
    .expect("ruby-hash tool action should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(
        action.input,
        json!({"path": "src/native/prompt_assembly.rs"})
    );
}

#[test]
fn tool_action_parse_accepts_loose_structured_tool_action_with_unquoted_keys() {
    let action = NativeToolAction::parse(
        "tool:{tool: \"list_files\", version: \"1.0.0\", input: {\"path\":\"src/native\",\"recursive\":true}}",
    )
    .expect("loose structured action should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "list_files");
    assert_eq!(
        action.input,
        json!({"path": "src/native", "recursive": true})
    );
}

#[test]
fn tool_action_parse_accepts_loose_structured_tool_action_with_cli_flag_args() {
    let action = NativeToolAction::parse(
        "tool:{tool: \"list_files\", args: {--path \"src/native\", --recursive true}}",
    )
    .expect("loose structured cli-flag action should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "list_files");
    assert_eq!(
        action.input,
        json!({"path": "src/native", "recursive": true})
    );
}

#[test]
fn tool_action_parse_accepts_prefixed_args_shell_payload() {
    let action = NativeToolAction::parse(
        "tool:read_file\nargs:--path \"src/adapters/runtime/telemetry_provenance.rs\"",
    )
    .expect("prefixed args shell payload should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(
        action.input,
        json!({"path": "src/adapters/runtime/telemetry_provenance.rs"})
    );
}

#[test]
fn tool_action_parse_accepts_tool_wrapper_leak_before_ruby_hash_payload() {
    let action = NativeToolAction::parse(
        "tool:tool] {tool => \"read_file\", args => { --path \"src/native/prompt_assembly.rs\" }}",
    )
    .expect("tool wrapper leak should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(
        action.input,
        json!({"path": "src/native/prompt_assembly.rs"})
    );
}

#[test]
fn tool_action_parse_accepts_tool_name_with_tool_call_prefix() {
    let action = NativeToolAction::parse(
        "tool:<tool_call>read_file:{\"path\":\"src/native/turn_items.rs\"}",
    )
    .expect("tool_call-prefixed tool name should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(action.input, json!({"path": "src/native/turn_items.rs"}));
}

#[test]
fn tool_action_parse_accepts_key_value_payload_without_json() {
    let action = NativeToolAction::parse("tool:read_file] path=\"src/native/prompt_assembly.rs\"")
        .expect("key=value payload should parse")
        .expect("expected tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(
        action.input,
        json!({"path": "src/native/prompt_assembly.rs"})
    );
}

#[test]
fn tool_action_parse_accepts_act_prefixed_structured_key_values() {
    let action = NativeToolAction::parse(
        "tool:ACT tool=\"read_file\" path=\"src/native/prompt_assembly.rs\"",
    )
    .expect("ACT-prefixed structured key values should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(
        action.input,
        json!({"path": "src/native/prompt_assembly.rs"})
    );
}

#[test]
fn tool_action_parse_accepts_tool_bracket_prefix_before_inline_json() {
    let action = NativeToolAction::parse(
        "tool:tool]list_files{\"path\":\"src/adapters/runtime\",\"recursive\":true}",
    )
    .expect("tool]-prefixed inline json should parse")
    .expect("expected tool action");

    assert_eq!(action.name, "list_files");
    assert_eq!(
        action.input,
        json!({"path": "src/adapters/runtime", "recursive": true})
    );
}

#[test]
fn tool_action_parse_accepts_tool_name_with_trailing_bracket_before_json_payload() {
    let action =
        NativeToolAction::parse("tool:read_file] {\"path\":\"src/native/prompt_assembly.rs\"}")
            .expect("trailing bracket tool name should parse")
            .expect("expected tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(
        action.input,
        json!({"path": "src/native/prompt_assembly.rs"})
    );
}

#[test]
fn tool_action_parse_accepts_whitespace_separator_with_trailing_annotations() {
    let action = NativeToolAction::parse(
        "tool:read_file {\"path\":\"src/native/prompt_assembly.rs\"}\n[assistant:act] ACT:tool:read_file:{\"path\":\"src/native/turn_items.rs\"}",
    )
    .expect("parse")
    .expect("tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(
        action.input,
        json!({"path": "src/native/prompt_assembly.rs"})
    );
}

#[test]
fn tool_action_parse_rejects_truncated_inline_json_payload_without_separator() {
    let error = NativeToolAction::parse(r#"tool:read_file{"path":"#)
        .expect_err("truncated inline JSON payload should be rejected");

    assert_eq!(error.code, "native_tool_input_invalid");
    assert!(error.message.contains("tool input JSON payload is invalid"));
}

#[test]
fn tool_action_parse_recovers_wrapper_leaked_structured_payload() {
    let action = NativeToolAction::parse(
        r#"tool:ACT] {"tool":"write_file","args":{"path":"src/native/turn_items.rs","content":"patched","append":false}}[/tool_call]"#,
    )
    .expect("parse")
    .expect("tool action");

    assert_eq!(action.name, "write_file");
    assert_eq!(
        action.input,
        json!({"path": "src/native/turn_items.rs", "content": "patched", "append": false})
    );
}

#[test]
fn tool_action_parse_recovers_wrapper_leaked_plain_payload() {
    let action = NativeToolAction::parse(
        "tool:ACT] tool:list_files {\"include_hidden\": false, \"path\": \"src/core/registry\", \"recursive\": true}\n[/tool_call",
    )
    .expect("parse")
    .expect("tool action");

    assert_eq!(action.name, "list_files");
    assert_eq!(
        action.input,
        json!({"include_hidden": false, "path": "src/core/registry", "recursive": true})
    );
}

#[test]
fn tool_action_parse_preserves_valid_tool_name_with_structured_json_input() {
    let action = NativeToolAction::parse(
        r#"tool:read_file {"tool":"write_file","args":{"path":"note.txt"}}"#,
    )
    .expect("parse")
    .expect("tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(
        action.input,
        json!({"tool": "write_file", "args": {"path": "note.txt"}})
    );
}

#[test]
fn tool_action_parse_keeps_incomplete_bare_action_without_fabricating_input() {
    let action = NativeToolAction::parse("tool:read_file")
        .expect("parse")
        .expect("tool action");

    assert_eq!(action.name, "read_file");
    assert_eq!(action.input, json!({}));
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
