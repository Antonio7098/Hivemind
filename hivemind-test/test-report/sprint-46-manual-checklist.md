# Sprint 46 Manual Checklist

Date: 2026-02-20
Owner: Antonio
Sprint: 46 (Graph Query Runtime Integration)

## 46.1 Graph Query Contract

- [x] Validate `graph query neighbors` bounded deterministic response ordering.
- [x] Validate `graph query dependents` bounded deterministic response ordering.
- [x] Validate `graph query subgraph` depth and max-results bounds.
- [x] Validate `graph query filter` path/type filtering and deterministic ordering.

## 46.2 Runtime and Tool Integration

- [x] Validate native `graph_query` tool invocation path from runtime.
- [x] Validate CLI graph query inspection commands (`graph query *`) return structured outputs.
- [x] Validate query telemetry events include cost/result-size metrics (`graph_query_executed`).

## 46.3 Staleness and Integrity Gates

- [x] Validate stale snapshot hard-fails runtime graph query with actionable refresh hint.
- [x] Validate snapshot refresh clears stale gate and allows runtime graph query completion.
- [x] Validate canonical fingerprint remains stable across repeated equivalent queries.

## 46.4 Fresh-Clone Smoke (`hivemind-test`)

- [x] Run `hivemind-test/test_worktree.sh` with release binary.
- [x] Run `hivemind-test/test_execution.sh` with release binary.
- [x] Inspect canonical SQLite events for runtime and filesystem observations.

## Artifacts

- `hivemind-test/test-report/2026-02-20-sprint46-worktree.log`
- `hivemind-test/test-report/2026-02-20-sprint46-execution.log`
- `hivemind-test/test-report/2026-02-20-sprint46-events.log`
- `hivemind-test/test-report/2026-02-20-sprint46-fs-events.log`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-manual.log`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-determinism.json`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-event-slice.json`
- `hivemind-test/test-report/2026-02-20-sprint46-depth-bound.json`
- `hivemind-test/test-report/sprint-46-manual-report.md`
