# Beta Testing Fix Report: CLI Contract Hardening & Regression Coverage

> Based on Tier beta reports and regression runs; see source table for exact artifacts.

---

## 1. Metadata

| Field | Value |
|-------|-------|
| Fix Title | CLI Contract Hardening & Regression Coverage |
| Date | 2026-02-10 |
| Fix Window | 2026-02-09 → 2026-02-10 |
| Branch | `feat/beta-testing-fixes` |
| Owner(s) | Antonio |
| Related Issues / Reports | Tier-0/Tier-1/Tier-2 FINAL reports, SMK-006, PRJ-004, PRJ-008, TSK-003, TSK-006 |

---

## 2. Source Reports & Sprints

| Artifact / Information | Origin Sprint or Report | Reference / Link |
|------------------------|------------------------|------------------|
| Overall beta tier status | TIER-0 FINAL REPORT | `24-hour-hivemind/runs/tier_0_setup_smoke_must_pass_before_deeper_testing/TIER-0-FINAL-REPORT.md` |
| CLI smoke: SMK-006 | SMK-006 FINAL REPORT | `24-hour-hivemind/runs/tier_0_setup_smoke_must_pass_before_deeper_testing/SMK-006/SMK-006-FINAL-REPORT.md` |
| Project attach-repo behavior | Tier-1 PRJ-004 FINAL REPORT | `24-hour-hivemind/runs/tier_1_projects_repositories_core_registry_ux/tier_1_projects_repositories_core_registry_ux-FINAL-REPORT.md` |
| Task CRUD and state constraints | Tier-2 tasks FINAL REPORT | `24-hour-hivemind/runs/tier_2_tasks_crud_state_constraints/tier_2_tasks_crud_state_constraints-FINAL-REPORT.md` |

---

## 3. Problem Statement

- **Symptoms:**
  - Inconsistent CLI JSON contracts between success and failure paths made Tier scripts brittle and confusing, especially around `project attach-repo`, task inspection, and error envelopes.
  - Error events and docs did not clearly state when `ErrorOccurred` should be emitted, leading to potential spurious events for read-only commands.
- **Impact:**
  - Beta Tier suites (Tier-0/Tier-1/Tier-2) required custom parsing and were sensitive to envelope changes.
  - Operators lacked a single, reliable contract for machine consumption of CLI results.
- **Detection Source:**
  - Tier-0/Tier-1/Tier-2 FINAL reports and individual Tier scripts (e.g. PRJ-004, PRJ-008, TSK-003, TSK-006) highlighted drift between docs, implementation, and regression expectations.

---

## 4. Root Cause Summary

- **Primary Cause:**
  - Historical CLI helpers printed raw JSON data for some success paths while errors used a structured `CliResponse` envelope; documentation was partially out of sync with reality.
- **Contributing Factors:**
  - Older commands predated the unified envelope helper and never migrated.
  - Tier scripts initially assumed raw JSON arrays/objects instead of the newer `{ success, data, error }` contract.
- **Why Now:**
  - Beta testing and Tier report consolidation made these inconsistencies visible as blocking noise for automated verification.

---

## 5. Fix Scope

| Area | Changes |
|------|---------|
| CLI | Routed `print_project`/`print_projects`/`print_task`/`print_tasks` through `cli::output::output` for JSON/YAML, ensuring success envelopes while preserving table output. |
| Core/Registry | Ensured state-changing failures emit `ErrorOccurred` events; tightened attach-repo, runtime-set, and task create validation and error codes. |
| Docs | Updated `docs/design/cli-operational-semantics.md` and `docs/design/error-model.md` to match actual ErrorOccurred semantics and CLI envelopes. |
| Tooling/Tests | Updated Tier scripts (PRJ-004 invalid_paths/extended, PRJ-008 env parsing, TSK-003, TSK-006) to parse current envelopes and avoid spurious failures. |
| Other | N/A |

---

## 6. Validation & Regression Matrix

| Check | Result | Evidence / Command |
|-------|--------|---------------------|
| cargo fmt --all --check | PASS | `cargo fmt --all --check` |
| cargo clippy --all-targets --all-features -- -D warnings | PASS | `cargo clippy --all-targets --all-features -- -D warnings` |
| cargo test --all-features | PASS | `cargo test --all-features` (141 unit + 10 integration tests) |
| Tier / Shell Scripts | PASS | PRJ-004 invalid_paths + extended, PRJ-008 env_parsing, TSK-003 task_close_idempotence, TSK-006 task_inspect_not_found |
| Manual QA / CLI scenarios | PASS | Manual runs of key CLI flows (project attach/detach, runtime-set, task create/list/inspect/close) in table/json formats |

---

## 7. Observability & Documentation Updates

- `docs/design/error-model.md` now documents `ErrorOccurred` payload vs correlation metadata, and clarifies that only **state-changing** failures must emit `ErrorOccurred`.
- `docs/design/cli-operational-semantics.md` now includes updated commands (project/task inspect/update/detach-repo) and documents the structured CLI envelope shape for errors and successes.
- Tier scripts log explicit PASS/FAIL reasons tied to exit codes and envelope fields.

---

## 8. Residual Risk & Follow-ups

| Risk / Debt | Mitigation Plan | Owner | Due |
|-------------|-----------------|-------|-----|
| Some non-project/task commands may still bypass the success envelope and print raw JSON | Audit remaining CLI JSON paths and migrate them to `cli::output::output` where appropriate | Antonio | Future hardening pass |

---

## 9. Outstanding Actions

- [x] Changelog updated
- [ ] PR prepared per `ops/process/sprint-execution.md` (including description referencing this report)
- [ ] Additional TODOs (e.g. expand regression coverage to new commands)

---

## 10. Attachments / Evidence

- Tier FINAL reports referenced above.
- Latest Tier script outputs under `24-hour-hivemind/runs/.../results/` for PRJ-004, PRJ-008, TSK-003, TSK-006.
- `git diff` for `src/main.rs`, `src/cli/output.rs`, `src/core/registry.rs`, and docs.

---

_Report generated: 2026-02-10_
