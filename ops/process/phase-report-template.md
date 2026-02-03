# Phase Report Template

> Copy this template to `ops/reports/phase-<N>-report.md` and fill in all sections.

---

# Phase [N] Report: [Title]

## Metadata

| Field | Value |
|-------|-------|
| Phase Number | |
| Phase Title | |
| Branch | `phase/<N>-<name>` |
| PR | #[number] |
| Start Date | YYYY-MM-DD |
| Completion Date | YYYY-MM-DD |
| Duration | X days |
| Author | |

---

## Objectives

_What this phase aimed to achieve, from ops/ROADMAP.md._

1.
2.
3.

---

## Deliverables

_What was actually delivered._

### Code Changes

| Area | Files Changed | Lines Added | Lines Removed |
|------|---------------|-------------|---------------|
| core/ | | | |
| cli/ | | | |
| adapters/ | | | |
| storage/ | | | |
| tests/ | | | |
| **Total** | | | |

### New Components

- [ ] Component 1: _description_
- [ ] Component 2: _description_

### New Commands

- [ ] `hivemind <command>`: _description_

### New Events

- [ ] `EventName`: _description_

---

## Exit Criteria Results

_Copy exit criteria from ops/ROADMAP.md and mark pass/fail._

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | | PASS/FAIL | How verified |
| 2 | | PASS/FAIL | How verified |
| 3 | | PASS/FAIL | How verified |

**Overall: [N]/[M] criteria passed**

---

## Test Results

### Unit Tests

```
[Paste cargo test output summary]
```

- Tests run:
- Tests passed:
- Tests failed:
- Tests skipped:

### Integration Tests

```
[Paste integration test output summary]
```

- Tests run:
- Tests passed:

### Coverage

```
[Paste coverage summary]
```

- Line coverage: X%
- Branch coverage: X%
- New code coverage: X%

---

## Validation Results

| Check | Status | Notes |
|-------|--------|-------|
| `cargo fmt --check` | PASS/FAIL | |
| `cargo clippy -D warnings` | PASS/FAIL | |
| `cargo test` | PASS/FAIL | |
| `cargo doc` | PASS/FAIL | |
| Coverage threshold (80%) | PASS/FAIL | Actual: X% |

---

## Principle Compliance

_For each principle, note how it was upheld or any decisions made._

| # | Principle | Compliance | Notes |
|---|-----------|------------|-------|
| 1 | Observability is truth | | |
| 2 | Fail fast, fail loud | | |
| 3 | Reliability over cleverness | | |
| 4 | Explicit error taxonomy | | |
| 5 | Structure scales | | |
| 6 | SOLID principles | | |
| 7 | CLI-first | | |
| 8 | Absolute observability | | |
| 9 | Automated checks mandatory | | |
| 10 | Failures are first-class | | |
| 11 | Build incrementally | | |
| 12 | Maximum modularity | | |
| 13 | Abstraction without loss | | |
| 14 | Human authority | | |
| 15 | No magic | | |

---

## Documentation Updates

| Document | Change |
|----------|--------|
| `docs/architecture/*.md` | |
| `docs/design/*.md` | |
| `README.md` | |
| Code doc comments | |
| `changelog.json` | Entry added |

---

## Challenges

_What was difficult or unexpected._

1. **Challenge**: _description_
   - **Resolution**: _how it was resolved_

2. **Challenge**: _description_
   - **Resolution**: _how it was resolved_

---

## Learnings

_What was learned that might inform future phases._

1.
2.
3.

---

## Technical Debt

_Any shortcuts taken or debt incurred._

| Item | Severity | Tracking |
|------|----------|----------|
| | Low/Medium/High | Issue #X or N/A |

---

## Dependencies

### This Phase Depended On

| Phase | Status |
|-------|--------|
| Phase [X] | Complete |

### Next Phases Depend On This

| Phase | Readiness |
|-------|-----------|
| Phase [Y] | Ready/Blocked |

---

## Metrics Summary

| Metric | Value |
|--------|-------|
| Total commits | |
| Files changed | |
| Lines added | |
| Lines removed | |
| Tests added | |
| Test coverage | |
| Duration (days) | |

---

## Sign-off

- [ ] All exit criteria pass
- [ ] All validation checks pass
- [ ] Documentation updated
- [ ] Changelog updated
- [ ] Ready for next phase

**Phase [N] is COMPLETE.**

---

_Report generated: YYYY-MM-DD_
