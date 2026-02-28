# Sprint 53 Report: Sandbox And Approval Orchestrator

## 1. Sprint Metadata

- **Sprint**: 53
- **Title**: Sandbox And Approval Orchestrator
- **Branch**: `feat/phase-4-5-native-runtime-hardening-sprint-reports`
- **Date**: 2026-02-28
- **Owner**: Antonio

## 2. Objectives

- Add explicit sandbox policy orchestration around native tool execution.
- Add explicit approval modes with deterministic review/caching semantics.
- Add rules-backed exec policy hardening for `run_command` with dangerous-command guards.
- Make sandbox/approval/exec policy decisions event-observable and attributable.

## 3. Delivered Changes

- **Native policy orchestrator (`src/native/tool_engine.rs`)**
  - Added explicit sandbox policy contract and env controls:
    - `read-only`
    - `workspace-write` (writable roots + read-only overlays)
    - `danger-full-access`
    - `host-passthrough`
  - Added platform-aware sandbox default hook (`windows` -> `host-passthrough`, others -> `workspace-write`) and policy tagging.
  - Added explicit approval policy modes:
    - `never`
    - `on-failure`
    - `on-request`
    - `unless-trusted`
  - Added explicit review decision handling (`approve|deny`) and bounded `approved_for_session` cache behavior.
  - Added rules-backed exec policy manager with:
    - allow/deny/deny-by-default base rules
    - bounded prefix amendments (`HIVEMIND_NATIVE_EXEC_PREFIX_RULE_MAX`)
    - broad-prefix protections (`*`, shell-wide prefixes)
    - UNIX + Windows dangerous-command analyzers and deterministic deny paths
  - Enforced sandbox + approval evaluation on every native tool call path before handler execution.

- **Native trace/event observability (`src/adapters/runtime.rs`, `src/core/events.rs`, `src/core/registry.rs`, `src/native/adapter.rs`)**
  - Extended native tool-call trace payload with `policy_tags`.
  - Added `policy_tags` projection to:
    - `ToolCallRequested`
    - `ToolCallStarted`
    - `ToolCallCompleted`
    - `ToolCallFailed`
  - Added/updated assertions proving policy-tag visibility on policy-failure paths.

- **Tests**
  - Added native tool-engine coverage for:
    - read-only sandbox deny behavior
    - approval deny/approve/cached session outcomes
    - dangerous command denial requiring elevated sandbox + explicit approval mode
    - broad prefix rejection
    - bounded prefix-amendment filtering
  - Updated registry integration assertion to require `policy_tags` on native policy violations.

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Every command/tool execution path evaluates sandbox + approval policy explicitly | PASS | `NativeToolEngine::evaluate_tool_policies` executes before every tool handler; unit tests cover read/write/exec paths |
| 2 | Dangerous command and policy conflicts fail deterministically with actionable errors | PASS | new dangerous-command tests + deterministic `native_policy_violation` assertions |
| 3 | Approval outcomes are cached and event-observable | PASS | `approval_cache_marks_second_run_as_cached` + `policy_tags` emitted in tool-call events |

**Overall: 3/3 criteria passed**

## 5. Validation

Executed:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test --all-features -- --ignored`
- `cargo doc --no-deps --document-private-items`
- `cargo llvm-cov --all-features --lcov --output-path lcov.info`
- `cargo llvm-cov report --summary-only`

Result: commands pass.

Coverage summary (`cargo llvm-cov report --summary-only`):

- **TOTAL line coverage:** `68.00%`
- **TOTAL region coverage:** `68.30%`

## 6. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/test-report/sprint-53-manual-checklist.md`
- `hivemind-test/test-report/sprint-53-manual-report.md`
- `hivemind-test/test-report/2026-02-28-sprint53-test_worktree.log`
- `hivemind-test/test-report/2026-02-28-sprint53-test_execution.log`
- `hivemind-test/test-report/2026-02-28-sprint53-event-inspection.log`
- `hivemind-test/test-report/2026-02-28-sprint53-filesystem-inspection.log`

Manual checks confirmed:

- fresh-clone script execution with explicit release binary path override is stable.
- worktree and runtime execution flows remain operational.
- canonical SQLite event store includes expected runtime lifecycle and filesystem observation events.

## 7. Documentation Updates

Updated:

- `ops/roadmap/phase-4.5-native-runtime-hardening.md` (Sprint 53 checklist + exit criteria completion)
- `docs/architecture/runtime-adapters.md`
- `docs/architecture/event-model.md`
- `docs/architecture/cli-capabilities.md`
- `docs/design/scope-enforcement.md`
- `hivemind-test/test-report/sprint-53-manual-checklist.md`
- `hivemind-test/test-report/sprint-53-manual-report.md`
- `ops/reports/sprint-53-report-2026-02-28.md`

## 8. Principle Checkpoint

- **1 Observability is truth**: policy decisions are explicit `policy_tags` on tool-call events.
- **2 Fail fast, fail loud**: policy conflicts return deterministic `native_policy_violation` failures.
- **3 Reliability over cleverness**: explicit mode-based policy contracts replace implicit behavior.
- **4 Explicit error taxonomy**: sandbox/approval/exec denials remain typed and actionable.
- **5 Structure scales**: policy orchestration centralized in native tool engine.
- **6 SOLID doctrine**: policy responsibilities separated (sandbox, approval, exec manager).
- **7 CLI-first**: behavior is runtime/env configurable via explicit keys.
- **8 Absolute observability**: runtime policy outcomes are replay-visible in events.
- **9 Automated checks mandatory**: fmt/clippy/tests/docs/coverage executed.
- **10 Failures are first-class**: denials are preserved as explicit failed tool calls.
- **11 Build incrementally**: Sprint 53 extends Sprint 52 hardening without semantic regressions.
- **12 Maximum modularity**: policy manager abstractions remain adapter/runtime-agnostic.
- **13 Abstraction without loss**: low-level decisions still surfaced as tags and structured errors.
- **14 Human authority**: approval mode + review decision are explicit control boundaries.
- **15 No magic**: all sandbox/approval/exec escalations are explicit and attributable.

## 9. Challenges

1. **Fresh-clone smoke script binary-path mismatch**
   - **Resolution**: use explicit `HIVEMIND=/home/antonio/.cargo/shared-target/release/hivemind` override in manual scripts.

2. **Strict clippy lints with large policy evaluator path**
   - **Resolution**: addressed format/lint findings and added targeted `#[allow(clippy::too_many_lines)]` on policy evaluator only.

## 10. Metrics Summary

- **Current tracked diff summary**: `13 files changed, 1077 insertions(+), 105 deletions(-)`
- **Manual artifact files added**: 6 Sprint 53 test-report artifacts
- **Unit/integration/doc tests executed**: 252 unit + 38 integration + 7 doc tests
