# Sprint 41.5 Manual Checklist (Phase 3.5 Interlude)

Date: 2026-02-18
Scope: DB Migration Interlude (filesystem -> DB canonical event storage)

## Environment

- [x] Manual run executed with isolated `HOME`
- [x] Release binary built from current workspace changes
- [x] Evidence captured under `hivemind-test/test-report/`

## 41.5.1 Canonical Event Store Migration

- [x] CLI project/task/graph/flow/worktree lifecycle runs successfully using DB-backed store
- [x] Event sequencing remains monotonic and queryable from `db.sqlite`
- [x] Runtime/task/governance events remain visible via CLI commands

Evidence: `31-sprint41-5-worktree.log`, `32-sprint41-5-execution.log`, `33-sprint41-5-events.log`

## 41.5.2 Compatibility and Operability

- [x] `events.jsonl` mirror is still emitted during DB-backed operation
- [x] Mirror contains runtime event payloads expected by operator inspection workflows
- [x] DB and mirror event counts are consistent for manual run

Evidence: `33-sprint41-5-events.log`

## 41.5.3 Smoke Scripts (`@hivemind-test`)

- [x] `hivemind-test/test_worktree.sh` passes
- [x] `hivemind-test/test_execution.sh` passes
- [x] Event store inspection confirms DB and mirror observability

Evidence: `31-sprint41-5-worktree.log`, `32-sprint41-5-execution.log`, `33-sprint41-5-events.log`

## 41.5.4 Exit Criteria Mapping

- [x] Canonical event persistence is DB-backed and transactional
- [x] Existing CLI/event/replay semantics are non-regressed
- [x] Observability remains explicit through CLI plus compatibility mirror
- [x] Manual validation in `@hivemind-test` is completed and documented
