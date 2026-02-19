# Beta Testing Fix Report: Tier 17 DB Recovery and Parity Hardening

## 1. Metadata

| Field | Value |
|-------|-------|
| Fix Title | Tier 17 DB append-only + recovery operability hardening |
| Date | 2026-02-19 |
| Fix Window | 2026-02-19 13:55 → 14:30 UTC |
| Branch | `fix/beta-tier11plus-2026-02-19` |
| Owner(s) | Codex + @antonio |
| Related Issues / Reports | DBM-001-CRIT, DBM-001-HIGH, DBM-001-MED, DBM-005-MED-001 |

## 2. Source Reports & Inputs

| Artifact / Information | Origin Sprint/Report | Reference / Link |
|------------------------|----------------------|------------------|
| Tier aggregate status | Tier 17 FINAL report | `../24-hour-hivemind/runs/tier_17_db_migration_interlude_validation_sprint_41_5/tier_17_db_migration_interlude_validation_sprint_41_5-FINAL-REPORT.md` |
| Scenario-level evidence | DBM-001 FINAL report | `../24-hour-hivemind/runs/tier_17_db_migration_interlude_validation_sprint_41_5/DBM-001/DBM-001-FINAL-REPORT.md` |
| Scenario-level evidence | DBM-005 FINAL report | `../24-hour-hivemind/runs/tier_17_db_migration_interlude_validation_sprint_41_5/DBM-005/DBM-005-FINAL-REPORT.md` |

## 3. Problem Statement

- **Symptoms:** Canonical SQLite event history could be desynchronized from `events.jsonl` after direct/manual DB corruption and there was no operator CLI recovery path.
- **Impact:** Recovery required ad-hoc manual DB surgery; production safety posture was incomplete.
- **Detection Source:** Tier 17 beta reliability run (DBM-001/005).

## 4. Root Cause Summary

- **Primary Cause:** Missing operational primitives for event-store parity verification and canonical reconstruction from mirror history.
- **Contributing Factors:** Legacy test harnesses assumed raw-array JSON output and undercounted results against wrapped CLI envelopes.
- **Why Not Detected Earlier:** Sprint 41.5 migration focused first on canonical DB correctness and parity under normal writes, not explicit corruption recovery workflows.

## 5. Fix Scope

| Area | Changes |
|------|---------|
| CLI | Added `events verify` and `events recover --from-mirror --confirm` commands |
| Core/Registry | Implemented mirror parsing, parity/integrity summaries, and safe SQLite rebuild from mirror with backup + post-recovery verification |
| Storage | Added DB-level append-only triggers (already in this tier fix set) |
| Docs | Updated `docs/design/cli-semantics.md` and `docs/overview/governance-runbooks.md` for verify/recover contracts and runbook |
| Tooling/Tests | Added integration regression for verify/recover recovery path and confirmation gate |

## 6. Validation & Regression Matrix

| Check | Result | Evidence / Command |
|-------|--------|---------------------|
| `cargo fmt --all` | PASS | local run |
| Targeted regression tests | PASS | `cargo test -q sqlite_store_enforces_append_only_triggers -- --nocapture` |
| Targeted regression tests | PASS | `cargo test -q cli_events_verify_and_recover_from_mirror_restores_canonical_db -- --nocapture` |
| Targeted regression tests | PASS | `cargo test -q cli_project_governance_init_accepts_project_flag -- --nocapture` |
| Targeted regression tests | PASS | `cargo test -q cli_task_commands_accept_legacy_project_task_arity -- --nocapture` |
| Beta script rerun (Tier 17) | PASS with known harness false negatives | `DBM-002/tests/contention_test.sh` |
| Beta script rerun (Tier 17) | PASS | `DBM-003/tests/parity_test.sh` |

### Notes on `DBM-002` rerun

- Core store signals remained healthy in script output:
  - Final sequence integrity: `Events: 116; Sequences: 0-115; Gaps: 0`
  - Final parity: `SQLite: 116, Mirror: 116`
- Script-level failures are caused by outdated jq selectors (`jq 'length'`, `.[].event_id`) that do not account for the CLI envelope (`{"success":true,"data":[...]}`).

## 7. Observability & Documentation Updates

- Event/schema changes:
  - Added operator-visible event-store verification/recovery outputs (no new event payload type emitted).
- Docs updated:
  - `docs/design/cli-semantics.md`
  - `docs/overview/governance-runbooks.md`
- Changelog:
  - Updated `changelog.json` (`v0.1.36`) with verify/recover + regression coverage entries.

## 8. Residual Risk & Follow-ups

| Risk / Debt | Mitigation Plan | Owner | Due |
|-------------|-----------------|-------|-----|
| DBM-002 harness still reports false negatives from jq contract mismatch | Update harness jq selectors to `.data | length` and `.data[].metadata.id` | Beta harness owner | next Tier 17 rerun |

## 9. Outstanding Actions

- [x] Changelog updated (`changelog.json`)
- [ ] PR opened and linked
- [ ] CI green
- [ ] Squash merge completed
- [ ] Follow-up tasks filed (if any)

## 10. Attachments / Evidence

- Logs/artifacts:
  - `../24-hour-hivemind/runs/tier_17_db_migration_interlude_validation_sprint_41_5/DBM-002/results/`
  - `../24-hour-hivemind/runs/tier_17_db_migration_interlude_validation_sprint_41_5/DBM-003/results/parity_report.json`

_Report generated: 2026-02-19_
