# Beta Testing Fix Report: Tier 11+ Governance/Observability Stabilization

## 1. Metadata

| Field | Value |
|-------|-------|
| Fix Title | Tier 11+ governance replay/diagnose and anti-magic observability fixes |
| Date | 2026-02-19 |
| Fix Window | 2026-02-19 12:00 -> 12:35 UTC |
| Branch | `fix/beta-tier11plus-2026-02-19` |
| Owner(s) | Codex + Antonio |
| Related Issues / Reports | Tier 11-15 FINAL reports; ENF-005; RCV-001/004 findings |

## 2. Source Reports & Inputs

| Artifact / Information | Origin Sprint/Report | Reference / Link |
|------------------------|----------------------|------------------|
| Tier 11 aggregate status | Tier FINAL report | `24-hour-hivemind/runs/tier_11_governance_storage_and_artifact_lifecycle_sprint_34_35/tier_11_governance_storage_and_artifact_lifecycle_sprint_34_35-FINAL-REPORT.md` |
| Tier 12 aggregate status | Tier FINAL report | `24-hour-hivemind/runs/tier_12_constitution_and_graph_snapshot_contracts_sprint_36_37/tier_12_constitution_and_graph_snapshot_contracts_sprint_36_37-FINAL-REPORT.md` |
| Tier 13 aggregate status | Tier FINAL report | `24-hour-hivemind/runs/tier_13_constitution_enforcement_gates_sprint_38/tier_13_constitution_enforcement_gates_sprint_38-FINAL-REPORT.md` |
| Tier 14 aggregate status | Tier FINAL report | `24-hour-hivemind/runs/tier_14_deterministic_context_assembly_and_manifests_sprint_39/tier_14_deterministic_context_assembly_and_manifests_sprint_39-FINAL-REPORT.md` |
| Tier 15 scenario failures | Scenario FINAL reports | `24-hour-hivemind/runs/tier_15_governance_replay_snapshots_and_repair_sprint_40/RCV-001/RCV-001-FINAL-REPORT.md`, `24-hour-hivemind/runs/tier_15_governance_replay_snapshots_and_repair_sprint_40/RCV-004/RCV-004-FINAL-REPORT.md` |
| ENF anti-magic evidence | Scenario test harness | `24-hour-hivemind/runs/tier_13_constitution_enforcement_gates_sprint_38/ENF-005/tests/focused-anti-magic-test.sh` |

## 3. Problem Statement

- **Symptoms:**
  - `project governance replay --verify` returned success while projected artifacts were missing on disk.
  - `project governance diagnose` returned healthy state while projection-backed artifacts were missing.
  - Invalid `task start`/`task retry` operations returned CLI errors but emitted no `error_occurred` events (ENF anti-magic gap).
  - `events list` default limit (50) hid late events in larger runs.
- **Impact:**
  - Silent governance drift and incomplete event audit trails for operator incident response.
  - Tier 13/15 observability principles not fully enforced.
- **Detection Source:** Tier 13/15 beta reports + rerun harnesses and direct reproductions.

## 4. Root Cause Summary

- **Primary Cause:** replay verification logic checked replay idempotence/state parity but not filesystem projection presence.
- **Contributing Factors:** diagnose focused on constitution/template checks and did not include projection drift checks from repair planning.
- **Why Not Detected Earlier:** existing integration coverage did not include missing-on-disk projection failures for replay+diagnose and did not assert error event emission for start/retry invalid operations.

## 5. Fix Scope

| Area | Changes |
|------|---------|
| CLI | Raised `events list` default limit to 200; improved `task retry` mode help text |
| Core/Registry | replay verify now fails on missing projection files; diagnose now includes repair-plan drift findings; start/retry invalid paths now emit `error_occurred`; snapshot restore events now include `stale_files` + `repaired_projection_count` |
| Docs | Updated retry semantics, events limit default, governance runbook replay failure behavior, and clarified template document reference validation timing (instantiate-time) |
| Tooling/Tests | Added integration and unit regression tests for new failure/observability contracts, no-op migration event emission, and snapshot restore event payload completeness |

## 6. Validation & Regression Matrix

| Check | Result | Evidence / Command |
|-------|--------|---------------------|
| cargo fmt --all | PASS | `cd hivemind && cargo fmt --all` |
| Targeted governance integration tests | PASS | `cd hivemind && cargo test -q cli_governance_ -- --nocapture` |
| New error-event unit tests | PASS | `cargo test -q error_occurred_emitted_on_start_task_not_in_flow -- --nocapture`; `cargo test -q error_occurred_emitted_on_retry_task_not_in_flow -- --nocapture` |
| Migration no-op event regression | PASS | `cargo test -q project_governance_migrate_emits_event_when_no_legacy_artifacts_exist -- --nocapture` |
| Snapshot restore event payload regression | PASS | `cargo test -q governance_snapshot_create_and_restore_roundtrip -- --nocapture` |
| ENF-005 focused anti-magic script | PASS (assertions), script footer has arithmetic bug | `/tmp/enf005_focused_rerun.out` shows PASS for Test2/Test6 after fix |
| Snapshot restore event parity (manual) | PASS | `events list` now includes `stale_files` and `repaired_projection_count` in `governance_snapshot_restored` payload |
| RCV-004 error taxonomy script | PASS | `/tmp/rcv004_error_taxonomy_rerun.out`; `results/test4_drift_corrupt.json` now reports unhealthy with issues |
| RCV-004 additional script | PARTIAL (script exits at Test A4 by design) | `/tmp/rcv004_additional_rerun.out` |
| RCV-005 active-flow protections (manual equivalent) | PASS | `repair apply` and `snapshot restore` blocked while flow RUNNING; both succeed after abort (`governance_repair_blocked_active_flow`, `governance_restore_blocked_active_flow`) |
| RCV missing-artifact replay/diagnose manual repro | PASS | manual run now returns `governance_replay_verification_failed` + diagnose `governance_artifact_missing` |

## 7. DX Issues Identified (Optional)

| Issue | Severity | Description | Follow-up |
|-------|----------|-------------|-----------|
| Legacy beta scripts use mixed/legacy CLI assumptions | Low | Some scripts fail due harness issues (`-f json` placement, arithmetic parsing) rather than SUT behavior | Refresh tier scripts to current CLI contract |

## 8. Observability & Documentation Updates

- Event/schema changes:
  - Added `error_occurred` emission in `start_task_execution` and `retry_task` invalid-operation paths.
- Docs updated:
  - `docs/design/cli-semantics.md`
  - `docs/overview/governance-runbooks.md`
- Operational runbook/process updates:
  - Added this stabilization report and updated changelog entry.

## 9. Residual Risk & Follow-ups

| Risk / Debt | Mitigation Plan | Owner | Due |
|-------------|-----------------|-------|-----|
| Snapshot restore semantics vs intentional deletion expectations in RCV-002 | Clarify contract in runbooks + add explicit rollback mode if needed | Antonio | Sprint 42 planning |
| Additional ENF harness brittleness | Refresh tier scripts to current command grammar and stricter exit handling | Antonio | Next beta cycle |

## 10. Outstanding Actions

- [x] Changelog updated (`changelog.json`)
- [ ] PR opened and linked
- [ ] CI green
- [ ] Squash merge completed
- [ ] Follow-up tasks filed (if any)

## 11. Attachments / Evidence

- Repro and validation logs:
  - `/tmp/enf005_focused_rerun.out`
  - `/tmp/rcv004_error_taxonomy_rerun.out`
  - `/tmp/rcv004_additional_rerun.out`
- Tier artifacts:
  - `24-hour-hivemind/runs/tier_13_constitution_enforcement_gates_sprint_38/ENF-005/results/`
  - `24-hour-hivemind/runs/tier_15_governance_replay_snapshots_and_repair_sprint_40/RCV-004/results/`

_Report generated: 2026-02-19_
