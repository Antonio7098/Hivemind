# Sprint 44 Manual Report

Date: 2026-02-19
Owner: Antonio
Sprint: 44 (Tool Engine v1)
Status: PASS

## Scope

Manual validation for Sprint 44 native typed tool engine behavior:

- typed tool contract/dispatch semantics
- built-in tool success and failure paths
- scope/policy attribution and deterministic failure semantics
- event-level inspectability for replay and audit
- fresh-clone smoke validation via `hivemind-test`

## Commands and Outcomes

1. Fresh-clone smoke scripts
   - Commands:
     - `hivemind-test/test_worktree.sh`
     - `hivemind-test/test_execution.sh`
   - Outcome: both scripts completed with release binary and produced expected worktree/runtime artifacts.

2. Runtime event store inspection
   - Commands:
     - `sqlite3 -json /tmp/hivemind-test/.hm_home/.hivemind/db.sqlite ... | rg 'runtime_|task_execution_state_changed|merge_'`
     - `sqlite3 -json /tmp/hivemind-test/.hm_home/.hivemind/db.sqlite ... | rg 'runtime_filesystem_observed'`
   - Outcome: canonical SQLite event log contains runtime lifecycle, task transitions, and filesystem observation signals.

3. Native typed tool success path (`list_files`)
   - Commands:
     - configure project runtime with `ACT:tool:list_files:{...}` directive
     - create graph/flow and run `flow tick`
   - Outcome: event stream contains `tool_call_requested`, `tool_call_started`, `tool_call_completed`, and successful `agent_invocation_completed`.

4. Native policy denial path (`run_command`)
   - Commands:
     - configure project runtime with `ACT:tool:run_command:{...}` directive
     - create graph/flow and run `flow tick`
   - Outcome: event stream contains `tool_call_failed` with policy attribution and failed `agent_invocation_completed`; runtime classification recorded.

5. Trace inspectability checks
   - Commands:
     - `hivemind -f json events stream --flow <flow-id> | rg 'tool_call_|agent_invocation_|runtime_error_classified'`
   - Outcome: both success and failure flows are inspectable from event traces with explicit failure taxonomy.

## Artifacts

- `hivemind-test/test-report/2026-02-19-sprint44-worktree.log`
- `hivemind-test/test-report/2026-02-19-sprint44-execution.log`
- `hivemind-test/test-report/2026-02-19-sprint44-events.log`
- `hivemind-test/test-report/2026-02-19-sprint44-native-tools-manual.log`

## Notes

- `flow tick` return codes remain orchestration-centric; policy-denied native tool paths are validated by event/state outcomes (`TaskExecutionFailed`, `ToolCallFailed`, `RuntimeErrorClassified`) rather than shell exit status alone.
