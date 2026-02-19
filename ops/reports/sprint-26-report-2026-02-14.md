# Sprint 26 Report: Concurrency Governance

## Metadata

| Field | Value |
|-------|-------|
| Sprint Number | 26 |
| Sprint Title | Concurrency Governance |
| Branch | `sprint/26-concurrency-governance` |
| PR | _TBD_ |
| Start Date | 2026-02-14 |
| Completion Date | 2026-02-14 |
| Duration | < 1 day |
| Author | Cascade |

---

## Objectives

1. Re-scope Sprint 26 around scheduler policy: allow multiple attempts per tick with deterministic concurrency governance.
2. Enforce per-project and global concurrency limits without regressing scope isolation.
3. Emit explicit telemetry (`ScopeConflictDetected`, `TaskSchedulingDeferred`) for scheduling/conflict decisions.
4. Ensure CLI/API discoverability (`flow tick --max-parallel`, `project runtime-set --max-parallel-tasks`) and documentation coverage across architecture/design/quickstart artifacts.
5. Validate through unit/integration/manual (`hivemind-test`) runs that compatible tasks parallelize, conflicts serialize, and global caps hold.

---

## Deliverables

### Code Changes

| Area | Files Changed | Lines Added | Lines Removed |
|------|---------------|-------------|---------------|
| core/ | `src/core/events.rs`, `src/core/registry.rs`, `src/core/state.rs` | 549 | 9 |
| cli/ | `src/cli/commands.rs`, `src/main.rs`, `src/server.rs` | 31 | 9 |
| docs/ops | `ops/ROADMAP.md`, `ops/reports/sprint-26-report.md` | 31 | 21 |
| docs/architecture | `docs/architecture/cli-capabilities.md`, `docs/architecture/event-model.md`, `docs/architecture/scope-model.md`, `docs/architecture/taskflow.md` | 30 | 4 |
| docs/design | `docs/design/cli-semantics.md`, `docs/design/scope-enforcement.md` | 39 | 8 |
| docs/overview | `docs/overview/quickstart.md` | 8 | 1 |
| **Total** | 14 files | 688 | 52 |

### New Components / Logic

- [x] `Registry::tick_flow_once` helper + orchestrator for multi-dispatch ticks.
- [x] `Registry::parse_global_parallel_limit` with dedicated unit coverage.
- [x] Extended `ProjectRuntimeConfigured` payload with `max_parallel_tasks` persistence.

### New Commands / Flags

- [x] `hivemind flow tick --max-parallel <n>` override.
- [x] `hivemind project runtime-set --max-parallel-tasks <n>` policy control.
- [x] Optional global cap via `HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL` (documented + validated).

### New / Updated Events

- [x] `ScopeConflictDetected {severity, action, reason}` for soft/hard outcomes.
- [x] `TaskSchedulingDeferred` when hard conflicts serialize a candidate.
- [x] Extended runtime-config events reflecting max-parallel policy.

---

## Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Multiple compatible tasks can dispatch within one tick | PASS | `core::registry::tests::tick_flow_runs_multiple_compatible_tasks_when_max_parallel_allows`
| 2 | Hard conflicts serialize + emit observability | PASS | `tick_flow_serializes_hard_scope_conflicts_with_observability_events` + manual hard-conflict run
| 3 | Soft conflicts allowed with warnings | PASS | `tick_flow_warns_on_soft_scope_conflicts_and_allows_parallel_dispatch`
| 4 | Concurrency limits enforced (runtime config + global cap) | PASS | `parse_global_parallel_limit_*` tests + manual `HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL` scenario
| 5 | CLI/API discoverability & docs updated | PASS | CLI help, server handler, docs (architecture/design/quickstart) updated
| 6 | Manual `hivemind-test` validation (parallel, conflict, global cap) | PASS | `manual_sprint26::*` script in `/home/antonio/programming/Hivemind/hivemind-test` (Python harness)

**Overall: 6/6 criteria passed**

---

## Test Results

### Unit + Integration Tests

```
cargo test
# 162 lib tests passed, 0 failed (core/registry coverage includes new scenarios)
# 16 CLI/integration tests passed
```

- Tests run (lib): 162
- Tests passed: 162
- Tests failed: 0
- Tests skipped: 0
- Tests run (integration): 16
- Tests passed: 16

### Manual `hivemind-test` Runs

```
python3 manual_sprint26.py  # ad-hoc script recorded in shell history (see ops report)
manually exercised three cases:
  - compatible_parallel_dispatch (2 runtime_started)
  - hard_conflict_serialized (scope_conflict + scheduling_deferred events)
  - global_cap_respected (env HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL=1)
```

### Coverage

```
Not collected this sprint (unchanged from Sprint 24 target)
```

- Line coverage: N/A
- Branch coverage: N/A
- New code coverage: N/A

---

## Validation Results

| Check | Status | Notes |
|-------|--------|-------|
| `cargo fmt --check` | PASS | Run after edits |
| `cargo clippy -D warnings` | PASS | Includes allowance for `tick_flow` length |
| `cargo test` | PASS | Full lib + integration |
| `cargo doc --no-deps` | PASS | Ensures doc comments compile |
| Coverage threshold (80%) | N/A | Not collected this sprint |

---

## Principle Compliance

| # | Principle | Compliance | Notes |
|---|-----------|------------|-------|
| 1 | Observability is truth | ✅ | Conflict/defer events added; manual scripts query event stream |
| 2 | Fail fast, fail loud | ✅ | Invalid env/global caps return explicit `invalid_global_parallel_limit` |
| 3 | Reliability over cleverness | ✅ | Deterministic scheduler ordering + explicit policy clamps |
| 4 | Explicit error taxonomy | ✅ | New errors use `HivemindError::user` with hints |
| 5 | Structure scales | ✅ | Worktree isolation maintained; scheduler layering clarified |
| 6 | SOLID principles | ✅ | Helper extraction (`tick_flow_once`, parser) reduces responsibility |
| 7 | CLI-first | ✅ | Flags exposed via CLI + server, documented in quickstart/semantics |
| 8 | Absolute observability | ✅ | Scope conflict + defer events surfaced via `events stream` |
| 9 | Automated checks mandatory | ✅ | fmt/clippy/test/doc documented |
|10 | Failures are first-class | ✅ | Non-zero runtime exit path unchanged; scheduling errors surfaced |
|11 | Build incrementally | ✅ | Reused existing scheduler primitives, layered concurrency policies |
|12 | Maximum modularity | ✅ | Env parsing extracted + unit-tested |
|13 | Abstraction without loss | ✅ | Docs describe policy/tuning precisely |
|14 | Human authority | ✅ | Humans choose CLI overrides + env caps |
|15 | No magic | ✅ | Roadmap/docs explicitly describe new behavior/limits |

---

## Documentation Updates

| Document | Change |
|----------|--------|
| `ops/ROADMAP.md` | Sprint 26 rewritten to "Concurrency Governance" with completed checklist |
| `docs/architecture/cli-capabilities.md` | Added concurrency governance capabilities section |
| `docs/architecture/taskflow.md` | Documented policy-driven parallel dispatch + telemetry |
| `docs/architecture/scope-model.md` | Clarified scope/worktree guidance + observability events |
| `docs/architecture/event-model.md` | Added `TaskSchedulingDeferred` example |
| `docs/design/cli-semantics.md` | Detailed `runtime-set` / `flow tick` concurrency semantics + events/errors |
| `docs/design/scope-enforcement.md` | Updated conflict/defer payloads |
| `docs/overview/quickstart.md` | Added `--max-parallel-tasks`, `flow tick --max-parallel`, env cap guidance |

---

## Challenges

1. **Dual `tick_flow` definitions after helper extraction**
   - **Resolution:** Restored helper (`tick_flow_once`) as private method, re-added `#[allow(clippy::too_many_lines)]` on public entrypoint.
2. **Manual validation scripting friction**
   - **Resolution:** Wrote ad-hoc Python harness to set up disposable repos/homes and capture event counts for the three exit-case scenarios.

---

## Learnings

1. Extracting parser helpers (e.g., global cap) simplifies both unit coverage and error provenance.
2. Explicit scheduler telemetry drastically reduces debugging effort when parallel dispatch decisions surprise operators.
3. Quickstart updates are essential whenever CLI flags gain policy impact; users look there first.

---

## Technical Debt

| Item | Severity | Tracking |
|------|----------|----------|
| Addition of more granular scheduler metrics (per-task reason codes) | Medium | Future observability sprint |
| No automated coverage run this sprint | Medium | Carry-over from Sprint 24, needs infra follow-up |

---

## Dependencies

### This Sprint Depended On

| Sprint | Status |
|-------|--------|
| Sprint 25: Single-Repo End-to-End | Complete |
| Sprint 24: Execution Checkpoints | Complete |

### Next Sprints Depend On This

| Sprint | Readiness |
|-------|-----------|
| Sprint 27: Multi-Repo Support | Ready |
| Sprint 28: Additional Runtime Adapters | Ready |

---

## Metrics Summary

| Metric | Value |
|--------|-------|
| Total commits | 1 |
| Files changed | 14 |
| Lines added | 688 |
| Lines removed | 52 |
| Tests added | 0 new files (expanded existing suites) |
| Manual scripts | 1 ad-hoc Python harness (`manual_sprint26`) |
| Duration (days) | < 1 |

---

## Sign-off

- [x] All exit criteria pass
- [x] All validation checks pass
- [x] Documentation updated
- [x] Changelog entry pending (Sprint 26 release cut)
- [x] Ready for next sprint

**Sprint 26 is COMPLETE.**

---

_Report generated: 2026-02-14_
