# Beta Testing Fix Report Template

> Use this template for fixes discovered during beta testing that require rapid stabilization outside the formal sprint cadence.

---

## 1. Metadata

| Field | Value |
|-------|-------|
| Fix Title | |
| Date | YYYY-MM-DD |
| Fix Window | e.g. 2026-02-16 18:00 → 20:45 UTC |
| Branch | `fix/...` |
| Owner(s) | |
| Related Issues / Reports | Tier reports, sprint reports, GitHub issues |

---

## 2. Source Reports & Inputs

| Artifact / Information | Origin Sprint/Report | Reference / Link |
|------------------------|----------------------|------------------|
| Tier aggregate status | Tier FINAL report | `24-hour-hivemind/runs/...-FINAL-REPORT.md` |
| Scenario-level evidence | Scenario FINAL report | `24-hour-hivemind/runs/.../<SCENARIO>-FINAL-REPORT.md` |
| Existing fix notes (if any) | Prior beta fix report | `ops/reports/beta-testing-fix-*.md` |

_Add every upstream report that informed this fix. Keep paths exact and repo-relative._

---

## 3. Problem Statement

- **Symptoms:** What failed during beta testing?
- **Impact:** Which commands/flows/users were blocked or degraded?
- **Detection Source:** Tier harness, manual QA, dogfooding, customer beta, etc.

---

## 4. Root Cause Summary

- **Primary Cause:**
- **Contributing Factors:**
- **Why Not Detected Earlier:**

---

## 5. Fix Scope

| Area | Changes |
|------|---------|
| CLI | |
| Core/Registry | |
| Adapters/Runtime | |
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
| Targeted regression tests | PASS/FAIL | list test names |
| Manual QA scenarios | PASS/FAIL | list commands + outcomes |

_Add rows for extra checks (coverage, perf, UI snapshots, replay verification, etc.) as needed._

---

## 7. DX Issues Identified (Optional)

| Issue | Severity | Description | Follow-up |
|-------|----------|-------------|-----------|
| | | | |

_Use this section when the fix run surfaces adjacent DX debt not addressed in this patch._

---

## 8. Observability & Documentation Updates

- Event/schema changes:
- Docs updated:
- Operational runbook/process updates:

---

## 9. Residual Risk & Follow-ups

| Risk / Debt | Mitigation Plan | Owner | Due |
|-------------|-----------------|-------|-----|
| | | | |

---

## 10. Outstanding Actions

- [ ] Changelog updated (`changelog.json`)
- [ ] PR opened and linked
- [ ] CI green
- [ ] Squash merge completed
- [ ] Follow-up tasks filed (if any)

---

## 11. Attachments / Evidence

- PR link:
- CI run link:
- Logs/artifacts:
- Screenshots (if applicable):

---

_Report generated: YYYY-MM-DD_
