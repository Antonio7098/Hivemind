# Sprint 38 Manual Checklist (Phase 3)

Date: 2026-02-18
Scope: Constitution Enforcement

## Environment

- [x] Manual run executed with isolated `HOME`
- [x] Repositories initialized with deterministic commits for reproducible graph snapshots
- [x] Evidence captured in command logs under `hivemind-test/test-report/`

## 38.1 Enforcement Engine

- [x] Manual `constitution check --project` evaluates `validate(graph_snapshot, constitution)` using graph snapshot evidence
- [x] Severity-aware outcomes are visible: `hard`, `advisory`, and `informational`
- [x] Violation payloads include rule IDs, rule types, evidence, and remediation hints

Evidence: `15-sprint38-manual.log`

## 38.2 Enforcement Gates

- [x] Checkpoint completion gate blocks on hard constitution violations (`checkpoint_complete`)
- [x] Merge prepare gate blocks on hard constitution violations (`merge_prepare`)
- [x] Merge approve gate blocks on hard constitution violations (`merge_approve`)
- [x] Merge execute gate blocks on hard constitution violations (`merge_execute`)

Evidence: `15-sprint38-manual.log`

## 38.3 UX and Explainability

- [x] `constitution check` output reports hard/advisory/informational counters and blocked status
- [x] Gate failures return explicit `constitution_hard_violation` policy errors with remediation hint
- [x] `constitution_violation_detected` events are emitted with gate/rule context and remain queryable

Evidence: `15-sprint38-manual.log`

## 38.4 End-to-End Regression (`@hivemind-test`)

- [x] Fresh-clone `test_worktree.sh` smoke run passes with release binary
- [x] Fresh-clone `test_execution.sh` smoke run passes with release binary
- [x] Event store inspection confirms runtime/task observability remains intact

Evidence: `16-sprint38-ci-worktree.log`, `17-sprint38-ci-execution.log`, `18-sprint38-ci-events.log`

## 38.5 Exit Criteria Mapping

- [x] Hard violations block progression deterministically
- [x] Advisory/informational violations remain visible and queryable
- [x] Enforcement behavior is deterministic under replay-compatible event history
- [x] Manual validation in `@hivemind-test` is completed and documented
