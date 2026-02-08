# Phase 12 Report: Baseline & Diff Computation

## Metadata

| Field | Value |
|-------|-------|
| Phase Number | 12 |
| Phase Title | Baseline & Diff Computation |
| Branch | `phase/12-baseline-diff-computation` |
| PR | #TBD |
| Start Date | 2026-02-08 |
| Completion Date | 2026-02-08 |
| Duration | 0 days |
| Author | Antonio |

---

## Objectives

1. Capture a per-attempt baseline snapshot (file list + hashes + git HEAD) prior to execution.
2. Compute created/modified/deleted file changes and unified diffs on attempt completion.
3. Persist baseline/diff artifacts replay-safely and emit events for observability and attribution.

---

## Deliverables

### Code Changes

| Area | Files Changed | Lines Added | Lines Removed |
|------|---------------|-------------|---------------|
| core/ | `src/core/events.rs`, `src/core/state.rs`, `src/core/registry.rs` | 692 | 3 |
| cli/ | `src/cli/commands.rs`, `src/main.rs` | 340 | 142 |
| tests/ | `tests/integration.rs` | 66 | 0 |
| docs/ | `docs/design/cli-operational-semantics.md`, `ops/ROADMAP.md`, `changelog.json`, `ops/reports/phase-12-report.md` | 360 | 18 |
| other | `Cargo.toml`, `Cargo.lock` | 2 | 2 |
| **Total** | 12 files | 1460 | 165 |

### New Components

- `AttemptState` (replay-derived): tracks attempt attribution including baseline/diff IDs.
- Baseline artifacts: `.hivemind/artifacts/baselines/<baseline-id>/baseline.json` + baseline file copies under `files/`.
- Diff artifacts: `.hivemind/artifacts/diffs/<diff-id>.json` containing diff metadata plus unified diff text.

### New Commands

- `hivemind task start <task-id>`: captures baseline and starts a new attempt.
- `hivemind task complete <task-id>`: computes diff, persists diff artifact, and transitions to VERIFYING.
- `hivemind attempt inspect <attempt-id> --diff`: displays stored unified diff.

### New Events

- `AttemptStarted`: attempt creation and numbering.
- `BaselineCaptured`: baseline artifact reference and git head.
- `FileModified`: per-file change classification and hashes.
- `DiffComputed`: diff artifact reference and change count.
- `CheckpointCommitCreated`: best-effort checkpoint commit SHA when dirty state exists at completion.

---

## Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Diffs computed accurately | PASS | Integration test `cli_attempt_inspect_diff_after_manual_execution` asserts unified diff contains expected `-test`/`+changed` lines |
| 2 | Diffs attributed to task/attempt | PASS | State replay stores `baseline_id`/`diff_id` on `AttemptState`; CLI `attempt inspect` returns per-attempt artifact |
| 3 | Changes observable via events | PASS | `events stream --task <task-id>` shows `attempt_started`, `baseline_captured`, `file_modified`, `diff_computed`, and (when applicable) `checkpoint_commit_created` |

**Overall: 3/3 criteria passed**

---

## Test Results

### Unit + Integration Tests

```
make validate
(nextest) Summary: 130 tests run: 130 passed, 0 skipped
```

- Tests run: 130
- Tests passed: 130
- Tests failed: 0
- Tests skipped: 0

### Manual CLI Testing

- Performed an end-to-end CLI session in a fresh temp `HOME` + real git repo.
- Verified:
  - `task start` emits baseline events
  - `task complete` emits file/diff events
  - `attempt inspect --diff` prints unified diff
  - `events stream --task` shows `attempt_started`, `baseline_captured`, `file_modified`, `diff_computed`, `checkpoint_commit_created`

### Coverage

Not measured as part of `make validate`.

---

## Validation Results

| Check | Status | Notes |
|-------|--------|-------|
| `cargo fmt --check` | PASS | via `make validate` |
| `cargo clippy -D warnings` | PASS | via `make validate` |
| `cargo test` / `cargo nextest` | PASS | via `make validate` |
| `cargo doc` | PASS | via `make validate` |

---

## Principle Compliance

| # | Principle | Compliance | Notes |
|---|-----------|------------|-------|
| 1 | Observability is truth | PASS | Baselines/diffs are persisted as artifacts and referenced by events; CLI exposes diffs via `attempt inspect --diff` |
| 2 | Fail fast, fail loud | PASS | Baseline/diff failures are structured errors with explicit codes |
| 3 | Reliability over cleverness | PASS | Uses simple filesystem snapshotting + unified diff; no hidden state |
| 4 | Explicit error taxonomy | PASS | Errors use `HivemindError` codes and origins |
| 5 | Structure scales | PASS | Attempt artifacts and events are per-attempt and replay-safe |
| 6 | SOLID principles | PASS | Artifact logic centralized in registry; state replay isolated in AppState |
| 7 | CLI-first | PASS | Lifecycle boundaries are explicit commands (`task start`, `task complete`, `attempt inspect --diff`) |
| 8 | Absolute observability | PASS | Event stream captures what happened; artifacts preserve the actual diff content |
| 9 | Automated checks mandatory | PASS | `make validate` is green |
| 10 | Failures are first-class | PASS | Attempt/diff artifacts are preserved regardless of later verification outcome |
| 11 | Build incrementally | PASS | Manual boundaries introduced ahead of runtime adapters, matching phase sequencing |
| 12 | Maximum modularity | PASS | Artifacts are external; events remain minimal references |
| 13 | Abstraction without loss | PASS | Unified diff preserved verbatim for human inspection |
| 14 | Human authority | PASS | Diff inspection is read-only; verification overrides remain explicit |
| 15 | No magic | PASS | Artifact locations and event references are explicit and inspectable |

---

## Documentation Updates

| Document | Change |
|----------|--------|
| `ops/ROADMAP.md` | Mark Phase 12 items complete |
| `docs/design/cli-operational-semantics.md` | Add semantics for `task start`/`task complete` and `attempt inspect --diff` |
| `changelog.json` | Add v0.1.3 entry for Phase 12 |
| `ops/reports/phase-12-report.md` | Added this report |

---

## Challenges

1. **Challenge**: Maintaining replay safety while persisting large diffs.
   - **Resolution**: Store artifacts externally and reference them via events; state replay stores only IDs.

2. **Challenge**: Selecting the correct active attempt when multiple attempts exist.
   - **Resolution**: Resolve latest attempt for the task/flow lacking a `diff_id` (and use timestamps for tie-breaking).

---

## Learnings

1. Unified diffs are easiest to preserve and inspect when stored as an artifact rather than embedded in events.
2. Per-attempt attribution benefits from first-class attempt state in replay (`AttemptState`).

---

## Technical Debt

| Item | Severity | Tracking |
|------|----------|----------|
| CLI command naming/numbering for task execution boundaries vs future runtime adapter execution | Medium | N/A |
| Coverage reporting is not included in `make validate` | Low | N/A |

---

## Dependencies

### This Phase Depended On

| Phase | Status |
|-------|--------|
| Phase 11 | Complete |

### Next Phases Depend On This

| Phase | Readiness |
|-------|-----------|
| Phase 13 | Ready |

---

## Metrics Summary

| Metric | Value |
|--------|-------|
| Total commits | TBD |
| Files changed | 12 |
| Lines added | 1460 |
| Lines removed | 165 |
| Tests added | 1 integration test |
| Duration (days) | 0 |

---

## Sign-off

- [x] All exit criteria pass
- [x] All validation checks pass
- [x] Documentation updated
- [x] Changelog updated
- [x] Ready for next phase

**Phase 12 is COMPLETE.**

---

_Report generated: 2026-02-08_
