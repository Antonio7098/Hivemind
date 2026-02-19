# Sprint 37: UCP Graph Integration and Snapshot Projection

## 1. Sprint Metadata

- **Sprint**: 37
- **Title**: UCP Graph Integration and Snapshot Projection
- **Branch**: `sprint/37-ucp-graph-snapshot-ci`
- **Date**: 2026-02-17
- **Owner**: Antonio

## 2. Objectives

- Integrate UCP codegraph extraction into Hivemind without duplicating parser/extractor logic.
- Materialize project graph snapshot artifacts in Hivemind governance storage with explicit schema/provenance/version metadata.
- Add explicit snapshot refresh command and automatic lifecycle triggers (attach/checkpoint/merge).
- Enforce graph snapshot integrity/staleness gates before constitution lifecycle operations.
- Preserve replay determinism and existing execution behavior while adding snapshot observability.

## 3. Delivered Changes

- **UCP integration and snapshot artifact model**
  - Added pinned `ucp-api` dependency to unified-content-protocol git revision `cf745b5efb66711f5fbcd89a41c640533b2fdc84` (CI-compatible).
  - Implemented graph snapshot build pipeline in registry using UCP APIs:
    - `build_code_graph`
    - `validate_code_graph_profile`
    - `canonical_fingerprint`
    - `codegraph_prompt_projection`
  - Added snapshot envelope model with:
    - schema + snapshot version
    - provenance (project + per-repo commit heads + generated timestamp)
    - UCP engine/profile versions
    - canonical fingerprint
    - aggregate summary stats
    - per-repo portable document + structure/blocks projection
    - static whole-codebase projection string for LLM context.

- **Lifecycle command and auto triggers**
  - Added CLI command: `hivemind graph snapshot refresh <project>`.
  - Added auto-refresh triggers on:
    - `project attach-repo` (`trigger=project_attach`)
    - `checkpoint complete` (`trigger=checkpoint_complete`)
    - `merge execute` completion (`trigger=merge_completed`).

- **Event model and observability**
  - Added events:
    - `GraphSnapshotStarted`
    - `GraphSnapshotCompleted`
    - `GraphSnapshotFailed`
    - `GraphSnapshotDiffDetected`
  - Wired event label rendering in CLI/server surfaces.
  - Kept replay deterministic by treating snapshot lifecycle events as no-op for state derivation.

- **Integrity + staleness gates for constitution**
  - Added pre-constitution guard for projects with attached repos:
    - snapshot exists
    - profile compatibility
    - per-repo fingerprint integrity
    - aggregate fingerprint integrity
    - commit provenance freshness vs current HEAD
  - Added actionable recovery hint: `Run: hivemind graph snapshot refresh <project>`.

- **Tests**
  - Added/updated coverage for:
    - manual refresh artifact/event lifecycle
    - stale snapshot rejection for constitution validation
    - checkpoint trigger snapshot completion
    - merge trigger snapshot completion.

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Hivemind uses UCP graph engine as authoritative extraction backend | PASS | Registry uses `ucp-api` build/validate/fingerprint/projection APIs; no duplicate extractor implementation |
| 2 | Hivemind accepts only profile-compliant UCP graph artifacts | PASS | Profile validation + fingerprint + scope/logical_key checks in `graph_snapshot_refresh` |
| 3 | Snapshot lifecycle triggers and telemetry are explicit/observable | PASS | Snapshot events emitted on attach/checkpoint/merge/manual refresh; validated in unit + manual logs |
| 4 | Constitution engine receives stable, reproducible graph input | PASS | Constitution lifecycle guarded by snapshot staleness/integrity checks |
| 5 | Hivemind-owned versioning/eventing/failure semantics enforced | PASS | Snapshot schema/version metadata + explicit failed/diff events + structured errors |
| 6 | Manual validation in `@hivemind-test` completed/documented | PASS | Sprint 37 checklist/report + execution/worktree logs committed |

**Overall: 6/6 criteria passed**

## 5. Automated Validation

Executed:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test --all-features -- --ignored`
- `cargo test --doc --all-features`
- `cargo doc --no-deps --document-private-items`
- `cargo llvm-cov --all-features --lcov --output-path lcov.info`

Result: ✅ all validation commands above pass.

Coverage note:

- `lcov.info` generated successfully.
- Line coverage (from LCOV `DA` entries): **68.63%** (`14464/21075`).
- 80% threshold is **not met** in the full repository baseline; Sprint 37 changes include targeted regression tests and manual coverage evidence.

## 6. Manual Validation (`@hivemind-test`)

Manual run artifacts:

- `hivemind-test/test-report/08-sprint37-manual.log`
- `hivemind-test/test-report/09-sprint37-worktree.log`
- `hivemind-test/test-report/10-sprint37-execution.log`
- `hivemind-test/test-report/13-sprint37-ci-worktree.log`
- `hivemind-test/test-report/14-sprint37-ci-execution.log`
- `hivemind-test/test-report/sprint-37-manual-checklist.md`
- `hivemind-test/test-report/sprint-37-manual-report.md`

Validated manually:

- attach-triggered snapshot lifecycle (`project_attach`)
- explicit refresh command path (`manual_refresh`)
- checkpoint-triggered snapshot lifecycle (`checkpoint_complete`)
- merge-triggered snapshot lifecycle (`merge_completed`)
- stale snapshot constitution failure + recovery path
- non-constitution project flow remains operational
- baseline `hivemind-test` worktree and execution scripts remain green.

## 7. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-3.md` (Sprint 37 checklist marked complete)
- `docs/design/cli-semantics.md`
- `docs/architecture/cli-capabilities.md`
- `docs/architecture/event-model.md`
- `hivemind-test/test-report/sprint-37-manual-checklist.md`
- `hivemind-test/test-report/sprint-37-manual-report.md`
- `ops/reports/sprint-37-report.md`
- `changelog.json` (`v0.1.28` entry)
- `Cargo.toml` version bump to `0.1.28`

## 8. Principle Checkpoint

- **1 / 8 Observability is truth / absolute observability**: snapshot lifecycle and diff/failure semantics are explicit events.
- **2 / 4 Fail fast + explicit taxonomy**: stale/missing/integrity/profile errors are structured with recovery hints.
- **3 Reliability over cleverness**: deterministic profile/fingerprint/provenance checks gate acceptance.
- **7 CLI-first**: explicit `graph snapshot refresh` command exposed for operators/agents.
- **10 Failures are first-class**: failed refresh paths emit `GraphSnapshotFailed` + `ErrorOccurred` telemetry.
- **11 Incremental foundations**: this sprint delivers static graph projection only; traversal/semantic expansion deferred intentionally.
- **14 Human authority**: constitution mutations remain explicit and now require fresh observable code facts.
- **15 No magic**: snapshot trigger reasons and artifact provenance are explicit and auditable.

## 9. Challenges

1. **Challenge**: Governance scaffold seeded `graph_snapshot.json` with `{}`, which initially failed strict artifact parsing.
   - **Resolution**: Added backward-compatible reader behavior treating empty/placeholder snapshot files as absent.

2. **Challenge**: Host disk exhaustion during release/coverage/manual cycles.
   - **Resolution**: Performed `cargo clean` to reclaim target storage, then rebuilt and completed manual evidence runs.

3. **Challenge**: Local path dependency to `../unified-content-protocol` failed on GitHub Actions runners.
   - **Resolution**: Replaced path dependency with pinned git dependency to UCP revision `cf745b5efb66711f5fbcd89a41c640533b2fdc84`, regenerated lockfile, and revalidated full suite.

## 10. Next Sprint Readiness

- Sprint 38 (Constitution Enforcement) can now rely on deterministic, observable, profile-compliant graph snapshot inputs and explicit staleness/integrity gates.
