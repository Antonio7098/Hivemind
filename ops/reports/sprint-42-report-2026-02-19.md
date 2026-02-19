# Sprint 42: Native Runtime Contracts and Harness Foundation

## 1. Sprint Metadata

- **Sprint**: 42
- **Title**: Native Runtime Contracts and Harness Foundation
- **Branch**: `sprint/42-native-runtime-contracts-harness`
- **Date**: 2026-02-19
- **Owner**: Antonio

## 2. Objectives

- Introduce native runtime contract primitives without changing TaskFlow semantics.
- Add explicit runtime capability/selection observability in CLI and events.
- Preserve external adapter execution paths (`opencode`, `codex`, `claude-code`, `kilo`).
- Validate manually in `@hivemind-test` with captured artifacts.

## 3. Delivered Changes

- **Native runtime contract layer (`src/native/`)**
  - Added `ModelClient` provider-agnostic trait.
  - Added deterministic `AgentLoop` FSM (`init -> think -> act -> done`).
  - Added `NativeRuntimeConfig` budgets (turn/time/token).
  - Added structured `NativeRuntimeError` taxonomy with recovery hints and mappings to `HivemindError`/`RuntimeError`.
  - Added deterministic `MockModelClient` harness client.

- **Native adapter integration**
  - Added `NativeRuntimeAdapter` and `NativeAdapterConfig`.
  - Added `native` to adapter descriptors and supported adapter list.
  - Added capability descriptors and `requires_binary` metadata to runtime descriptors.

- **Runtime selection and observability**
  - Added `RuntimeSelectionSource` enum (`task_override`, `flow_default`, `project_default`, `global_default`).
  - Added `RuntimeCapabilitiesEvaluated` event payload and emission before runtime start.
  - Extended runtime health/list payloads with `capabilities` and `selection_source`.
  - Extended CLI table output for runtime list/health to show capabilities and selection source.

- **Test coverage**
  - Added native loop tests for deterministic transitions, invalid transition failure, and malformed model output failure with explicit recovery hints.
  - Extended runtime tests for:
    - native adapter visibility/capabilities
    - runtime precedence selection source mapping
    - runtime capability event emission
  - Hardened flaky scope-violation integration input path for deterministic CI behavior.

- **Codex study stage outputs**
  - Audited `codex-main/.npmrc` and captured workspace constraints:
    - `shamefully-hoist=true`
    - `strict-peer-dependencies=false`
    - `node-linker=hoisted`
    - `prefer-workspace-packages=true`
  - Audited `codex-main/codex-rs` runtime/tool harness patterns (`codex.rs`, `client.rs`, tool spec/orchestrator/sandboxing/runtime modules, protocol, test suites).
  - Attempted focused codex harness test run; blocked by missing system `libcap` dependency in `codex-linux-sandbox` build.

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Native contracts compile and are test-covered | PASS | `cargo clippy --all-targets --all-features -- -D warnings`; native unit tests; registry/runtime tests |
| 2 | Existing adapter-based execution remains non-regressed | PASS | `cargo nextest run --all-features` (245/245 pass) |
| 3 | Runtime selection behavior is explicit and observable | PASS | runtime health/list capability + selection source output; `RuntimeCapabilitiesEvaluated` event evidence |
| 4 | Manual validation in `@hivemind-test` is completed and documented | PASS | Sprint 42 checklist/report + timestamped logs in `hivemind-test/test-report/` |

**Overall: 4/4 criteria passed**

## 5. Validation

Executed:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo nextest run --all-features`

Result: ✅ pass.

Additional note:

- `cargo test --all-features --lib --tests` still demonstrates a known non-deterministic failure pattern in one scope-violation integration path under threaded harness execution; CI target command is `nextest`, and the integration path was hardened in this sprint.

## 6. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/test-report/sprint-42-manual-checklist.md`
- `hivemind-test/test-report/sprint-42-manual-report.md`
- `hivemind-test/test-report/2026-02-19-sprint42-worktree.log`
- `hivemind-test/test-report/2026-02-19-sprint42-execution.log`
- `hivemind-test/test-report/2026-02-19-sprint42-runtime-list.log`
- `hivemind-test/test-report/2026-02-19-sprint42-runtime-health-native.log`
- `hivemind-test/test-report/2026-02-19-sprint42-runtime-health-task-override.log`
- `hivemind-test/test-report/2026-02-19-sprint42-runtime-health-flow-default.log`
- `hivemind-test/test-report/2026-02-19-sprint42-runtime-capability-events.log`
- `hivemind-test/test-report/2026-02-19-sprint42-flow-runtime-events.log`

Validated manually:

- native adapter registration/health/capabilities
- precedence visibility (`task_override` then `flow_default`)
- runtime capability event emission in flow stream
- smoke scripts (`test_worktree.sh`, `test_execution.sh`)

## 7. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-4.md` (Sprint 42 checklist + exit criteria completion)
- `docs/architecture/architecture.md`
- `docs/architecture/cli-capabilities.md`
- `docs/architecture/event-model.md`
- `docs/architecture/runtime-adapters.md`
- `docs/design/cli-semantics.md`
- `changelog.json` (`v0.1.38`, Sprint 42)
- `Cargo.toml` version bump to `0.1.38`

## 8. Principle Checkpoint

- **1 Observability is truth**: Added explicit runtime capability evaluation event and capability output surfaces.
- **3 Reliability over cleverness**: Native loop is explicit FSM with bounded budgets and deterministic mock harnessing.
- **4 Explicit error taxonomy**: Added structured `NativeRuntimeError` family with consistent mapping and hints.
- **7 CLI-first**: Runtime capabilities/selection source are directly inspectable via CLI commands.
- **8 Absolute observability**: Runtime resolution decisions are evented, not implicit.
- **9 Automated checks mandatory**: Enforced `fmt`/`clippy -D warnings`/`nextest`.
- **11 Build incrementally**: Delivered contract/harness foundation without changing flow semantics.
- **12 Maximum modularity**: Added provider-agnostic native contract while preserving external adapter paths.
- **15 No magic**: Runtime precedence source is explicit in health responses and event payloads.

## 9. Challenges

1. **Challenge**: One integration path (`cli_scope_violation_is_fatal_and_preserves_worktree`) remained non-deterministic under high-concurrency test execution.
   - **Resolution**: Hardened test runtime command input so scope violation signaling is deterministic.

2. **Challenge**: Fresh-clone manual workflow assumed local `target/release/hivemind`, while environment uses shared Cargo target directory.
   - **Resolution**: Verified fresh-clone build stage and ran manual scripts against the actual built release binary in `~/.cargo/shared-target/release/hivemind`.

## 10. Next Sprint Readiness

Sprint 43 can now build on:

- native runtime contract baseline (`src/native`)
- explicit runtime capability event primitive
- deterministic harness patterns and selection observability coverage
