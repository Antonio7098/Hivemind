# Beta Testing Fix Report

## 1. Metadata

| Field | Value |
|-------|-------|
| Fix Title | Tier 6-9 Beta Stabilization (scope, merge observability, abort consistency, dependency semantics) |
| Date | 2026-02-15 |
| Fix Window | 2026-02-15 → 2026-02-15 |
| Branch | `fix/tier6-9-beta-stabilization` |
| Owner(s) | Codex + Antonio |
| Related Issues / Reports | Tier 6-9 FINAL reports + ART/MRG/EVT/REL individual reports |

---

## 2. Source Reports & Sprints

| Artifact / Information | Origin Sprint or Report | Reference / Link |
|------------------------|------------------------|------------------|
| Tier 6 status and failing findings | Tier 6 FINAL REPORT | `24-hour-hivemind/runs/tier_6_worktrees_attempts_diffs_and_scope_safety/tier_6_worktrees_attempts_diffs_and_scope_safety-FINAL-REPORT.md` |
| Scope violation gaps | ART-003 FINAL REPORT | `24-hour-hivemind/runs/tier_6_worktrees_attempts_diffs_and_scope_safety/ART-003/ART-003-FINAL-REPORT.md` |
| Attempt context gap | ART-004 FINAL REPORT | `24-hour-hivemind/runs/tier_6_worktrees_attempts_diffs_and_scope_safety/ART-004/ART-004-FINAL-REPORT.md` |
| Cleanup safety/observability gaps | ART-005 FINAL REPORT | `24-hour-hivemind/runs/tier_6_worktrees_attempts_diffs_and_scope_safety/ART-005/ART-005-FINAL-REPORT.md` |
| Tier 7 status and merge observability gaps | Tier 7 FINAL REPORT | `24-hour-hivemind/runs/tier_7_merge_protocol_and_branch_integrity/tier_7_merge_protocol_and_branch_integrity-FINAL-REPORT.md` |
| Merge precondition event gap | MRG-001 FINAL REPORT | `24-hour-hivemind/runs/tier_7_merge_protocol_and_branch_integrity/MRG-001/MRG-001-FINAL-REPORT.md` |
| Merge completed event reliability concern | MRG-002 FINAL REPORT | `24-hour-hivemind/runs/tier_7_merge_protocol_and_branch_integrity/MRG-002/MRG-002-FINAL-REPORT.md` |
| Abort/task inconsistency evidence | MRG-004 FINAL REPORT | `24-hour-hivemind/runs/tier_7_merge_protocol_and_branch_integrity/MRG-004/MRG-004-FINAL-REPORT.md` |
| Tier 8 status and correlation findings | Tier 8 FINAL REPORT | `24-hour-hivemind/runs/tier_8_events_replay_and_deterministic_operations/tier_8_events_replay_and_deterministic_operations-FINAL-REPORT.md` |
| Abort/correlation findings | EVT-004 FINAL REPORT | `24-hour-hivemind/runs/tier_8_events_replay_and_deterministic_operations/EVT-004/EVT-004-FINAL-REPORT.md` |
| Exit-code/event behavior sweep | EVT-005 FINAL REPORT | `24-hour-hivemind/runs/tier_8_events_replay_and_deterministic_operations/EVT-005/EVT-005-FINAL-REPORT.md` |
| Tier 9 interruption/recovery findings | REL-002 FINAL REPORT | `24-hour-hivemind/runs/tier_9_long_session_reliability_and_operator_recovery/REL-002/REL-002-FINAL-REPORT.md` |

---

## 3. Problem Statement

- Symptoms:
  - Scope violations outside worktree were not reliably detected.
  - `attempt inspect --context` did not return retry context.
  - `worktree cleanup` lacked force safety and audit event emission.
  - Flow abort left active task executions in non-terminal states.
  - Blocked `merge prepare` attempts were not visible on the flow timeline.
  - Dependency argument semantics were ambiguous, leading to false anomaly reports.
- Impact:
  - Broke observability/trust guarantees and caused operator confusion during incident/recovery drills.
- Detection Source:
  - Tier 6-9 beta reports from `24-hour-hivemind/runs`.

---

## 4. Root Cause Summary

- Primary Cause:
  - Partial implementation/documentation drift in observability and scope enforcement details for wrapper runtime paths.
- Contributing Factors:
  - Historical script/report assumptions that diverged from current CLI contracts.
  - Ambiguous dependency argument semantics (`graph add-dependency`).
- Why Now:
  - Tier 6-9 deep beta scenarios exercised adversarial and recovery paths that unit happy-paths did not fully cover.

---

## 5. Fix Scope

| Area | Changes |
|------|---------|
| CLI | Added `worktree cleanup --force/--dry-run`; completed `attempt inspect --context`; clarified dependency semantics in CLI arg docs |
| Core/Registry | Added scope baseline artifacts and ambient verification, optional syscall-trace enforcement, cleanup audit event, abort terminal transitions, merge precondition `error_occurred` emission |
| Docs | Updated `scope-enforcement.md`, `cli-semantics.md`, `quickstart.md` |
| Tooling/Tests | Added integration regressions for scope/context/cleanup/abort/merge observability/dependency readiness |
| Other | Added per-tier fix notes under `ops/reports` and changelog entry `v0.1.21` |

---

## 6. Validation & Regression Matrix

| Check | Result | Evidence / Command |
|-------|--------|---------------------|
| cargo fmt --all | PASS | `cd hivemind && cargo fmt --all` |
| cargo test --all-features --test integration -- --test-threads=1 | PASS | 22/22 tests passed |
| cargo test --all-features -- --test-threads=1 | PASS | unit + integration + doc tests passed |
| Tier / Shell Scripts | PASS (with caveat) | `ART-002/tests/test_worktree_inspection.sh` executed; `EVT-002/tests/test_json_contracts.sh`, `test_malformed_ids.sh`, `test_exit_codes.sh` executed. Some legacy script summary text is stale (static). |
| Manual QA / CLI scenarios | PASS | `/tmp/hm_tier6_9_validation.txt` (tier 6-9 matrix reproductions) |

---

## 7. Observability & Documentation Updates

- Added `WorktreeCleanupPerformed` event and CLI output for `--force`/`--dry-run`.
- Added `attempt_id` to `TaskExecutionStateChanged` payload for correlation.
- Recorded blocked merge prepare preconditions via `error_occurred` events.
- Updated scope enforcement docs to match implemented baseline + optional trace behavior.
- Clarified dependency semantics in CLI/design docs to prevent reversed-edge misuse.

---

## 8. Residual Risk & Follow-ups

| Risk / Debt | Mitigation Plan | Owner | Due |
|-------------|-----------------|-------|-----|
| Scope detection without syscall trace may still miss arbitrary non-`/tmp` absolute writes | Keep docs explicit; prioritize stronger runtime mediation in later sprint | Core | Next sprint |
| Legacy 24h scripts include stale assumptions and static summary text | Refresh scripts to current CLI contracts and exit behavior | QA/Tooling | Next sprint |
| REL-005 not yet completed by testers | Re-run same process when REL-005 final report is published | QA + Core | On report arrival |

---

## 9. Outstanding Actions

- [x] Changelog updated
- [ ] PR prepared per `ops/process/sprint-execution.md`
- [x] Additional TODOs

---

## 10. Attachments / Evidence

- Commits on branch:
  - `f0fd90a` (tier 6 core/docs/tests/changelog)
  - `8dc1072` (tier 7 note)
  - `f48c40f` (tier 8 note)
  - `68c6a1e` (tier 9 note)
- Validation logs:
  - `/tmp/hm_tier6_9_validation.txt`
  - `/tmp/evt002-json-contracts.out`
  - `/tmp/evt002-malformed.out`
  - `/tmp/evt002-exit-codes.out`
  - `/tmp/art002-worktree.out`

---

_Report generated: 2026-02-15_
