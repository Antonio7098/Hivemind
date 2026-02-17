# Sprint 37 Manual Test Report

Date: 2026-02-17
Scope: Phase 3 / Sprint 37 — UCP Graph Integration and Snapshot Projection

## Summary

Manual validation confirms Sprint 37 behavior is operational and observable:

- graph snapshots are generated from UCP extraction and persisted under project governance storage
- snapshot lifecycle events are emitted on project attach, manual refresh, checkpoint completion, and merge completion
- constitution lifecycle commands enforce fresh snapshot provenance and fail loudly on stale snapshots
- non-constitution project execution flows continue to function without new mandatory coupling

## Evidence Artifacts

- `08-sprint37-manual.log`
- `09-sprint37-worktree.log`
- `10-sprint37-execution.log`
- `13-sprint37-ci-worktree.log`
- `14-sprint37-ci-execution.log`
- `sprint-37-manual-checklist.md`

## Validations Performed

1. Snapshot creation and command surface
   - created project + attached repository
   - observed automatic snapshot refresh (`trigger=project_attach`)
   - executed `hivemind graph snapshot refresh proj37` and observed `trigger=manual_refresh`

2. Constitution staleness gate
   - initialized constitution on fresh snapshot
   - mutated repository HEAD and validated constitution failure (`graph_snapshot_stale`)
   - refreshed snapshot and re-ran constitution validation successfully (`valid=true`)

3. Checkpoint and merge lifecycle triggers
   - executed flow with runtime-driven checkpoint completion
   - observed snapshot refresh event chain on checkpoint completion (`trigger=checkpoint_complete`)
   - executed merge lifecycle (`prepare/approve/execute --mode local`)
   - observed snapshot refresh event chain on merge completion (`trigger=merge_completed`)

4. Regression checks in `@hivemind-test`
   - executed `test_worktree.sh` and `test_execution.sh` with release binary override
   - confirmed baseline worktree/execution flows remain stable with Sprint 37 integration

## Exit Criteria Check

- [x] Hivemind uses UCP graph engine as authoritative extraction backend
- [x] Hivemind accepts only profile-compliant UCP graph artifacts
- [x] Snapshot lifecycle triggers and event telemetry are explicit and observable
- [x] Constitution engine receives stable, reproducible graph input
- [x] Hivemind-owned versioning/eventing/failure semantics are fully enforced
- [x] Manual validation in `@hivemind-test` is completed and documented
