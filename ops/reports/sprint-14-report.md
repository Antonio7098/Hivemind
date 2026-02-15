# Sprint 14 Report: OpenCode Adapter (First Runtime)

## Metadata

| Field | Value |
|-------|-------|
| Sprint Number | 14 |
| Sprint Title | OpenCode Adapter (First Runtime) |
| Branch | `sprint/14-opencode-adapter` |
| PR | _TBD_ |
| Start Date | 2026-02-08 |
| Completion Date | 2026-02-08 |
| Duration | 1 day |
| Author | Antonio |

---

## Objectives

1. Implement a first runtime adapter by wrapping the OpenCode CLI.
2. Persist per-project runtime configuration and invoke the adapter during `flow tick`.
3. Harden the adapter implementation and raise overall test coverage to the protocol target (>= 80% excluding `src/main.rs`).

---

## Deliverables

### Code Changes

| Area | Files Changed | Lines Added | Lines Removed |
|------|---------------|-------------|---------------|
| core/ | `src/core/events.rs`, `src/core/registry.rs`, `src/core/state.rs`, `src/core/worktree.rs` |  |  |
| cli/ | `src/cli/commands.rs`, `src/main.rs` |  |  |
| adapters/ | `src/adapters/opencode.rs`, `src/adapters/runtime.rs` |  |  |
| storage/ | `src/storage/event_store.rs` |  |  |
| tests/ | `tests/integration.rs` |  |  |
| docs/ops | `docs/design/cli-operational-semantics.md`, `ops/ROADMAP.md`, `changelog.json`, `Makefile`, `.gitignore` |  |  |
| **Total** | 16 files | 1454 | 125 |

### New Components

- [x] `OpenCodeAdapter`: runtime adapter implementing `RuntimeAdapter` for OpenCode.
- [x] Per-project runtime configuration model stored as `ProjectRuntimeConfigured` events.

### New Commands

- [x] `hivemind project runtime-set <project> [--adapter <name>] [--binary-path <path>] [--arg <arg>...] [--env KEY=VALUE...] [--timeout-ms <ms>]`: persist per-project runtime configuration.
- [x] `hivemind flow tick <flow-id>`: execute READY tasks via the configured runtime adapter.

### New Events

- [x] `ProjectRuntimeConfigured`: persist runtime adapter settings on a project.
- [x] `RuntimeFilesystemObserved`: persist filesystem change summary observed after runtime execution.

---

## Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | OpenCode can be launched | PASS | Manual test executed `opencode` via configured runtime and observed `runtime_started` + `runtime_exited` events |
| 2 | Output captured | PASS | Observed `runtime_output_chunk` events; adapter unit tests assert stdout/stderr capture |
| 3 | Timeout works | PASS | Unit test `execute_enforces_timeout` in `src/adapters/opencode.rs` |
| 4 | Filesystem changes observed | PASS | Observed `runtime_filesystem_observed` event in manual test and in integration coverage |

**Overall: 4/4 criteria passed**

---

## Test Results

### Unit Tests

```
running 122 tests
...
test result: ok. 122 passed; 0 failed
```

- Tests run: 122
- Tests passed: 122
- Tests failed: 0
- Tests skipped: 0

### Integration Tests

```
running 8 tests
...
test result: ok. 8 passed; 0 failed
```

- Tests run: 8
- Tests passed: 8

### Coverage

```
Line coverage (excluding main.rs): 80.03%
Lines hit: 4892
Lines found: 6113
```

- Line coverage: 80.03% (excluding `src/main.rs`)

---

## Validation Results

| Check | Status | Notes |
|-------|--------|-------|
| `cargo fmt --check` | PASS | `make validate` |
| `cargo clippy -D warnings` | PASS | `make validate` |
| `cargo test --all-features` | PASS | unit + integration tests |
| `cargo doc` | N/A | not part of `make validate` for this sprint |
| Coverage threshold (80%) | PASS | Actual: 80.03% excluding `src/main.rs` |

---

## Manual Testing (Real OpenCode Runtime)

Manual run configured the project runtime to use `opencode` from `PATH` and executed a READY task via `flow tick`.

- Flow ID: `7aca015d-fee2-444e-8c95-3042fd2f34e3`
- Task ID: `c2d46064-81db-4049-9b08-a177989b8211`
- Attempt ID: `4bf76701-1e3a-41c8-9f1d-325bad82dcc0`

Observed runtime lifecycle events in `events stream`:

- `runtime_started`
- `runtime_output_chunk`
- `runtime_filesystem_observed`
- `runtime_exited`

Artifacts captured in: `/tmp/tmp.NDwACF2PLR/events_stream.json`

---

## Principle Compliance

| # | Principle | Compliance | Notes |
|---|-----------|------------|-------|
| 1 | Observability is truth | YES | Runtime execution emits lifecycle events and persists output + filesystem observation |
| 2 | Fail fast, fail loud | YES | Adapter returns structured `RuntimeError` with codes; strict clippy gates |
| 7 | CLI-first | YES | Runtime config and execution exposed via CLI commands |
| 9 | Automated checks mandatory | YES | `make validate` and coverage threshold met |
| 11 | Build incrementally | YES | Added config first, then tick integration, then hardening/tests |
| 15 | No magic | YES | Runtime behavior is event-sourced and inspectable via `events`/`attempt inspect` |

---

## Documentation Updates

| Document | Change |
|----------|--------|
| `docs/design/cli-operational-semantics.md` | Documented new runtime commands and semantics |
| `ops/ROADMAP.md` | Marked Sprint 14 checklist complete |
| `changelog.json` | Added Sprint 14 entry (`v0.1.5`) |

---

## Challenges

1. **Rebase conflicts across core state/registry and CLI attempt inspection**
   - **Resolution**: merged event-based runtime inspection with Sprint 12 diff artifact inspection; added targeted lint fixes.

2. **Clippy hard gates (`too_many_lines`, `unreachable-patterns`, dead-code)**
   - **Resolution**: minimal suppression where justified (`OpenCodeAdapter::execute`), and structural fixes for match arms/unused helpers.

---

## Learnings

1. Coverage evidence is most robust when derived from `lcov.info` and explicitly excluding `src/main.rs`.
2. Runtime event inspection is the most reliable place to surface execution details; CLI `attempt inspect` should remain thin over events/artifacts.

---

## Technical Debt

| Item | Severity | Tracking |
|------|----------|----------|
| `OpenCodeAdapter::execute` remains long (lint suppressed) | Medium | N/A |

---

## Dependencies

### This Sprint Depended On

| Sprint | Status |
|-------|--------|
| Sprint 13 | Complete |
| Sprint 12 | Complete |

### Next Sprints Depend On This

| Sprint | Readiness |
|-------|----------|
| Sprint 15 | Ready |

---

## Metrics Summary

| Metric | Value |
|--------|-------|
| Total commits | 6 |
| Files changed | 16 |
| Lines added | 1454 |
| Lines removed | 125 |
| Tests added | Adapter unit tests + integration coverage improvements |
| Test coverage | 80.03% lines excluding `src/main.rs` |
| Duration (days) | 1 |

---

## Sign-off

- [x] All exit criteria pass
- [x] All validation checks pass
- [x] Documentation updated
- [x] Changelog updated
- [x] Ready for next sprint

**Sprint 14 is COMPLETE.**

---

_Report generated: 2026-02-08_
