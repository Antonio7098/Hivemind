# Phase 23 Report: Runtime Event Projection (OpenCode)

## Metadata

| Field | Value |
|-------|-------|
| Phase Number | 23 |
| Phase Title | Runtime Event Projection (OpenCode) |
| Branch | `phase/23-runtime-event-projector` |
| PR | (pending) |
| Start Date | 2026-02-13 |
| Completion Date | 2026-02-13 |
| Duration | < 1 day |
| Author | Antonio |

---

## Objectives

1. Project deterministic runtime stdout/stderr markers into observational events.
2. Preserve existing runtime lifecycle/output semantics and keep projection telemetry-only.
3. Validate behavior with unit tests, integration tests, and manual runtime testing (including real OpenCode runtime execution).

---

## Deliverables

### Code Changes

| Area | Files Changed | Lines Added | Lines Removed |
|------|---------------|-------------|---------------|
| core/ | `src/core/runtime_event_projection.rs`, `src/core/events.rs`, `src/core/registry.rs`, `src/core/state.rs`, `src/core/mod.rs` | (see `git diff --numstat`) | (see `git diff --numstat`) |
| cli/ | `src/main.rs`, `src/server.rs` | (see `git diff --numstat`) | (see `git diff --numstat`) |
| tests/ | `tests/integration.rs`, `hivemind-test/test_runtime_projection.sh`, `hivemind-test/test_runtime_projection_real_opencode.sh` | (see `git diff --numstat`) | (see `git diff --numstat`) |
| docs/ | `ops/ROADMAP.md`, `ops/reports/phase-23-report.md`, `docs/architecture/*.md`, `docs/design/*.md`, `changelog.json` | (see `git diff --numstat`) | (see `git diff --numstat`) |
| release/ | `Cargo.toml`, `Cargo.lock` | (see `git diff --numstat`) | (see `git diff --numstat`) |
| **Total** | 22 files | 1094 | 57 |

### New Components

- [x] `RuntimeEventProjector` (`src/core/runtime_event_projection.rs`): buffered stdout/stderr parser that emits best-effort runtime observations.

### New Commands

- [x] Manual validation script: `hivemind-test/test_runtime_projection.sh`
- [x] Real runtime validation script: `hivemind-test/test_runtime_projection_real_opencode.sh`

### New Events

- [x] `RuntimeCommandObserved`: command-like line detected from runtime output.
- [x] `RuntimeToolCallObserved`: tool invocation-like line detected from runtime output.
- [x] `RuntimeTodoSnapshotUpdated`: checklist/todo line detected from runtime output.
- [x] `RuntimeNarrativeOutputObserved`: plain narrative runtime line detected from runtime output.

---

## Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Runtime event projection works for OpenCode in real flow execution | PASS | Executed `hivemind-test/test_runtime_projection_real_opencode.sh`; observed emitted runtime projection events from real `opencode` run |
| 2 | Observational events emitted and queryable via CLI | PASS | `hivemind -f json events stream --flow <flow-id>` includes all four new event types |
| 3 | No TaskFlow control semantics changed by projection logic | PASS | Projection integrated as additive event emission only; `AppState` treats projection payloads as non-state-mutating |
| 4 | Automated and manual tests pass | PASS | Unit + integration tests passed; manual scripts validated projection coverage and lifecycle events |

**Overall: 4/4 criteria passed**

---

## Test Results

### Unit Tests

- PASS (`cargo test --all-features`)
- Includes parser-focused tests in `src/core/runtime_event_projection.rs` (chunk boundaries, stderr-only, noise filtering, newline handling).

### Integration Tests

- PASS (`cargo test --all-features -- --ignored`)
- `tests/integration.rs` extended to assert projection events in end-to-end flow execution.
- PASS (`make validate` summary: 162 tests run, 162 passed, 0 skipped).

### Coverage

- PASS (`cargo llvm-cov --all-features --lcov --output-path lcov.info`)
- PASS (`cargo llvm-cov --all-features --summary-only`)

---

## Validation Results

| Check | Status | Notes |
|-------|--------|-------|
| `cargo fmt --all --check` | PASS | |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS | |
| `cargo test --all-features` | PASS | |
| `cargo test --all-features -- --ignored` | PASS | |
| `cargo test --doc` | PASS | |
| `make validate` | PASS | |

---

## Principle Compliance

| # | Principle | Compliance | Notes |
|---|-----------|------------|-------|
| 1 | Observability is truth | Yes | Projection emits explicit events; no hidden state transitions |
| 2 | Fail fast, fail loud | Yes | Projection failures degrade telemetry only; runtime lifecycle remains explicit |
| 3 | Reliability over cleverness | Yes | Deterministic marker parsing only; no probabilistic inference |
| 4 | Explicit error taxonomy | Yes | No new hidden error channels introduced |
| 5 | Structure scales | Yes | Projector isolated in dedicated core module |
| 6 | SOLID principles | Yes | Parsing logic encapsulated and testable independently |
| 7 | CLI-first | Yes | New event types surfaced in CLI labels and JSON stream |
| 8 | Absolute observability | Yes | Runtime chunk + projected observation coexist for auditability |
| 9 | Automated checks mandatory | Yes | Full validation suite and coverage commands executed |
| 10 | Failures are first-class | Yes | Runtime stderr and noisy output are captured and tested |
| 11 | Build incrementally | Yes | Additive changes over existing runtime event pipeline |
| 12 | Maximum modularity | Yes | `RuntimeEventProjector` is runtime-agnostic and reusable |
| 13 | Abstraction without loss | Yes | Preserves raw output events while adding structured observations |
| 14 | Human authority | Yes | Projection events are non-authoritative and governance-neutral |
| 15 | No magic | Yes | Marker rules are explicit, documented, and deterministic |

---

## Documentation Updates

| Document | Change |
|----------|--------|
| `docs/architecture/hivemind_event_model.md` | Added four runtime observational payload definitions and invariants |
| `docs/architecture/hivemind_runtime_adapters.md` | Added projection behavior and failure semantics |
| `docs/architecture/architecture.md` | Clarified projection as telemetry-only in runtime wrapper architecture |
| `docs/architecture/hivemind_cli_capability_specification.md` | Added CLI support notes for runtime observational events |
| `docs/design/runtime-wrapper-mechanics.md` | Added output projection mechanics and observability notes |
| `docs/design/cli-operational-semantics.md` | Added projected runtime event semantics for `flow tick` |
| `docs/design/event-replay-semantics.md` | Added replay behavior for non-state-mutating projection events |
| `ops/ROADMAP.md` | Added/completed Phase 23 and renumbered subsequent phases |
| `changelog.json` | Added `v0.1.13` entry for Phase 23 |

---

## Challenges

1. **Challenge**: Real OpenCode runtime attempts initially failed due to API key and runtime command-shape issues.
   - **Resolution**: Switched validation to local model `opencode/big-pickle` and passed prompt directly via `opencode run`.

2. **Challenge**: Need to preserve correctness semantics while adding output-derived events.
   - **Resolution**: Kept projector strictly observational and ensured `AppState` replay handling is non-mutating for new payloads.

3. **Challenge**: Parser robustness across fragmented/chunked stdout/stderr.
   - **Resolution**: Implemented buffered line assembly with unit tests covering partial lines and stream-specific behavior.

---

## Learnings

1. Best-effort telemetry can be added safely when raw lifecycle/output events remain authoritative.
2. Deterministic marker parsing gives useful observability without introducing orchestration coupling.
3. Real runtime validation is essential to confirm behavior beyond synthetic shell outputs.

---

## Technical Debt

| Item | Severity | Tracking |
|------|----------|----------|
| Marker heuristics may require evolution as runtimes emit new textual formats | Low | N/A |

---

## Dependencies

### This Phase Depended On

| Phase | Status |
|-------|--------|
| Phase 14 (Runtime configuration + execution) | Complete |
| Phase 15 (Interactive runtime execution path) | Complete |
| Phase 22 (Flow integration hardening) | Complete |

### Next Phases Depend On This

| Phase | Readiness |
|-------|-----------|
| Phase 24 (Single-repo end-to-end) | Ready |
| Phase 28 (Event streaming & observability) | Ready |

---

## Metrics Summary

| Metric | Value |
|--------|-------|
| Total commits | (pending) |
| Files changed | 22 |
| Lines added | 1094 |
| Lines removed | 57 |
| Tests added | Unit tests in projector + integration assertions + 2 manual scripts |
| Test coverage | Executed via `cargo llvm-cov` |
| Duration (days) | < 1 |

---

## Sign-off

- [x] All exit criteria pass
- [x] All validation checks pass
- [x] Documentation updated
- [x] Changelog updated
- [x] Ready for next phase

**Phase 23 is COMPLETE.**

---

_Report generated: 2026-02-13_
