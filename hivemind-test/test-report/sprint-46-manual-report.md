# Sprint 46 Manual Report

Date: 2026-02-20
Owner: Antonio
Sprint: 46 (Graph Query Runtime Integration)
Status: PASS

## Scope

Manual validation for Sprint 46 graph-query substrate/runtime integration:

- deterministic bounded graph query primitives
- CLI graph query inspection path
- native runtime `graph_query` tool integration
- stale snapshot hard-fail behavior with recovery hint
- query telemetry/event observability
- fresh-clone smoke checks via `hivemind-test`

## Commands and Outcomes

1. Fresh-clone smoke scripts
   - Commands:
     - `hivemind-test/test_worktree.sh`
     - `hivemind-test/test_execution.sh`
   - Outcome: both scripts completed with release binary and produced expected worktree/runtime artifacts.

2. CLI determinism and bounds checks
   - Commands:
     - `hivemind -f json graph query filter sprint46-manual --type file --path src --max-results 2` (run twice)
     - `hivemind -f json graph query subgraph sprint46-manual --seed main::src/main.rs --depth 10 --max-results 25`
   - Outcome:
     - repeated filter queries returned identical ordered node sets and canonical fingerprint
     - oversized depth rejected with `graph_query_depth_exceeded`

3. Runtime stale snapshot hard-fail behavior
   - Commands:
     - configure native runtime with `ACT:tool:graph_query:{...}`
     - mutate repository HEAD without refreshing snapshot
     - `hivemind flow tick <flow-id>`
   - Outcome:
     - runtime emitted `tool_call_failed` with `graph_snapshot_stale`
     - failure message includes hint: `Run: hivemind graph snapshot refresh <project>`

4. Runtime success path after refresh
   - Commands:
     - `hivemind graph snapshot refresh sprint46-manual`
     - run native flow tick with `graph_query` tool action
     - inspect events stream
   - Outcome:
     - tool call completed and event stream includes `graph_query_executed` with `source=native_tool_graph_query`

5. Event observability checks
   - Commands:
     - `hivemind -f json events stream --project sprint46-manual --limit 800`
   - Outcome:
     - query telemetry and failure signals are explicit and attributable (`graph_query_executed`, `tool_call_failed`, `agent_invocation_completed`, `runtime_error_classified`)

## Artifacts

- `hivemind-test/test-report/2026-02-20-sprint46-worktree.log`
- `hivemind-test/test-report/2026-02-20-sprint46-execution.log`
- `hivemind-test/test-report/2026-02-20-sprint46-events.log`
- `hivemind-test/test-report/2026-02-20-sprint46-fs-events.log`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-manual.log`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-determinism.json`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-event-slice.json`
- `hivemind-test/test-report/2026-02-20-sprint46-depth-bound.json`

## Notes

- Task default checkpoints still gate flow completion. In successful runtime graph-query path, `flow tick` may return `checkpoints_incomplete` while still emitting native tool/graph-query telemetry events; event stream is the authoritative execution trace.
