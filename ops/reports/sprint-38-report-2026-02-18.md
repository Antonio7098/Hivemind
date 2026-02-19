# Sprint 38: Constitution Enforcement

## 1. Sprint Metadata

- **Sprint**: 38
- **Title**: Constitution Enforcement
- **Branch**: `sprint/38-constitution-enforcement`
- **Date**: 2026-02-18
- **Owner**: Antonio

## 2. Objectives

- Enforce constitution rules against deterministic graph snapshots at execution/merge boundaries.
- Introduce explicit CLI command for manual constitution enforcement checks.
- Emit structured constitution violation events with rule/gate/evidence context.
- Preserve non-constitution project compatibility while enforcing hard-rule gates for constitution-enabled projects.

## 3. Delivered Changes

- **Constitution enforcement engine**
  - Implemented deterministic `validate(graph_snapshot, constitution)` evaluation for:
    - `forbidden_dependency`
    - `allowed_dependency`
    - `coverage_requirement`
  - Added severity-aware outcomes (`hard`, `advisory`, `informational`) with blocked/non-blocked behavior.

- **Execution + merge gates**
  - Enforced constitution checks at:
    - `checkpoint_complete`
    - `merge_prepare`
    - `merge_approve`
    - `merge_execute`
    - PR-mode merge execution path
  - Added policy failure path `constitution_hard_violation` with remediation hint.

- **CLI surface**
  - Added `hivemind constitution check --project <project>` for explicit manual validation output.

- **Event model + observability**
  - Added `ConstitutionViolationDetected` event payload with:
    - project/flow/task/attempt correlation
    - gate, rule ID/type, severity
    - message, evidence, remediation, blocked flag
  - Wired event projections/labels through CLI/server/replay surfaces.

- **Tests**
  - Added unit and integration coverage for:
    - severity-aware violation reporting
    - checkpoint hard-gate blocking
    - merge prepare/approve/execute hard-gate blocking
    - explicit CLI constitution check output semantics

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Hard violations block progression deterministically | PASS | Unit tests + manual checkpoint/merge gate logs (`15-sprint38-manual.log`) |
| 2 | Advisory/informational violations remain visible and queryable | PASS | `constitution check` output includes severity counters + violation evidence |
| 3 | Enforcement behavior is deterministic under replay | PASS | Enforcement decisions derived from graph snapshot + constitution artifact; event trail is deterministic and replay-compatible |
| 4 | Manual validation in `@hivemind-test` completed/documented | PASS | Sprint 38 checklist/report and fresh-clone smoke logs committed |

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

Result: ✅ all validation commands above pass.

Coverage note:

- `lcov.info` generated successfully.
- Line coverage (from LCOV `DA` entries): **69.33%**.
- Repository-wide 80% threshold remains unmet in current baseline; Sprint 38 includes targeted regression tests + manual gate verification artifacts.

## 6. Manual Validation (`@hivemind-test`)

Manual run artifacts:

- `hivemind-test/test-report/15-sprint38-manual.log`
- `hivemind-test/test-report/16-sprint38-ci-worktree.log`
- `hivemind-test/test-report/17-sprint38-ci-execution.log`
- `hivemind-test/test-report/18-sprint38-ci-events.log`
- `hivemind-test/test-report/sprint-38-manual-checklist.md`
- `hivemind-test/test-report/sprint-38-manual-report.md`

Validated manually:

- hard/advisory/informational constitution outcomes from explicit `constitution check`
- checkpoint boundary hard-rule enforcement
- merge boundary hard-rule enforcement (`prepare/approve/execute`)
- `constitution_violation_detected` event observability for each enforced gate
- fresh-clone `test_worktree.sh` and `test_execution.sh` regression smoke + event inspection

## 7. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-3.md` (Sprint 38 checklist + exit criteria marked complete)
- `docs/design/cli-semantics.md`
- `docs/architecture/cli-capabilities.md`
- `docs/architecture/event-model.md`
- `docs/overview/quickstart.md`
- `hivemind-test/test-report/sprint-38-manual-checklist.md`
- `hivemind-test/test-report/sprint-38-manual-report.md`
- `ops/reports/sprint-38-report.md`
- `changelog.json` (Sprint 38 / `v0.1.29` entry)
- `Cargo.toml` version bump to `0.1.29`

## 8. Principle Checkpoint

- **2 Fail fast, fail loud**: hard constitution violations block immediately at checkpoint/merge gates with explicit policy errors.
- **4 Explicit error taxonomy**: enforcement failures use concrete policy code `constitution_hard_violation` with actionable hints.
- **10 Failures are first-class**: violations persist as explicit events for review/retry/audit.
- **7 CLI-first**: explicit manual command `constitution check --project` enables deterministic operator/agent workflows.
- **8 Absolute observability**: gate-level violation events include correlation IDs, rule IDs, evidence, and remediation hints.
- **11 Build incrementally**: enforcement layer added on top of Sprint 36 constitution + Sprint 37 snapshot foundations.
- **14 Human authority**: merge boundaries remain explicit and now require constitution policy compliance.
- **15 No magic**: every block has an attributable rule ID, gate, and event trail.

## 9. Challenges

1. **Challenge**: strict clippy policy (`-D warnings`) surfaced precision/style/test-length lints in new enforcement/test paths.
   - **Resolution**: refactored helper signatures, removed lossy float casts, normalized formatting/signatures, and applied scoped test lint allowances.

2. **Challenge**: manual validation had mixed JSON output envelope shapes across commands.
   - **Resolution**: hardened manual scripts/parsing strategy and captured stable artifacts for reproducibility.

## 10. Next Sprint Readiness

- Sprint 39 (Deterministic Agent Context Assembly) can now rely on enforced constitution policy gates and explicit violation telemetry at critical execution/merge boundaries.
