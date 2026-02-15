# Beta Testing Fix Report Template

> Use this template for fixes discovered during beta testing that require rapid stabilization outside the formal phase cadence.

---

## 1. Metadata

| Field | Value |
|-------|-------|
| Fix Title | |
| Date | YYYY-MM-DD |
| Fix Window | e.g. 2026-02-09 → 2026-02-10 |
| Branch | `feat/...` |
| Owner(s) | |
| Related Issues / Reports | Tier reports, GitHub issues, etc. |

---

## 2. Source Reports & Phases

| Artifact / Information | Origin Phase or Report | Reference / Link |
|------------------------|------------------------|------------------|
| Overall beta tier status | TIER-0 FINAL REPORT | `24-hour-hivemind/runs/tier_0_setup_smoke_must_pass_before_deeper_testing/TIER-0-FINAL-REPORT.md` |
| Specific scenario (e.g. SMK-006) | SMK-006 FINAL REPORT | `24-hour-hivemind/runs/tier_0_setup_smoke_must_pass_before_deeper_testing/SMK-006/SMK-006-FINAL-REPORT.md` |

_Use this table to cite every prior report, phase, or Tier artifact (Tier-0/1/2 FINAL reports, SMK-XXX reports, phase reports) that informed the fix. Add as many rows as needed and keep paths exact so they are navigable from the repo._

---

## 3. Problem Statement

- **Symptoms:** What failures surfaced during beta testing?
- **Impact:** Scope of affected commands/users.
- **Detection Source:** Tier script, manual QA, customer beta, etc.

---

## 4. Root Cause Summary

- **Primary Cause:**
- **Contributing Factors:**
- **Why Now:**

---

## 5. Fix Scope

| Area | Changes |
|------|---------|
| CLI | |
| Core/Registry | |
| Docs | |
| Tooling/Tests | |
| Other | |

---

## 6. Validation & Regression Matrix

| Check | Result | Evidence / Command |
|-------|--------|---------------------|
| cargo fmt --all --check | PASS/FAIL | |
| cargo clippy --all-targets --all-features -- -D warnings | PASS/FAIL | |
| cargo test --all-features | PASS/FAIL | |
| Tier / Shell Scripts | PASS/FAIL | List scripts + links |
| Manual QA / CLI scenarios | PASS/FAIL | Notes |

_Add extra rows for any additional validation (coverage, doc tests, UI checks, etc.)._

---

## 7. Observability & Documentation Updates

- Bullet list any doc changes, dashboards, logging tweaks, or instructions added to keep testers aligned.

---

## 8. Residual Risk & Follow-ups

| Risk / Debt | Mitigation Plan | Owner | Due |
|-------------|-----------------|-------|-----|

---

## 9. Outstanding Actions

- [ ] Changelog updated
- [ ] PR prepared per `ops/process/phase-execution.md`
- [ ] Additional TODOs

---

## 10. Attachments / Evidence

- Links to diffs, logs, Tier artifacts, screenshots, etc.

---

_Report generated: YYYY-MM-DD_
