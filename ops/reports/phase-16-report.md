# Phase 16 Report: Scope Enforcement (Phase 1: Detection)

## Metadata

| Field | Value |
|-------|-------|
| Phase Number | 16 |
| Phase Title | Scope Enforcement (Phase 1: Detection) |
| Branch | `phase/16-scope-enforcement` |
| PR | N/A |
| Start Date | 2026-02-09 |
| Completion Date | 2026-02-09 |
| Duration | 0 days |
| Author | Antonio |

---

## Objectives

1. Detect scope violations post-execution (filesystem + git).
2. Make violations fatal and observable, preserving evidence for debugging.
3. Surface clear, actionable error messages in the CLI.

---

## Deliverables

### Code Changes

| Area | Files Changed | Lines Added | Lines Removed |
|------|---------------|-------------|---------------|
| core/ | `src/core/diff.rs`, `src/core/events.rs`, `src/core/registry.rs`, `src/core/state.rs` | N/A | N/A |
| cli/ | `src/main.rs` | N/A | N/A |
| server/ | `src/server.rs` | N/A | N/A |
| tests/ | `tests/integration.rs` | N/A | N/A |
| **Total** | | N/A | N/A |

### New Components

- N/A

### New Commands

- N/A

### New Events

- `ScopeValidated`: Emitted after post-execution verification passes, attached to `flow_id`, `task_id`, `attempt_id`, and includes the effective `scope`.
- `ScopeViolationDetected`: Emitted after post-execution verification fails, includes the effective `scope` and full structured violation details.

---

## Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Violations detected reliably | PASS | Added integration test exercising forbidden filesystem write and asserting `ScopeViolationDetected` appears in event stream |
| 2 | Violations are fatal to attempt | PASS | `process_verifying_task` returns a non-recoverable `HivemindError::scope("scope_violation", ...)` and transitions attempt to `Failed` with no retry |
| 3 | Violations emit observable events | PASS | `ScopeViolationDetected` / `ScopeValidated` added to event model and emitted in registry verification |
| 4 | Honest: prevention is Phase 2+ | PASS | Enforcement remains post-hoc verification; no runtime interception added |

---

## Test Results

### Unit Tests

- `cargo test --all-features`: PASS

### Integration Tests

- `cargo test --all-features`: PASS (includes `cli_scope_violation_is_fatal_and_preserves_worktree`)

### Lints / Formatting

- `cargo fmt --all --check`: PASS
- `cargo clippy --all-targets --all-features -- -D warnings`: PASS

---

## Validation Results

- Verified end-to-end that a scope violation:
  - Emits `ScopeViolationDetected`
  - Transitions the task to `Failed`
  - Preserves the worktree, and surfaces its path in the error hint

---

## Principle Compliance Notes

- **Observability Is Truth**: Scope outcomes are emitted as first-class events with structured violations.
- **Fail Fast, Fail Loud, Fail Early**: Violations fail the attempt immediately and surface as an explicit scope-category error.
- **Failures Are First-Class Outcomes**: Worktree is preserved to retain evidence for debugging.
- **No Magic**: Git-scope checks explicitly account for Hivemind checkpoint commits to avoid false positives.

---

## Challenges / Learnings

- Git scope detection must distinguish between task-authored commits and Hivemind-authored checkpoint commits.

---

## Technical Debt / Followups

- Extend git operation detection beyond commits/branches (e.g., remote pushes) when the runtime and merge protocol support it.
- Consider richer reporting of allowed/denied path rules in violation events (design doc suggests surfacing those lists).

---

## Metrics Summary

- N/A
