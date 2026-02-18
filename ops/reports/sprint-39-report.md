# Sprint 39: Deterministic Agent Context Assembly

## 1. Sprint Metadata

- **Sprint**: 39
- **Title**: Deterministic Agent Context Assembly
- **Branch**: `sprint/39-deterministic-agent-context-assembly`
- **Date**: 2026-02-18
- **Owner**: Antonio

## 2. Objectives

- Freeze reproducible attempt context from explicit governance artifacts.
- Persist immutable attempt context manifests with deterministic hash lineage.
- Expose context provenance for operators via `attempt inspect --context`.
- Ensure retry attempts explicitly reference prior attempt manifests.

## 3. Delivered Changes

- **Deterministic context assembly pipeline**
  - Implemented ordered attempt context assembly inputs:
    - constitution
    - system prompt
    - skills
    - project documents
    - graph summary
  - Excluded non-executional/implicit sources by contract:
    - project notepad
    - global notepad
    - implicit memory
  - Added context size budgeting + truncation policy metadata (`ordered_section_then_total_budget`).

- **Context manifest + event model**
  - Added attempt context event payloads:
    - `AttemptContextOverridesApplied`
    - `AttemptContextAssembled`
    - `AttemptContextTruncated`
    - `AttemptContextDelivered`
  - Persisted immutable manifest per attempt with:
    - resolved artifact IDs/revisions/hashes
    - template resolution metadata
    - retry links to prior attempt manifest hashes
    - budget/truncation telemetry

- **Template + override semantics**
  - Resolved latest instantiated template snapshot at attempt start and froze resolution.
  - Applied explicit per-task include/exclude attachment overrides with audit event emission.
  - Guaranteed retry linkage to prior attempt manifest hashes.

- **CLI observability**
  - Extended `attempt inspect --context` to return structured context provenance:
    - `retry`
    - `manifest`
    - `manifest_hash`
    - `inputs_hash`
    - `delivered_context_hash`

- **Tests**
  - Added Sprint 39 integration test coverage for:
    - deterministic `inputs_hash` across equivalent first attempts
    - template resolution + include/exclude override behavior
    - retry manifest linkage visibility in `attempt inspect --context`
  - Updated Sprint 35 integration assertion to match prompt structure (`Documents:` section).

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Attempt context assembly is deterministic and fully inspectable | PASS | Integration test + manual inspect output in `19-sprint39-manual.log` |
| 2 | Context replay reproduces identical manifests for identical inputs | PASS | Matching first-attempt `inputs_hash` across equivalent flows (`9448ad9e0801e254`) |
| 3 | Runtime initialization does not rely on hidden state | PASS | Manifest explicitly records included/excluded sources; runtime context built from explicit sections/events |
| 4 | Manual validation in `@hivemind-test` completed/documented | PASS | Sprint 39 checklist/report + fresh-clone smoke/event logs |

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
- Line coverage (from LCOV `DA` entries): **69.47%**.

## 6. Manual Validation (`@hivemind-test`)

Manual run artifacts:

- `hivemind-test/test-report/19-sprint39-manual.log`
- `hivemind-test/test-report/20-sprint39-ci-worktree.log`
- `hivemind-test/test-report/21-sprint39-ci-execution.log`
- `hivemind-test/test-report/22-sprint39-ci-events.log`
- `hivemind-test/test-report/sprint-39-manual-checklist.md`
- `hivemind-test/test-report/sprint-39-manual-report.md`

Validated manually:

- deterministic manifest hash inputs across repeated first attempts
- retry context/manifest linkage and inspectability via `attempt inspect --context`
- template resolution freeze + attachment include/exclude override behavior
- explicit context event observability (`attempt_context_*`, `retry_context_assembled`)
- fresh-clone smoke tests (`test_worktree.sh`, `test_execution.sh`) and event inspection

## 7. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-3.md` (Sprint 39 checklist + exit criteria marked complete)
- `docs/architecture/architecture.md`
- `docs/architecture/cli-capabilities.md`
- `docs/architecture/event-model.md`
- `docs/architecture/state-model.md`
- `docs/architecture/taskflow.md`
- `docs/design/cli-semantics.md`
- `docs/design/retry-context.md`
- `docs/overview/quickstart.md`
- `hivemind-test/test-report/sprint-39-manual-checklist.md`
- `hivemind-test/test-report/sprint-39-manual-report.md`
- `ops/reports/sprint-39-report.md`
- `changelog.json` (Sprint 39 / `v0.1.30` entry)
- `Cargo.toml` version bump to `0.1.30`

## 8. Principle Checkpoint

- **1 Observability is truth**: context assembly, override, truncation, and delivery are explicit evented steps.
- **2 Fail fast, fail loud**: invalid/missing governance references surface as structured failures during assembly.
- **3 Reliability over cleverness**: deterministic ordered inputs and hash lineage replace implicit runtime memory.
- **7 CLI-first**: `attempt inspect --context` exposes context provenance through machine-readable output.
- **8 Absolute observability**: attempt context contracts are inspectable from both events and CLI projections.
- **10 Failures are first-class**: retry links and prior manifest hashes preserve full attempt lineage.
- **11 Build incrementally**: Sprint 39 builds on governance storage/templates/constitution/snapshot foundations from Sprints 34-38.
- **15 No magic**: runtime context provenance is explainable from explicit manifest fields and event trail.

## 9. Challenges

1. **Challenge**: Manual/fresh-clone scripts assumed local `target/release/hivemind` path, while this environment uses a shared cargo target directory.
   - **Resolution**: Ran fresh-clone scripts with explicit `HIVEMIND` binary path set to the built release artifact.

2. **Challenge**: Maintaining deterministic fingerprinting while supporting template documents and per-task include/exclude overrides.
   - **Resolution**: Normalized ordered inputs, stable sorted artifact IDs, and explicit excluded source lists in manifest/input hashing payload.

## 10. Next Sprint Readiness

- Sprint 40 (Governance Replay and Recovery) can now rely on immutable attempt context manifests and hash-linked retry lineage as replay/recovery inputs.
