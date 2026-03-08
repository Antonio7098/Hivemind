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
fn projects_narrative_from_task_prefixed_stdout_lines() {
    let mut projector = RuntimeEventProjector::new();
    let observed = projector.observe_chunk(
        RuntimeOutputStream::Stdout,
        "Task: Output exactly these five lines and nothing else:\n",
    );

    assert!(observed.iter().any(|obs| {
        matches!(
            obs,
            ProjectedRuntimeObservation::NarrativeOutputObserved { content, .. }
                if content == "Task: Output exactly these five lines and nothing else:"
        )
    }));
}

#[test]
fn projects_separate_interactive_chunks_without_newlines() {
    let mut projector = RuntimeEventProjector::new();

    let first = projector.observe_chunk(
        RuntimeOutputStream::Stdout,
        "Task: Output exactly these five lines and nothing else:",
    );
    assert!(first.is_empty());

    let second = projector.observe_chunk(RuntimeOutputStream::Stdout, "$ cargo test");
    assert!(second.iter().any(|obs| {
        matches!(
            obs,
            ProjectedRuntimeObservation::NarrativeOutputObserved { content, .. }
                if content == "Task: Output exactly these five lines and nothing else:"
        )
    }));

    let third = projector.observe_chunk(RuntimeOutputStream::Stdout, "Tool: grep");
    assert!(third.iter().any(|obs| {
        matches!(
            obs,
            ProjectedRuntimeObservation::CommandObserved { command, .. }
                if command == "cargo test"
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

#[test]
fn ignores_raw_provider_json_mirror_lines() {
    let mut projector = RuntimeEventProjector::new();
    let observed = projector.observe_chunk(
        RuntimeOutputStream::Stderr,
        "[opencode.json] {\"type\":\"text\",\"part\":{\"text\":\"$ cargo test\\nTool: grep\\n- [ ] collect logs\\nI will verify outputs now.\"}}\n",
    );

    assert!(observed.is_empty());
}
