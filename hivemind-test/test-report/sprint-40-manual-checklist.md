# Sprint 40 Manual Checklist

Date: 2026-02-18
Owner: Antonio
Sprint: 40 (Governance Replay and Recovery)

## 40.1 Replay Semantics for Governance

- [x] Verify `project governance replay <project> --verify` rebuilds governance projections and reports parity/idempotence.
- [x] Verify replay output includes governance projection entries and on-disk existence state.
- [x] Verify replay verification failures are explicit (structured error path covered by automated tests).

## 40.2 Snapshot and Restore

- [x] Create governance snapshot and verify summary metadata (`snapshot_id`, `artifact_count`, `source_event_sequence`).
- [x] List snapshots and verify latest snapshot appears with stable metadata.
- [x] Restore snapshot with `--confirm` and verify content is restored when revisions match.
- [x] Verify stale snapshot entries are skipped and surfaced in restore output.

## 40.3 Corruption and Drift Handling

- [x] Detect drift for missing governance artifact file.
- [x] Preview deterministic repair operations with and without snapshot binding.
- [x] Apply deterministic repair plan (`--confirm`) and verify remaining issues drop to zero.
- [x] Verify structured drift issue taxonomy includes recoverable/unrecoverable counts.

## 40.4 Event and Audit Trail

- [x] Verify events include:
  - `governance_snapshot_created`
  - `governance_snapshot_restored`
  - `governance_drift_detected`
  - `governance_repair_applied`

## 40.5 Fresh-Clone Smoke (`hivemind-test`)

- [x] Run `hivemind-test/test_worktree.sh` on fresh clone with release binary.
- [x] Run `hivemind-test/test_execution.sh` on fresh clone with release binary.
- [x] Inspect generated event store and confirm expected runtime/graph/governance telemetry is present.

## Artifacts

- `hivemind-test/test-report/27-sprint40-manual.log`
- `hivemind-test/test-report/28-sprint40-ci-worktree.log`
- `hivemind-test/test-report/29-sprint40-ci-execution.log`
- `hivemind-test/test-report/30-sprint40-ci-events.log`
- `hivemind-test/test-report/sprint-40-manual-report.md`
