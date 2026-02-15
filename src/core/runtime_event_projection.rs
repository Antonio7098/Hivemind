//! Runtime output to observational event projection.
//!
//! This module extracts best-effort, non-authoritative runtime observations
//! from stdout/stderr chunks. These projections are telemetry only and must
//! never drive `TaskFlow` control flow.

use crate::core::events::RuntimeOutputStream;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectedRuntimeObservation {
    CommandObserved {
        stream: RuntimeOutputStream,
        command: String,
    },
    ToolCallObserved {
        stream: RuntimeOutputStream,
        tool_name: String,
        details: String,
    },
    TodoSnapshotUpdated {
        stream: RuntimeOutputStream,
        items: Vec<String>,
    },
    NarrativeOutputObserved {
        stream: RuntimeOutputStream,
        content: String,
    },
}

#[derive(Debug, Default)]
pub struct RuntimeEventProjector {
    stdout_buffer: String,
    stderr_buffer: String,
    todo_items: BTreeMap<String, bool>,
}

impl RuntimeEventProjector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe_chunk(
        &mut self,
        stream: RuntimeOutputStream,
        chunk: &str,
    ) -> Vec<ProjectedRuntimeObservation> {
        {
            let buffer = self.buffer_for_stream(stream);
            buffer.push_str(chunk);
        }

        let mut lines = Vec::new();
        {
            let buffer = self.buffer_for_stream(stream);
            while let Some(line) = Self::drain_next_line(buffer) {
                lines.push(line);
            }
        }

        let mut projected = Vec::new();
        for line in lines {
            projected.extend(self.observe_line(stream, &line));
        }
        projected
    }

    pub fn flush(&mut self) -> Vec<ProjectedRuntimeObservation> {
        let mut projected = Vec::new();

        if self.stdout_buffer.trim().is_empty() {
            self.stdout_buffer.clear();
        } else {
            let remaining = std::mem::take(&mut self.stdout_buffer);
            projected.extend(self.observe_line(
                RuntimeOutputStream::Stdout,
                remaining.trim_end_matches('\r'),
            ));
        }

        if self.stderr_buffer.trim().is_empty() {
            self.stderr_buffer.clear();
        } else {
            let remaining = std::mem::take(&mut self.stderr_buffer);
            projected.extend(self.observe_line(
                RuntimeOutputStream::Stderr,
                remaining.trim_end_matches('\r'),
            ));
        }

        projected
    }

    fn buffer_for_stream(&mut self, stream: RuntimeOutputStream) -> &mut String {
        match stream {
            RuntimeOutputStream::Stdout => &mut self.stdout_buffer,
            RuntimeOutputStream::Stderr => &mut self.stderr_buffer,
        }
    }

    fn drain_next_line(buffer: &mut String) -> Option<String> {
        let bytes = buffer.as_bytes();
        let mut newline_idx = None;
        let mut remove_len = 1usize;

        for (idx, b) in bytes.iter().enumerate() {
            if *b == b'\n' {
                newline_idx = Some(idx);
                remove_len = 1;
                break;
            }
            if *b == b'\r' {
                newline_idx = Some(idx);
                remove_len = if bytes.get(idx + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                break;
            }
        }

        let idx = newline_idx?;
        let line = buffer[..idx].to_string();
        buffer.drain(..idx + remove_len);
        Some(line)
    }

    fn observe_line(
        &mut self,
        stream: RuntimeOutputStream,
        raw_line: &str,
    ) -> Vec<ProjectedRuntimeObservation> {
        let normalized = normalize_projection_line(raw_line);
        let line = normalized.trim();
        if line.is_empty() {
            return Vec::new();
        }

        let mut projected = Vec::new();

        if let Some(command) = parse_command(line) {
            projected.push(ProjectedRuntimeObservation::CommandObserved { stream, command });
        }

        if let Some(tool_name) = parse_tool_name(line) {
            projected.push(ProjectedRuntimeObservation::ToolCallObserved {
                stream,
                tool_name,
                details: line.to_string(),
            });
        }

        if let Some((item, completed)) = parse_todo_item(line) {
            let changed = self
                .todo_items
                .insert(item, completed)
                .is_none_or(|prev| prev != completed);
            if changed {
                let items = self
                    .todo_items
                    .iter()
                    .map(|(todo, done)| {
                        if *done {
                            format!("[x] {todo}")
                        } else {
                            format!("[ ] {todo}")
                        }
                    })
                    .collect();
                projected.push(ProjectedRuntimeObservation::TodoSnapshotUpdated { stream, items });
            }
        }

        if is_narrative_line(line)
            && projected.is_empty()
            && matches!(stream, RuntimeOutputStream::Stdout)
        {
            projected.push(ProjectedRuntimeObservation::NarrativeOutputObserved {
                stream,
                content: line.to_string(),
            });
        }

        projected
    }
}

fn parse_command(line: &str) -> Option<String> {
    if let Some(rest) = line.find("Command: ").map(|idx| &line[idx..]) {
        let cmd = rest.trim_start_matches("Command: ").trim();
        if !cmd.is_empty() {
            return Some(cmd.to_string());
        }
    }
    if let Some(rest) = line.find("Running command: ").map(|idx| &line[idx..]) {
        let cmd = rest.trim_start_matches("Running command: ").trim();
        if !cmd.is_empty() {
            return Some(cmd.to_string());
        }
    }

    for prefix in [
        "$ ",
        "> ",
        "Command: ",
        "Running: ",
        "Running command: ",
        "Executing: ",
        "Execute: ",
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let cmd = rest.trim();
            if !cmd.is_empty() {
                return Some(cmd.to_string());
            }
        }
    }
    None
}

fn parse_tool_name(line: &str) -> Option<String> {
    if let Some(rest) = line.find("Tool: ").map(|idx| &line[idx..]) {
        let name = rest
            .trim_start_matches("Tool: ")
            .split_whitespace()
            .next()?;
        return Some(name.to_string());
    }
    if let Some(rest) = line.strip_prefix("Tool: ") {
        let name = rest.split_whitespace().next()?;
        return Some(name.to_string());
    }
    if let Some(rest) = line.strip_prefix("Using tool ") {
        let name = rest
            .split(|c: char| c.is_whitespace() || c == ':' || c == '(')
            .next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    if let Some(rest) = line.strip_prefix("tool=") {
        let name = rest
            .split(|c: char| c.is_whitespace() || c == ',' || c == ')')
            .next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

fn parse_todo_item(line: &str) -> Option<(String, bool)> {
    for (prefix, completed) in [
        ("- [ ] ", false),
        ("* [ ] ", false),
        ("- [x] ", true),
        ("* [x] ", true),
        ("- [X] ", true),
        ("* [X] ", true),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let item = rest.trim();
            if !item.is_empty() {
                return Some((item.to_string(), completed));
            }
        }
    }
    for (prefix, completed) in [
        ("TODO: ", false),
        ("TODO ", false),
        ("DONE: ", true),
        ("DONE ", true),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let item = rest.trim();
            if !item.is_empty() {
                return Some((item.to_string(), completed));
            }
        }
    }
    None
}

fn is_narrative_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("i ")
        || lower.starts_with("i'")
        || lower.starts_with("i\"")
        || lower.starts_with("next ")
        || lower.starts_with("plan:")
        || lower.starts_with("because")
        || lower.starts_with("thinking:")
        || lower.starts_with("hello")
        || lower.starts_with("starting")
        || lower.starts_with("working")
        || lower.starts_with("updating")
        || lower.starts_with("checking")
        || lower.starts_with("analyzing")
        || lower.starts_with("investigating")
}

fn normalize_projection_line(raw: &str) -> String {
    let no_ansi = strip_ansi_sequences(raw);
    no_ansi
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
        .collect::<String>()
}

fn strip_ansi_sequences(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek().is_some_and(|c| *c == '[') {
                let _ = chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
                continue;
            }
            continue;
        }
        out.push(ch);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_command_lines_from_stdout() {
        let mut projector = RuntimeEventProjector::new();
        let observed = projector.observe_chunk(RuntimeOutputStream::Stdout, "$ cargo test\n");

        assert_eq!(
            observed,
            vec![ProjectedRuntimeObservation::CommandObserved {
                stream: RuntimeOutputStream::Stdout,
                command: "cargo test".to_string(),
            }]
        );
    }

    #[test]
    fn projects_tool_and_todo_updates() {
        let mut projector = RuntimeEventProjector::new();
        let observed = projector.observe_chunk(
            RuntimeOutputStream::Stdout,
            "Tool: grep\n- [ ] collect logs\n- [x] collect logs\n",
        );

        assert!(observed.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::ToolCallObserved { tool_name, .. } if tool_name == "grep"
            )
        }));

        assert!(observed.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::TodoSnapshotUpdated { items, .. }
                    if items == &vec!["[x] collect logs".to_string()]
            )
        }));
    }

    #[test]
    fn handles_split_lines_across_chunks() {
        let mut projector = RuntimeEventProjector::new();
        let first = projector.observe_chunk(RuntimeOutputStream::Stdout, "Tool: git");
        assert!(first.is_empty());

        let second = projector.observe_chunk(RuntimeOutputStream::Stdout, " status\n");
        assert!(second.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::ToolCallObserved { tool_name, .. } if tool_name == "git"
            )
        }));
    }

    #[test]
    fn flushes_partial_lines_as_observations() {
        let mut projector = RuntimeEventProjector::new();
        let _ = projector.observe_chunk(
            RuntimeOutputStream::Stdout,
            "I will run verification checks next",
        );

        let flushed = projector.flush();
        assert_eq!(
            flushed,
            vec![ProjectedRuntimeObservation::NarrativeOutputObserved {
                stream: RuntimeOutputStream::Stdout,
                content: "I will run verification checks next".to_string(),
            }]
        );
    }

    #[test]
    fn projects_deterministic_markers_from_stderr() {
        let mut projector = RuntimeEventProjector::new();
        let observed = projector.observe_chunk(
            RuntimeOutputStream::Stderr,
            "Command: cargo clippy\nTool: rustc\n",
        );

        assert!(observed.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::CommandObserved { stream, command }
                    if *stream == RuntimeOutputStream::Stderr && command == "cargo clippy"
            )
        }));

        assert!(observed.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::ToolCallObserved {
                    stream,
                    tool_name,
                    ..
                } if *stream == RuntimeOutputStream::Stderr && tool_name == "rustc"
            )
        }));
    }

    #[test]
    fn ignores_noisy_lines_without_markers() {
        let mut projector = RuntimeEventProjector::new();
        let observed = projector.observe_chunk(
            RuntimeOutputStream::Stdout,
            "[12:30:44] ::::: non-structured runtime noise :::::\n",
        );

        assert!(observed.is_empty());
    }

    #[test]
    fn projects_todo_from_todo_prefixes() {
        let mut projector = RuntimeEventProjector::new();
        let observed = projector.observe_chunk(
            RuntimeOutputStream::Stdout,
            "TODO: collect logs\nDONE: collect logs\n",
        );

        assert!(observed.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::TodoSnapshotUpdated { items, .. }
                    if items == &vec!["[x] collect logs".to_string()]
            )
        }));
    }

    #[test]
    fn projects_narrative_from_runtime_status_lines() {
        let mut projector = RuntimeEventProjector::new();
        let observed = projector.observe_chunk(RuntimeOutputStream::Stdout, "Hello from runtime\n");

        assert!(observed.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::NarrativeOutputObserved { content, .. }
                    if content == "Hello from runtime"
            )
        }));
    }

    #[test]
    fn projects_markers_with_ansi_prefixes() {
        let mut projector = RuntimeEventProjector::new();
        let observed = projector.observe_chunk(
            RuntimeOutputStream::Stderr,
            "\u{1b}[0m→ Tool: git status\n\u{1b}[91mCommand: cargo test\u{1b}[0m\n",
        );

        assert!(observed.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::ToolCallObserved { tool_name, .. } if tool_name == "git"
            )
        }));
        assert!(observed.iter().any(|obs| {
            matches!(
                obs,
                ProjectedRuntimeObservation::CommandObserved { command, .. } if command == "cargo test"
            )
        }));
    }
}
