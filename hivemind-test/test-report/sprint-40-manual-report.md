# Sprint 40 Manual Report

Date: 2026-02-18
Owner: Antonio
Sprint: 40 (Governance Replay and Recovery)
Status: PASS

## Scope

Manual validation for Sprint 40 governance replay/recovery behavior, including:

- replay parity/idempotence verification
- bounded snapshot create/list/restore
- deterministic drift detect/preview/apply flows
- explicit recovery telemetry in event history
- fresh-clone smoke scripts under `hivemind-test/`

## Commands and Outcomes

1. Governance replay verification
   - Command: `hivemind -f json project governance replay proj --verify`
   - Outcome: `idempotent=true`, `current_matches_replay=true`.

2. Snapshot create/list/restore
   - Commands:
     - `project governance snapshot create proj`
     - `project governance snapshot list proj --limit 1`
     - `project governance snapshot restore proj <snapshot-id> --confirm`
   - Outcome: snapshots created/listed successfully; restore rewrote corrupted document when revision-compatible.

3. Drift detect/preview/apply
   - Commands:
     - `project governance repair detect proj`
     - `project governance repair preview proj --snapshot-id <snapshot-id>`
     - `project governance repair apply proj --snapshot-id <snapshot-id> --confirm`
   - Outcome: preview produced deterministic operation list (`emit_projection_upsert`, `restore_from_snapshot`); apply completed with `remaining_issue_count=0`.

4. Event audit trail
   - Outcome: event log contains
     - `governance_snapshot_created`
     - `governance_snapshot_restored`
     - `governance_drift_detected`
     - `governance_repair_applied`

5. Fresh clone smoke scripts (`hivemind-test`)
   - Outcome: `test_worktree.sh`, `test_execution.sh`, and event inspection completed with release binary path override.

## Artifacts

- `hivemind-test/test-report/27-sprint40-manual.log`
- `hivemind-test/test-report/28-sprint40-ci-worktree.log`
- `hivemind-test/test-report/29-sprint40-ci-execution.log`
- `hivemind-test/test-report/30-sprint40-ci-events.log`

## Notes

- Snapshot restore preserves event authority by skipping stale revision entries; a fresh snapshot was used after repair apply for content restore verification.
