# Beta Testing Fix Report: Tier 0-2 Beta Gaps

---

## 1. Metadata

| Field | Value |
|-------|-------|
| Fix Title | Tier 0-2 Beta Gaps (CLI contract, list commands, graph/task DX hardening) |
| Date | 2026-02-14 |
| Fix Window | 2026-02-14 |
| Branch | `fix/tier-0-2-beta-gaps` |
| Owner(s) | Antonio |
| Related Issues / Reports | SMK-003-HIGH, SMK-004-LOW, PRJ-003-01, PRJ-005-01/02/03, TSK-004-02, TSK-005-01/02 |

---

## 2. Source Reports & Sprints

| Artifact / Information | Origin Sprint or Report | Reference / Link |
|------------------------|------------------------|------------------|
| Tier 0 final status | Tier 0 FINAL REPORT | `24-hour-hivemind/runs/tier_0_baseline_setup_and_trust_in_tooling_simple/tier_0_baseline_setup_and_trust_in_tooling_simple-FINAL-REPORT.md` |
| Output contract failure (`flow status`) | SMK-003 FINAL REPORT | `24-hour-hivemind/runs/tier_0_baseline_setup_and_trust_in_tooling_simple/SMK-003/SMK-003-FINAL-REPORT.md` |
| Missing list commands | SMK-004 FINAL REPORT | `24-hour-hivemind/runs/tier_0_baseline_setup_and_trust_in_tooling_simple/SMK-004/SMK-004-FINAL-REPORT.md` |
| Tier 1 final status | Tier 1 FINAL REPORT | `24-hour-hivemind/runs/tier_1_project_and_repository_contract_hardening/tier_1_project_and_repository_contract_hardening-FINAL-REPORT.md` |
| Missing hint in `repo_already_attached` | PRJ-003 FINAL REPORT | `24-hour-hivemind/runs/tier_1_project_and_repository_contract_hardening/PRJ-003/PRJ-003-FINAL-REPORT.md` |
| Active-flow lifecycle / runtime docs and flow listing | PRJ-005 FINAL REPORT | `24-hour-hivemind/runs/tier_1_project_and_repository_contract_hardening/PRJ-005/PRJ-005-FINAL-REPORT.md` |
| Tier 2 final status | Tier 2 FINAL REPORT | `24-hour-hivemind/runs/tier_2_task_and_graph_planning_semantics/tier_2_task_and_graph_planning_semantics-FINAL-REPORT.md` |
| Graph error/hint and immutability DX gaps | TSK-004 / TSK-005 FINAL REPORTS | `24-hour-hivemind/runs/tier_2_task_and_graph_planning_semantics/TSK-004/TSK-004-FINAL-REPORT.md`, `24-hour-hivemind/runs/tier_2_task_and_graph_planning_semantics/TSK-005/TSK-005-FINAL-REPORT.md` |

---

## 3. Problem Statement

- **Symptoms:**
  - `flow status -f json|-f yaml` did not use the standard success envelope.
  - `hivemind flow list` and `hivemind graph list` were missing.
  - `repo_already_attached` lacked a recovery hint.
  - `graph_immutable`, `task_not_in_graph`, and cycle errors lacked actionable operator guidance.
  - Help/docs drifted from implementation in runtime-set behavior and output contract examples.
- **Impact:**
  - Automation and beta scripts had inconsistent parsing behavior.
  - Operator recovery from common failure states was slower and less deterministic.
- **Detection Source:**
  - Tier 0/1/2 beta final reports and individual scenario reports/scripts.

---

## 4. Root Cause Summary

- **Primary Cause:**
  - Mixed legacy and newer CLI output paths: some commands emitted raw payloads while others used the structured envelope helper.
- **Contributing Factors:**
  - Missing list command handlers and missing registry list APIs.
  - Uneven error-message hardening across graph/repo paths.
  - Operational docs were not fully synchronized with current behavior.
- **Why Now:**
  - Tier beta exercises emphasized machine-readability and operator UX under failure paths.

---

## 5. Fix Scope

| Area | Changes |
|------|---------|
| CLI | Added `graph list` and `flow list` commands (+ `--project` filter), improved command help text, and clarified graph immutability warning in help. |
| Core/Registry | Added `list_graphs`/`list_flows`; added hint to `repo_already_attached`; enriched `graph_immutable` with locking flow context and recovery hint; added hints for `task_not_in_graph` and `cycle_detected`. |
| Docs | Updated `docs/design/cli-operational-semantics.md` and `docs/overview/quickstart.md` to reflect runtime/env/output contracts, list commands, checkpoint gating, and corrected JSON flow-status parsing example. |
| Tooling/Tests | Added/updated integration and unit regression tests for output envelope, list filtering, and new error-hint/context expectations. |
| Other | Updated validation scripts under `24-hour-hivemind/runs/...` where parsing assumptions were stale (`SMK-003` output-contract script, `TSK-004` error script). |

---

## 6. Validation & Regression Matrix

| Check | Result | Evidence / Command |
|-------|--------|---------------------|
| cargo fmt --all --check | PASS | `env -u HIVEMIND_DATA_DIR cargo fmt --all --check` |
| cargo clippy --all-targets --all-features -- -D warnings | PASS | `env -u HIVEMIND_DATA_DIR cargo clippy --all-targets --all-features -- -D warnings` |
| cargo test --all-features | PASS | `env -u HIVEMIND_DATA_DIR cargo test --all-features` (155 unit + 16 integration tests) |
| Tier / Shell Scripts | PASS | `SMK-003/tests/output_contract_test.sh`, `PRJ-005/tests/final_validation_test.sh`, `TSK-001/tests/task_crud_test.sh`, `TSK-004/tests/test_cycle_detection.sh`, `TSK-004/tests/test_errors.sh` |
| Manual QA / CLI scenarios | PASS | Manual JSON checks for `graph list`, `flow list`, and `flow status` success envelope on fresh temp data dir |

---

## 7. Observability & Documentation Updates

- Added explicit success-envelope expectations in operational semantics output contract section.
- Added `version` and `HIVEMIND_DATA_DIR` operational documentation.
- Added runtime-set active-flow behavior note (allowed; applies to future ticks).
- Added graph/flow list command semantics and graph immutability help warning.
- Corrected quickstart scripting example to parse `flow status` from `data.state`.

---

## 8. Residual Risk & Follow-ups

| Risk / Debt | Mitigation Plan | Owner | Due |
|-------------|-----------------|-------|-----|
| Some legacy beta scripts still contain stale command syntax assumptions unrelated to fixed runtime behavior | Continue script refresh in `24-hour-hivemind/runs` as a maintenance pass; keep core acceptance tied to current authoritative scripts above | Antonio | Next beta maintenance pass |

---

## 9. Outstanding Actions

- [ ] Changelog updated
- [ ] PR prepared per `ops/process/sprint-execution.md`
- [ ] CI green on PR branch
- [ ] Merge completed

---

## 10. Attachments / Evidence

- Code diff in:
  - `src/main.rs`
  - `src/cli/commands.rs`
  - `src/core/registry.rs`
  - `tests/integration.rs`
  - `docs/design/cli-operational-semantics.md`
  - `docs/overview/quickstart.md`
- Tier artifacts and rerun evidence under:
  - `24-hour-hivemind/runs/tier_0_baseline_setup_and_trust_in_tooling_simple/SMK-003/results/`
  - `24-hour-hivemind/runs/tier_1_project_and_repository_contract_hardening/PRJ-005/results/`
  - `24-hour-hivemind/runs/tier_2_task_and_graph_planning_semantics/TSK-001/results/`

---

_Report generated: 2026-02-14_
