# Sprint 43: Native Runtime Event Model and Blob Storage

## 1. Sprint Metadata

- **Sprint**: 43
- **Title**: Native Runtime Event Model and Blob Storage
- **Branch**: `sprint/43-native-runtime-events-blobs`
- **Date**: 2026-02-19
- **Owner**: Antonio

## 2. Objectives

- Make native runtime execution replayable via explicit invocation/turn/tool event contracts.
- Persist large model/tool payloads as hash-addressed blobs with event-level provenance.
- Preserve legacy flow/task semantics while adding native-only observability depth.
- Validate manually in `@hivemind-test` for success/failure trails and blob integrity.

## 3. Delivered Changes

- **Native runtime event family**
  - Added payloads:
    - `AgentInvocationStarted`
    - `AgentTurnStarted`
    - `ModelRequestPrepared`
    - `ModelResponseReceived`
    - `ToolCallRequested`
    - `ToolCallStarted`
    - `ToolCallCompleted`
    - `ToolCallFailed`
    - `AgentTurnCompleted`
    - `AgentInvocationCompleted`
  - Added payload-level `native_correlation` IDs (`project`, `graph`, `flow`, `task`, `attempt`) and aligned metadata correlation.

- **Blob/provenance strategy**
  - Added hash-addressed blob persistence under `~/.hivemind/blobs/sha256/<prefix>/<digest>.blob`.
  - Added `NativeBlobRef` metadata in native request/response/tool events:
    - `digest`
    - `byte_size`
    - `media_type`
    - `blob_path`
    - optional inline `payload` (capture-mode dependent)
  - Added capture mode policy:
    - default `metadata_only`
    - explicit `full_payload` via `HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS=true`
  - Added invocation provenance fields:
    - provider (`HIVEMIND_NATIVE_PROVIDER`)
    - model
    - runtime version

- **Native runtime trace plumbing**
  - Extended runtime execution reporting with structured native invocation traces (turns, tool calls, failures).
  - Converted native adapter failure path to structured report output so invocation completion events are still emitted.
  - Added deterministic directive support for synthetic tool failure testing (`ACT:fail_tool:<message>`).

- **Replay/non-regression coverage**
  - Added registry tests for:
    - complete native event trail + blob reference integrity
    - deterministic replay/idempotence characteristics
    - failed tool-call path and failed invocation completion semantics
  - Added non-regression assertion proving non-native runs do not emit native event-family payloads.

## 4. Codex Study and Crate/Practice Research

- Audited `codex-main/.npmrc` workspace policy:
  - `shamefully-hoist=true`
  - `strict-peer-dependencies=false`
  - `node-linker=hoisted`
  - `prefer-workspace-packages=true`
- Audited `codex-main/codex-rs` harness/testing surfaces:
  - `core/tests/suite/tool_harness.rs`
  - `core/tests/suite/permissions_messages.rs`
  - `core/Cargo.toml`
- Ran focused codex harness test path:
  - `cargo test -p codex-core --test all suite::tool_harness::update_plan_tool_emits_plan_update_event -- --nocapture`
  - blocked by missing host `libcap` (`codex-linux-sandbox` build dependency).
- Evaluated Sprint 43 crate choices and best practices:
  - Used `sha2` (`SHA-256`) for deterministic, widely interoperable digesting and straightforward filesystem sharding.
  - Preserved `serde_json`-based stable payload serialization from existing runtime structures.
  - Kept canonical persistence in SQLite event store and mirrored blob authority in deterministic filesystem paths.

## 5. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Native runtime emits complete, attributable event trails | PASS | Unit/integration tests + manual success/failure event artifacts |
| 2 | Replay reproduces identical native runtime projection state | PASS | Replay/idempotence test coverage in `registry` native event tests |
| 3 | Large payload strategy is observable and configurable | PASS | Blob metadata refs + capture-mode config tests/manual logs |
| 4 | Manual validation in `@hivemind-test` is completed and documented | PASS | Sprint 43 checklist/report + timestamped logs under `hivemind-test/test-report/` |

**Overall: 4/4 criteria passed**

## 6. Validation

Executed in sprint branch:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test --all-features -- --ignored`
- `cargo llvm-cov --all-features --lcov --output-path lcov.info --ignore-run-fail`
- `cargo doc --no-deps --document-private-items`

Result: ✅ validation commands completed; known intermittent scope-violation integration flake remains in full threaded test runs, with isolated reruns passing. Coverage report remains below target (baseline constraint), while Sprint 43 native-path additions are directly test-covered.

## 7. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/test-report/sprint-43-manual-checklist.md`
- `hivemind-test/test-report/sprint-43-manual-report.md`
- `hivemind-test/test-report/2026-02-19-sprint43-worktree.log`
- `hivemind-test/test-report/2026-02-19-sprint43-execution.log`
- `hivemind-test/test-report/2026-02-19-sprint43-native-manual.log`
- `hivemind-test/test-report/2026-02-19-sprint43-success-events.json`
- `hivemind-test/test-report/2026-02-19-sprint43-success-blob-verify.log`
- `hivemind-test/test-report/2026-02-19-sprint43-failure-events.json`
- `hivemind-test/test-report/2026-02-19-sprint43-failure-blob-verify.log`

Validated manually:

- native event family completeness for success and failure invocations
- payload/blob reference digest-size integrity
- metadata-only default capture behavior
- explicit full payload capture behavior
- payload recovery from blob paths

## 8. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-4.md` (Sprint 43 checklist + exit criteria completion)
- `docs/architecture/architecture.md`
- `docs/architecture/cli-capabilities.md`
- `docs/architecture/event-model.md`
- `docs/architecture/runtime-adapters.md`
- `docs/design/cli-semantics.md`
- `docs/design/event-replay.md`
- `changelog.json` (`v0.1.39`, Sprint 43)
- `Cargo.toml` version bump to `0.1.39`
- `Cargo.lock` package version bump to `0.1.39`

## 9. Principle Checkpoint

- **1 Observability is truth**: Native invocation internals are explicit event contracts with stable attribution.
- **8 Absolute observability**: Model/tool request-response trails and failure paths are event-visible and queryable.
- **10 Failures are first-class outcomes**: Tool-call failures are explicitly emitted and preserved in invocation completion semantics.
- **11 Build incrementally**: Added event/blob layer without changing core flow/task semantics.
- **12 Maximum modularity**: Native observability added without regressing or coupling external adapter paths.
- **15 No magic**: Correlation, provenance, capture mode, and blob references are explicit and inspectable.

## 10. Next Sprint Readiness

Sprint 44 can now build on:

- complete native invocation event trail contracts
- hash-addressed payload artifact persistence
- capture-mode policy and provenance surfaces
- deterministic test harness coverage for tool-call success/failure emission paths
