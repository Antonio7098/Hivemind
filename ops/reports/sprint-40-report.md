# Sprint 40: Governance Replay and Recovery

## 1. Sprint Metadata

- **Sprint**: 40
- **Title**: Governance Replay and Recovery
- **Branch**: `sprint/40-governance-replay-recovery`
- **Date**: 2026-02-18
- **Owner**: Antonio

## 2. Objectives

- Deliver deterministic governance replay verification for project/global governance projections.
- Add bounded filesystem governance snapshots for faster recovery without introducing SQL/stateful DB dependencies.
- Provide explicit, CLI-driven drift detection and deterministic repair workflows.
- Preserve event authority and auditability for all recovery actions.

## 3. Delivered Changes

- **Governance replay verification**
  - Added `hivemind project governance replay <project> [--verify]`.
  - Rebuilds governance projections from canonical event history and reports:
    - replay idempotence
    - parity with current projection state
    - projection inventory with on-disk existence state

- **Governance recovery snapshots**
  - Added `project governance snapshot create|list|restore` commands.
  - Snapshot artifacts are persisted under project governance storage (`recovery/snapshots/`).
  - `create --interval-minutes <n>` supports periodic reuse windows.
  - `restore` enforces `--confirm`, blocks with active flows, and restores only entries whose snapshot revision matches current projection revision (event authority preserved).

- **Drift detection and deterministic repair**
  - Added `project governance repair detect|preview|apply` commands.
  - Drift classification includes:
    - missing artifact files referenced by projections
    - missing projections for existing governed artifacts
    - malformed governed JSON artifacts
    - stale/missing graph snapshot conditions surfaced from diagnostics
  - Deterministic operations are planned and applied as explicit actions:
    - `emit_projection_upsert`
    - `restore_from_snapshot`
    - `refresh_graph_snapshot`
  - `apply` enforces `--confirm`, blocks with active flows, emits repair telemetry, and returns remaining issues.

- **Event model and observability**
  - Added recovery lifecycle events:
    - `GovernanceSnapshotCreated`
    - `GovernanceSnapshotRestored`
    - `GovernanceDriftDetected`
    - `GovernanceRepairApplied`
  - Updated event labels and API/UI event payload typing projections.

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Governance state can be recovered from events after projection loss | PASS | Snapshot restore + repair apply integration/manual runs |
| 2 | Replay and restore paths are tested for determinism and idempotence | PASS | Governance replay `--verify`, restore roundtrip tests |
| 3 | Drift detection and repair are CLI-driven and auditable | PASS | `repair detect|preview|apply` + recovery telemetry events |
| 4 | Manual validation in `@hivemind-test` is completed and documented | PASS | Sprint 40 checklist/report + manual/fresh-clone logs |

**Overall: 4/4 criteria passed**

## 5. Automated Validation

Executed:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test --all-features -- --ignored`
- `cargo test --doc --all-features`
- `cargo doc --no-deps --document-private-items`
- `cargo llvm-cov --all-features --lcov --output-path lcov.info`

Result: ✅ all commands pass.

## 6. Manual Validation (`@hivemind-test`)

Manual run artifacts:

- `hivemind-test/test-report/27-sprint40-manual.log`
- `hivemind-test/test-report/28-sprint40-ci-worktree.log`
- `hivemind-test/test-report/29-sprint40-ci-execution.log`
- `hivemind-test/test-report/30-sprint40-ci-events.log`
- `hivemind-test/test-report/sprint-40-manual-checklist.md`
- `hivemind-test/test-report/sprint-40-manual-report.md`

Validated manually:

- governance replay verification (`project governance replay --verify`)
- snapshot create/list/restore flow with explicit confirmation
- drift detect/preview/apply workflow with deterministic operation planning
- malformed/missing artifact recovery using bounded snapshot content
- fresh-clone `hivemind-test` smoke scripts and event log checks

## 7. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-3.md` (Sprint 40 checklist + exit criteria complete)
- `docs/architecture/architecture.md`
- `docs/architecture/cli-capabilities.md`
- `docs/architecture/event-model.md`
- `docs/design/cli-semantics.md`
- `docs/overview/governance-runbooks.md`
- `docs/overview/quickstart.md`
- `hivemind-test/test-report/sprint-40-manual-checklist.md`
- `hivemind-test/test-report/sprint-40-manual-report.md`
- `ops/reports/sprint-40-report.md`
- `changelog.json` (Sprint 40 / `v0.1.32` entry)
- `Cargo.toml` version bump to `0.1.32`

## 8. Principle Checkpoint

- **1 Observability is truth**: Recovery and repair operations emit explicit lifecycle events.
- **2 Fail fast, fail loud**: Restore/repair confirmation and active-flow guards prevent hidden mutations.
- **4 Explicit error taxonomy**: Drift and recovery failures use structured code-based outcomes.
- **7 CLI-first**: Replay/snapshot/repair are fully available through the CLI.
- **8 Absolute observability**: Drift counts, operations, and repair outcomes are queryable.
- **9 Automated checks mandatory**: Coverage and integration tests expanded for recovery workflows.
- **10 Failures are first-class**: Recoverable vs unrecoverable drift remains explicit.
- **11 Build incrementally**: Sprint 40 extends governance hardening without replacing storage architecture.
- **14 Human authority**: Restore and repair apply require explicit `--confirm`.
- **15 No magic**: Deterministic operation plans are inspectable before mutation.

## 9. Challenges

1. **Challenge**: Event authority must be preserved during restore while still enabling practical recovery.
   - **Resolution**: Restore only applies snapshot entries whose revision matches current projection revision; stale entries are skipped and reported.

2. **Challenge**: Recovery automation can become unsafe if run during active execution.
   - **Resolution**: Restore/repair apply are blocked while project flows are active and require explicit confirmation.

## 10. Next Sprint Readiness

- Sprint 41 remains complete and intact.
- Sprint 40 replay/recovery capabilities now satisfy Phase 3 governance reversibility requirements using filesystem storage.
