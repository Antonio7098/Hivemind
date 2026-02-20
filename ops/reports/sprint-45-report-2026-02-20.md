# Sprint 45: Active Context Window and Deterministic Prompt Assembly

## 1. Sprint Metadata

- **Sprint**: 45
- **Title**: Active Context Window and Deterministic Prompt Assembly
- **Branch**: `sprint/45-active-context-window`
- **Date**: 2026-02-20
- **Owner**: Antonio

## 2. Objectives

- Replace ad hoc attempt context truncation with an operation-driven, attempt-scoped context window.
- Preserve legacy context manifest/delivery contracts while introducing deterministic manifest v2 semantics.
- Add context-window observability events with explicit provenance and pruning reasons.
- Validate deterministic hashing and truncation explainability, including manual artifacts in `@hivemind-test`.

## 3. Delivered Changes

- **Active context window module**
  - Added `src/core/context_window.rs` and exported via `src/core/mod.rs`.
  - Implemented operation semantics: `add`, `remove`, `expand`, `prune`, `snapshot`.
  - Added deterministic state hashing over canonical payloads.
  - Added explicit budget policy (`total`, `default/per-section`, `max_expand_depth`, `deduplicate`).

- **Attempt context assembly v2 integration**
  - Reworked `assemble_attempt_context` in `src/core/registry.rs` to build context through `ContextWindow`.
  - Upgraded manifest schema/version to `attempt_context.v2` / `manifest_version=2`.
  - Replaced ad hoc truncation with operation-driven prune outputs and explicit `truncation_reasons`.
  - Preserved compatibility with `AttemptContextAssembled` / `AttemptContextDelivered` event contracts.

- **Context-window observability events**
  - Added payloads in `src/core/events.rs`:
    - `ContextWindowCreated`
    - `ContextOpApplied`
    - `ContextWindowSnapshotCreated`
  - Extended `AttemptContextTruncated` with `section_reasons`.
  - Wired event emission in registry flow tick and replay/read paths (`state.rs`, `main.rs`, `server.rs`).

- **CLI inspectability updates**
  - Extended `attempt inspect --context` projection to include:
    - `context_window_hash`
    - `rendered_prompt_hash`
  - Preserved existing context projection fields for backward compatibility.

- **Testing updates**
  - Added unit/property/golden tests in `src/core/context_window.rs` (`proptest` + `insta`).
  - Updated integration regression in `tests/integration.rs` to assert deterministic context-window hash and manifest v2 visibility.

## 4. Codex Study and Crate/Practice Research

- Audited `@codex-main/codex-rs` for deterministic harness and observability patterns (tool lifecycle attribution, stable replay assertions, and strict failure classification).
- Reviewed `@codex-main/.npmrc` workspace policy defaults and retained deterministic dependency policy expectations for cross-repo consistency.
- Applied Sprint 45 crate guidance:
  - `proptest` for determinism/property coverage
  - `insta` for context snapshot/golden shape testing

## 5. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Context window is mutable, bounded, and replayable | PASS | `ContextWindow` op model + event family + replay-safe state handling |
| 2 | Prompt assembly is deterministic and hash-stable | PASS | unit/property tests + integration hash assertions + manual hash comparison artifact |
| 3 | Legacy attempt context inspection remains functional | PASS | compatibility events preserved + `attempt inspect --context` regression |
| 4 | Manual validation in `@hivemind-test` is completed and documented | PASS | Sprint 45 checklist/report + timestamped manual artifacts |

**Overall: 4/4 criteria passed**

## 6. Validation

Executed in sprint branch:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test context_window -- --nocapture`
- `cargo test cli_attempt_inspect_context_returns_manifest_and_retry_linkage -- --nocapture`

Result: ✅ validation suite passes for Sprint 45 scope.

## 7. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/test-report/sprint-45-manual-checklist.md`
- `hivemind-test/test-report/sprint-45-manual-report.md`
- `hivemind-test/test-report/2026-02-20-sprint45-manual.log`
- `hivemind-test/test-report/2026-02-20-sprint45-flow1-events.json`
- `hivemind-test/test-report/2026-02-20-sprint45-flow2-events.json`
- `hivemind-test/test-report/2026-02-20-sprint45-attempt-inspect-context.json`
- `hivemind-test/test-report/2026-02-20-sprint45-hash-compare.json`

Validated manually:

- Deterministic context window and inputs hashes across equivalent runs
- Context operation inspection (`ContextOpApplied`) with explicit prune reasons
- Manifest v2 + inspect hash surface (`context_window_hash`, `rendered_prompt_hash`)
- Truncation explainability alignment between prune op and compatibility truncation event

## 8. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-4.md` (Sprint 45 checklist + exit criteria completion)
- `docs/architecture/event-model.md`
- `docs/architecture/cli-capabilities.md`
- `docs/architecture/taskflow.md`
- `docs/design/cli-semantics.md`
- `docs/design/retry-context.md`
- `ops/reports/sprint-45-report-2026-02-20.md`
- `changelog.json` (`v0.1.41`, Sprint 45)
- `Cargo.toml` version bump to `0.1.41`
- `Cargo.lock` package version bump to `0.1.41`

## 9. Principle Checkpoint

- **1 Observability is truth**: context assembly now emits explicit operation-level events and snapshot lineage.
- **3 Reliability over cleverness**: deterministic ordering/hash semantics and explicit policy constraints replace implicit truncation paths.
- **8 Absolute observability**: prune/truncation reasons are surfaced in both operation and compatibility event layers.
- **13 Abstraction without loss of control**: context window abstraction preserves explicit low-level op trail.
- **15 No magic**: active context evolution and delivered prompt lineage are inspectable and attributable.
