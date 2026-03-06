//! Runtime output to observational event projection.
//!
//! This module extracts best-effort, non-authoritative runtime observations
//! from stdout/stderr chunks. These projections are telemetry only and must
//! never drive `TaskFlow` control flow.

use crate::core::events::RuntimeOutputStream;
use std::collections::BTreeMap;

mod normalize;
mod parser;

#[cfg(test)]
mod tests;

use normalize::{normalize_projection_line, starts_new_projection_line};
use parser::{is_narrative_line, parse_command, parse_todo_item, parse_tool_name};

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
        let mut projected = Vec::new();
        let trimmed_chunk = normalize_projection_line(chunk);
        let trimmed_chunk = trimmed_chunk.trim();

        if !trimmed_chunk.is_empty() && starts_new_projection_line(trimmed_chunk) {
            let buffer = self.buffer_for_stream(stream);
            if !buffer.trim().is_empty() {
                let buffered = std::mem::take(buffer);
                projected.extend(self.observe_line(stream, buffered.trim_end_matches('\r')));
            }
        }

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
