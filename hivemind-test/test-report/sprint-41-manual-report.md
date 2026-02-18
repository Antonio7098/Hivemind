# Sprint 41 Manual Test Report

Date: 2026-02-18
Scope: Phase 3 / Sprint 41 — Governance Hardening and Production Readiness

## Summary

Manual validation confirms Sprint 41 governance hardening capabilities are operational and observable:

- event queries support governance selectors (`artifact_id`, `template_id`, `rule_id`)
- governance diagnostics surface missing references and stale snapshots with actionable recovery hints
- replay verification remains consistent after governance/context-heavy execution activity
- artifact operations remain stable under concurrent flow activity
- fresh-clone `hivemind-test` smoke runs pass with expected runtime/task/governance telemetry

## Evidence Artifacts

- `23-sprint41-manual.log`
- `24-sprint41-ci-worktree.log`
- `25-sprint41-ci-execution.log`
- `26-sprint41-ci-events.log`
- `sprint-41-manual-checklist.md`

## Validations Performed

1. Event filter hardening
   - executed `events list` using `--template-id`, `--artifact-id`, and `--rule-id`
   - confirmed results are scoped to expected governance payloads (`template_instantiated`, `governance_artifact_upserted`, `constitution_violation_detected`)

2. Operator diagnostics behavior
   - created template with missing project document reference
   - confirmed diagnostics issue: `template_document_missing`
   - backfilled missing document and confirmed diagnostics returned `healthy: true`

3. Stale snapshot detection and recovery
   - committed repository change after snapshot refresh
   - confirmed diagnostics issue: `graph_snapshot_stale`
   - refreshed snapshot and confirmed diagnostics returned `healthy: true`

4. Concurrency stress path
   - ran two flow ticks concurrently while repeatedly instantiating templates and updating project documents
   - confirmed command stability and completion without hidden-state failures

5. Replay integrity
   - ran `events replay <flow-id> --verify` after governance/context activity
   - confirmed verification pass against current state

6. Fresh-clone smoke validation
   - executed `hivemind-test/test_worktree.sh` and `hivemind-test/test_execution.sh` in a fresh clone
   - inspected `/tmp/hivemind-test/.hm_home/.hivemind/events.jsonl` for runtime/task/governance event presence

## Exit Criteria Check

- [x] Governance/context features meet principle checkpoints with no hidden state
- [x] Operators can inspect, explain, and recover governance state end-to-end
- [x] Phase 3 capabilities are stable for broad project use
- [x] Manual validation in `@hivemind-test` is completed and documented
