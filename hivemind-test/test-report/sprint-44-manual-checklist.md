# Sprint 44 Manual Checklist

Date: 2026-02-19
Owner: Antonio
Sprint: 44 (Tool Engine v1)

## 44.1 Typed Tool Contracts and Dispatch

- [x] Verify native runtime uses typed tool contracts (`name`, `version`, input/output schema, scope/permission envelope).
- [x] Verify deterministic registry dispatch resolves built-in tools by explicit tool name.
- [x] Verify unknown tool names and invalid payload schemas fail with structured tool error codes.

## 44.2 Built-In Tool Set

- [x] Validate successful `list_files` native tool invocation path.
- [x] Validate `read_file`/`write_file` typed handler coverage via automated tests.
- [x] Validate `run_command` deny-by-default behavior.
- [x] Validate `git_status` and `git_diff` handlers and schema contracts via automated tests.
- [x] Confirm native invocation events include request/start/completion/failure for tool calls.

## 44.3 Safety and Policy Enforcement

- [x] Confirm task scope is projected to native runtime (`HIVEMIND_TASK_SCOPE_JSON`) and used for tool gating.
- [x] Confirm command policy controls are available:
  - `HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST`
  - `HIVEMIND_NATIVE_TOOL_RUN_COMMAND_DENYLIST`
  - `HIVEMIND_NATIVE_TOOL_RUN_COMMAND_DENY_BY_DEFAULT`
- [x] Confirm policy violations fail attempts deterministically and are attributable (`native_policy_violation`).

## 44.4 Replay/Inspectability

- [x] Confirm successful and denied tool paths are inspectable from event history (`tool_call_requested`, `tool_call_completed`, `tool_call_failed`, `agent_invocation_completed`).
- [x] Confirm deterministic replay property coverage for write/read tool sequences.

## 44.5 Fresh-Clone Smoke (`hivemind-test`)

- [x] Run `hivemind-test/test_worktree.sh` with release binary.
- [x] Run `hivemind-test/test_execution.sh` with release binary.
- [x] Inspect canonical SQLite event store for runtime/filesystem signals.

## Artifacts

- `hivemind-test/test-report/2026-02-19-sprint44-worktree.log`
- `hivemind-test/test-report/2026-02-19-sprint44-execution.log`
- `hivemind-test/test-report/2026-02-19-sprint44-events.log`
- `hivemind-test/test-report/2026-02-19-sprint44-native-tools-manual.log`
- `hivemind-test/test-report/sprint-44-manual-report.md`
