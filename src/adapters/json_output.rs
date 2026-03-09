use super::runtime::StructuredRuntimeObservation;
use crate::core::events::RuntimeOutputStream;
use serde_json::Value;
use std::convert::TryFrom;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonAdapterKind {
    OpenCode,
    Codex,
}

impl JsonAdapterKind {
    fn label(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedJsonAdapterOutput {
    pub stdout: String,
    pub stderr: String,
    pub parsed_event_count: usize,
    pub structured_runtime_observations: Vec<StructuredRuntimeObservation>,
}

pub(crate) fn transform_json_output(
    kind: JsonAdapterKind,
    raw_stdout: &str,
    raw_stderr: &str,
) -> ParsedJsonAdapterOutput {
    let mut stdout_lines = Vec::new();
    let mut stderr_lines = Vec::new();
    let mut parsed_event_count = 0;
    let mut structured_runtime_observations = Vec::new();

    for line in raw_stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_str::<Value>(trimmed) {
            Ok(value) => {
                parsed_event_count += 1;
                stderr_lines.push(format!("[{}.json] {trimmed}", kind.label()));

                match kind {
                    JsonAdapterKind::OpenCode => {
                        render_opencode_event(
                            &value,
                            &mut stdout_lines,
                            &mut stderr_lines,
                            &mut structured_runtime_observations,
                        );
                    }
                    JsonAdapterKind::Codex => {
                        render_codex_event(
                            &value,
                            &mut stdout_lines,
                            &mut stderr_lines,
                            &mut structured_runtime_observations,
                        );
                    }
                }
            }
            Err(_) => stdout_lines.push(trimmed.to_string()),
        }
    }

    if parsed_event_count == 0 {
        return ParsedJsonAdapterOutput {
            stdout: raw_stdout.to_string(),
            stderr: raw_stderr.to_string(),
            parsed_event_count,
            structured_runtime_observations,
        };
    }

    ParsedJsonAdapterOutput {
        stdout: join_lines(&stdout_lines),
        stderr: combine_outputs(raw_stderr, &join_lines(&stderr_lines)),
        parsed_event_count,
        structured_runtime_observations,
    }
}

fn render_opencode_event(
    value: &Value,
    stdout: &mut Vec<String>,
    stderr: &mut Vec<String>,
    structured_runtime_observations: &mut Vec<StructuredRuntimeObservation>,
) {
    match nested_str(value, &["type"]) {
        Some("text") => {
            if let Some(text) = nested_str(value, &["part", "text"]) {
                push_lines(stdout, text);
                structured_runtime_observations.extend(
                    extract_command_lines(text).into_iter().map(|command| {
                        StructuredRuntimeObservation::CommandCompleted {
                            stream: RuntimeOutputStream::Stdout,
                            command,
                            exit_code: None,
                            output: None,
                        }
                    }),
                );
            }
        }
        Some("tool_use") => {
            if let Some(tool) = nested_str(value, &["part", "tool"]) {
                stdout.push(format!("Tool: {tool}"));
            }
            if let Some(command) = nested_str(value, &["part", "state", "input", "command"]) {
                stdout.push(format!("Command: {command}"));
                structured_runtime_observations.push(
                    StructuredRuntimeObservation::CommandCompleted {
                        stream: RuntimeOutputStream::Stdout,
                        command: command.to_string(),
                        exit_code: nested_i64(value, &["part", "state", "exit_code"])
                            .and_then(|code| i32::try_from(code).ok()),
                        output: nested_str(value, &["part", "state", "output"])
                            .filter(|output| !output.trim().is_empty())
                            .map(ToString::to_string),
                    },
                );
            }
            if let Some(title) = nested_str(value, &["part", "state", "title"]) {
                push_lines(stdout, title);
            }
            if let Some(output) = nested_str(value, &["part", "state", "output"]) {
                push_lines(stdout, output);
            }
            if let Some(files) = nested_array(value, &["part", "state", "metadata", "files"]) {
                for file in files {
                    if let (Some(kind), Some(path)) = (
                        nested_str(file, &["type"]),
                        nested_str(file, &["relativePath"])
                            .or_else(|| nested_str(file, &["filePath"])),
                    ) {
                        stdout.push(format!("File change: {kind} {path}"));
                    }
                }
            }
        }
        Some("step_finish") => {
            if let Some(reason) = nested_str(value, &["part", "reason"]) {
                stdout.push(format!("Step finished: {reason}"));
            }
        }
        Some("error") => {
            if let Some(message) = nested_str(value, &["message"]) {
                stderr.push(format!("Error: {message}"));
            }
        }
        _ => {}
    }
}

fn render_codex_event(
    value: &Value,
    stdout: &mut Vec<String>,
    stderr: &mut Vec<String>,
    structured_runtime_observations: &mut Vec<StructuredRuntimeObservation>,
) {
    match nested_str(value, &["type"]) {
        Some("thread.started") => {
            if let Some(thread_id) = nested_str(value, &["thread_id"]) {
                stdout.push(format!("Thread started: {thread_id}"));
            }
        }
        Some("turn.started") => stdout.push("Turn started".to_string()),
        Some("turn.completed") => {
            let input_tokens = nested_i64(value, &["usage", "input_tokens"]).unwrap_or_default();
            let cached_input_tokens =
                nested_i64(value, &["usage", "cached_input_tokens"]).unwrap_or_default();
            let output_tokens = nested_i64(value, &["usage", "output_tokens"]).unwrap_or_default();
            stdout.push(format!(
                "Turn completed: input_tokens={input_tokens} cached_input_tokens={cached_input_tokens} output_tokens={output_tokens}"
            ));
        }
        Some("turn.failed") => {
            if let Some(message) = nested_str(value, &["error", "message"]) {
                stderr.push(format!("Error: {message}"));
            }
        }
        Some("error") => {
            if let Some(message) = nested_str(value, &["message"]) {
                stderr.push(format!("Error: {message}"));
            }
        }
        Some(event_type @ ("item.started" | "item.updated" | "item.completed")) => {
            if let Some(item) = nested_value(value, &["item"]) {
                render_codex_item(
                    event_type,
                    item,
                    stdout,
                    stderr,
                    structured_runtime_observations,
                );
            }
        }
        _ => {}
    }
}

fn render_codex_item(
    event_type: &str,
    item: &Value,
    stdout: &mut Vec<String>,
    stderr: &mut Vec<String>,
    structured_runtime_observations: &mut Vec<StructuredRuntimeObservation>,
) {
    match nested_str(item, &["type"]) {
        Some("agent_message") => {
            if let Some(text) = nested_str(item, &["text"]) {
                push_lines(stdout, text);
            }
        }
        Some("reasoning") => {
            if let Some(text) = nested_str(item, &["text"]) {
                stdout.push(format!("Thinking: {text}"));
            }
        }
        Some("command_execution") => {
            if event_type == "item.completed" {
                if let Some(command) = nested_str(item, &["command"]) {
                    stdout.push(format!("Command: {command}"));
                    structured_runtime_observations.push(
                        StructuredRuntimeObservation::CommandCompleted {
                            stream: RuntimeOutputStream::Stdout,
                            command: command.to_string(),
                            exit_code: nested_i64(item, &["exit_code"])
                                .and_then(|code| i32::try_from(code).ok()),
                            output: nested_str(item, &["aggregated_output"])
                                .filter(|output| !output.trim().is_empty())
                                .map(ToString::to_string),
                        },
                    );
                }
                if let Some(output) = nested_str(item, &["aggregated_output"]) {
                    push_lines(stdout, output);
                }
            }
        }
        Some("file_change") => {
            stdout.push("Tool: apply_patch".to_string());
            if let Some(changes) = nested_array(item, &["changes"]) {
                for change in changes {
                    if let (Some(kind), Some(path)) =
                        (nested_str(change, &["kind"]), nested_str(change, &["path"]))
                    {
                        stdout.push(format!("File change: {kind} {path}"));
                    }
                }
            }
        }
        Some("mcp_tool_call") => {
            if let (Some(server), Some(tool)) =
                (nested_str(item, &["server"]), nested_str(item, &["tool"]))
            {
                stdout.push(format!("Tool: mcp:{server}/{tool}"));
            }
            if let Some(message) = nested_str(item, &["error", "message"]) {
                stderr.push(format!("Error: {message}"));
            }
        }
        Some("collab_tool_call") => {
            if let Some(tool) = nested_str(item, &["tool"]) {
                stdout.push(format!("Tool: collab:{tool}"));
            }
        }
        Some("web_search") => {
            stdout.push("Tool: web_search".to_string());
            if let Some(query) = nested_str(item, &["query"]) {
                stdout.push(format!("Search query: {query}"));
            }
        }
        Some("todo_list") => {
            if let Some(items) = nested_array(item, &["items"]) {
                for todo in items {
                    if let Some(text) = nested_str(todo, &["text"]) {
                        let completed = nested_bool(todo, &["completed"]).unwrap_or(false);
                        stdout.push(format!(
                            "{}{}",
                            if completed { "DONE: " } else { "TODO: " },
                            text
                        ));
                    }
                }
            }
        }
        Some("error") => {
            if let Some(message) = nested_str(item, &["message"]) {
                stderr.push(format!("Error: {message}"));
            }
        }
        _ => {}
    }
}

fn nested_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn nested_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    nested_value(value, path)?.as_str()
}

fn nested_i64(value: &Value, path: &[&str]) -> Option<i64> {
    nested_value(value, path)?.as_i64()
}

fn nested_bool(value: &Value, path: &[&str]) -> Option<bool> {
    nested_value(value, path)?.as_bool()
}

fn nested_array<'a>(value: &'a Value, path: &[&str]) -> Option<&'a [Value]> {
    nested_value(value, path)?.as_array().map(Vec::as_slice)
}

fn push_lines(target: &mut Vec<String>, value: &str) {
    for line in value.lines() {
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            target.push(trimmed.to_string());
        }
    }
}

fn extract_command_lines(value: &str) -> Vec<String> {
    value
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(command) = trimmed.strip_prefix("$ ") {
                let command = command.trim();
                if !command.is_empty() {
                    return Some(command.to_string());
                }
            }
            if let Some(command) = trimmed.strip_prefix("Command: ") {
                let command = command.trim();
                if !command.is_empty() {
                    return Some(command.to_string());
                }
            }
            None
        })
        .collect()
}

fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn combine_outputs(primary: &str, secondary: &str) -> String {
    match (primary.trim(), secondary.trim()) {
        ("", "") => String::new(),
        ("", _) => secondary.to_string(),
        (_, "") => primary.to_string(),
        _ => format!("{}\n{}", primary.trim_end(), secondary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_opencode_json_and_preserves_raw_events() {
        let raw_stdout = concat!(
            r#"{"type":"tool_use","part":{"tool":"apply_patch","state":{"title":"Applied patch","input":{"command":"cargo test"},"output":"ok","metadata":{"files":[{"type":"add","relativePath":"src/main.rs"}]}}}}"#,
            "\n",
            r#"{"type":"text","part":{"text":"done"}}"#,
            "\n"
        );

        let parsed = transform_json_output(JsonAdapterKind::OpenCode, raw_stdout, "");

        assert!(parsed.stdout.contains("Tool: apply_patch"));
        assert!(parsed.stdout.contains("Applied patch"));
        assert!(parsed.stdout.contains("File change: add src/main.rs"));
        assert!(parsed.stdout.contains("done"));
        assert!(parsed.stderr.contains("[opencode.json]"));
        assert_eq!(
            parsed.structured_runtime_observations,
            vec![StructuredRuntimeObservation::CommandCompleted {
                stream: RuntimeOutputStream::Stdout,
                command: "cargo test".to_string(),
                exit_code: None,
                output: Some("ok".to_string()),
            }]
        );
    }

    #[test]
    fn transforms_opencode_text_payload_into_structured_command_events() {
        let raw_stdout = concat!(
            r#"{"type":"text","part":{"text":"$ cargo test\nTool: grep\nI will verify outputs now."}}"#,
            "\n"
        );

        let parsed = transform_json_output(JsonAdapterKind::OpenCode, raw_stdout, "");

        assert!(parsed.stdout.contains("$ cargo test"));
        assert_eq!(
            parsed.structured_runtime_observations,
            vec![StructuredRuntimeObservation::CommandCompleted {
                stream: RuntimeOutputStream::Stdout,
                command: "cargo test".to_string(),
                exit_code: None,
                output: None,
            }]
        );
    }

    #[test]
    fn transforms_codex_json_for_all_supported_item_shapes() {
        let raw_stdout = concat!(
            r#"{"type":"thread.started","thread_id":"thread-1"}"#,
            "\n",
            r#"{"type":"turn.started"}"#,
            "\n",
            r#"{"type":"item.started","item":{"id":"item-1","type":"command_execution","command":"/bin/bash -lc pwd","aggregated_output":"","exit_code":null,"status":"in_progress"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item-1","type":"command_execution","command":"/bin/bash -lc pwd","aggregated_output":"/tmp/runtime\n","exit_code":0,"status":"completed"}}"#,
            "\n",
            r#"{"type":"item.updated","item":{"id":"item-2","type":"todo_list","items":[{"text":"collect logs","completed":false},{"text":"write patch","completed":true}]}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item-3","type":"file_change","changes":[{"path":"src/lib.rs","kind":"update"}],"status":"completed"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item-4","type":"mcp_tool_call","server":"docs","tool":"search","arguments":{},"result":null,"error":null,"status":"completed"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item-5","type":"collab_tool_call","tool":"spawn_agent","sender_thread_id":"a","receiver_thread_ids":["b"],"prompt":null,"agents_states":{},"status":"completed"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item-6","type":"web_search","id":"call-1","query":"rust serde","action":"other"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item-7","type":"reasoning","text":"Need to inspect the config first."}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"item-8","type":"agent_message","text":"done"}}"#,
            "\n",
            r#"{"type":"error","message":"stream exploded"}"#,
            "\n",
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":5}}"#,
            "\n"
        );

        let parsed = transform_json_output(JsonAdapterKind::Codex, raw_stdout, "");

        assert!(parsed.stdout.contains("Thread started: thread-1"));
        assert!(parsed.stdout.contains("Turn started"));
        assert!(parsed.stdout.contains("Command: /bin/bash -lc pwd"));
        assert!(parsed.stdout.contains("/tmp/runtime"));
        assert!(parsed.stdout.contains("TODO: collect logs"));
        assert!(parsed.stdout.contains("DONE: write patch"));
        assert!(parsed.stdout.contains("Tool: apply_patch"));
        assert!(parsed.stdout.contains("File change: update src/lib.rs"));
        assert!(parsed.stdout.contains("Tool: mcp:docs/search"));
        assert!(parsed.stdout.contains("Tool: collab:spawn_agent"));
        assert!(parsed.stdout.contains("Tool: web_search"));
        assert!(parsed.stdout.contains("Search query: rust serde"));
        assert!(parsed
            .stdout
            .contains("Thinking: Need to inspect the config first."));
        assert!(parsed.stdout.contains("done"));
        assert!(parsed
            .stdout
            .contains("Turn completed: input_tokens=10 cached_input_tokens=2 output_tokens=5"));
        assert!(parsed.stderr.contains("Error: stream exploded"));
        assert!(parsed.stderr.contains("[codex.json]"));
        assert_eq!(
            parsed.structured_runtime_observations,
            vec![StructuredRuntimeObservation::CommandCompleted {
                stream: RuntimeOutputStream::Stdout,
                command: "/bin/bash -lc pwd".to_string(),
                exit_code: Some(0),
                output: Some("/tmp/runtime\n".to_string()),
            }]
        );
    }
}
