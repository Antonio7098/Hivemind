# Sprint 44: Tool Engine v1 (Typed, Safe, Replayable)

## 1. Sprint Metadata

- **Sprint**: 44
- **Title**: Tool Engine v1 (Typed, Safe, Replayable)
- **Branch**: `sprint/44-tool-engine-v1`
- **Date**: 2026-02-19
- **Owner**: Antonio

## 2. Objectives

- Build a native typed tool engine with strict input/output schema validation.
- Introduce deterministic registry dispatch and structured failure taxonomy for tool execution.
- Enforce scope/policy boundaries for native tools (especially command execution).
- Validate end-to-end behavior in `@hivemind-test` and preserve full event-level inspectability.

## 3. Delivered Changes

- **Typed tool engine and contracts**
  - Added `src/native/tool_engine.rs` with `ToolContract` metadata:
    - `name` + `version`
    - input/output JSON schemas
    - required scope + permissions
    - timeout/cancellation envelope
  - Added deterministic registry dispatch keyed by `name@version`.
  - Added structured tool error taxonomy for unknown tool, schema violations, scope violations, policy violations, execution failures, and timeout failures.

- **Built-in tool set (Sprint 44 minimum)**
  - Added built-ins:
    - `read_file`
    - `list_files`
    - `write_file` (scope-gated)
    - `run_command` (deny-by-default policy)
    - `git_status` and `git_diff` (read-oriented)
  - Added action parsing support for both concise and object-form tool directives.

- **Safety/policy enforcement integration**
  - Added env-driven `run_command` policy controls:
    - `HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST`
    - `HIVEMIND_NATIVE_TOOL_RUN_COMMAND_DENYLIST`
    - `HIVEMIND_NATIVE_TOOL_RUN_COMMAND_DENY_BY_DEFAULT`
  - Wired task scope into native execution via `HIVEMIND_TASK_SCOPE_JSON`.
  - Enforced deterministic failure semantics for policy/scope denials in registry/native flow execution.

- **Observability and runtime wiring**
  - Routed `ACT:tool:*` native directives through typed tool engine execution.
  - Preserved legacy `ACT:fail_tool:*` synthetic failure directive for explicit testability.
  - Added native invocation trace outputs containing request/completion/failure tool payloads.

- **Test and benchmark coverage**
  - Added unit tests for schema validation and gate enforcement.
  - Added property test coverage for deterministic replay behavior.
  - Added baseline performance tests for tool dispatch overhead and schema validation latency.
  - Added registry-level integration tests for typed tool success/failure eventing and deterministic failure classification.
  - Hardened scope-violation integration scenarios for deterministic coverage runs (explicit repo-outside-worktree mutation signal + adjusted timeout budget).

## 4. Codex Study and Crate/Practice Research

- Audited `codex-main/.npmrc` policy:
  - `shamefully-hoist=true`
  - `strict-peer-dependencies=false`
  - `node-linker=hoisted`
  - `prefer-workspace-packages=true`
- Audited `codex-main/codex-rs` harness/tool/permission test surfaces:
  - `core/tests/suite/tool_harness.rs`
  - `core/tests/suite/permissions_messages.rs`
  - config loader + protocol/schema/sandbox-related modules
- Applied Sprint 44 crate recommendations from the roadmap appendix:
  - Added `schemars` for schema generation
  - Added `jsonschema` for contract validation
  - Added `serde_path_to_error` for actionable payload error paths
  - Added `proptest` for deterministic replay property coverage

## 5. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Tool engine is typed, schema-validated, and fully event-observable | PASS | New typed engine module + native adapter/registry integration tests + event trace artifacts |
| 2 | Scope/policy violations are explicit and attributable | PASS | `native_scope_violation`/`native_policy_violation` classification tests and manual denied-path event traces |
| 3 | Replay reconstructs tool outcomes deterministically | PASS | Property tests for deterministic tool sequence behavior and replay-safe outputs |
| 4 | Manual validation in `@hivemind-test` is completed and documented | PASS | Sprint 44 manual checklist/report and timestamped logs in `hivemind-test/test-report/` |

**Overall: 4/4 criteria passed**

## 6. Validation

Executed in sprint branch:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test --all-features -- --ignored`
- `cargo llvm-cov --all-features --lcov --output-path lcov.info`
- `cargo test --doc`
- `cargo doc --no-deps --document-private-items`

Result: ✅ required validation suite passes for Sprint 44 scope, including a full non-ignored coverage run with `lcov.info` generated.

## 7. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/test-report/sprint-44-manual-checklist.md`
- `hivemind-test/test-report/sprint-44-manual-report.md`
- `hivemind-test/test-report/2026-02-19-sprint44-worktree.log`
- `hivemind-test/test-report/2026-02-19-sprint44-execution.log`
- `hivemind-test/test-report/2026-02-19-sprint44-events.log`
- `hivemind-test/test-report/2026-02-19-sprint44-native-tools-manual.log`

Validated manually:

- fresh-clone smoke run via `hivemind-test/test_worktree.sh` and `hivemind-test/test_execution.sh`
- successful typed tool path (`list_files`) with native tool lifecycle event visibility
- denied typed tool path (`run_command`) with explicit policy violation attribution
- canonical SQLite event inspection for runtime and filesystem observability signals

## 8. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-4.md` (Sprint 44 checklist + exit criteria completion)
- `docs/architecture/runtime-adapters.md`
- `docs/architecture/event-model.md`
- `docs/design/scope-enforcement.md`
- `hivemind-test/test-report/sprint-44-manual-checklist.md`
- `hivemind-test/test-report/sprint-44-manual-report.md`
- `changelog.json` (`v0.1.40`, Sprint 44)
- `Cargo.toml` version bump to `0.1.40`
- `Cargo.lock` package version bump to `0.1.40`

## 9. Principle Checkpoint

- **2 Fail fast, fail loud**: Unknown/invalid/forbidden tool operations fail deterministically with explicit error taxonomy.
- **4 Explicit error taxonomy**: Tool failures are structured (`native_unknown_tool`, `native_tool_schema_*`, `native_scope_violation`, `native_policy_violation`, etc.).
- **5 Structure and process scale**: Tool execution is contract-driven through a deterministic registry and scoped policy enforcement.
- **9 Automated checks are mandatory**: Unit/integration/property coverage and validation suite gating are in place for the sprint deliverables.
- **15 No magic**: Native tool request/response/failure semantics are event-observable and replay-inspectable.

## 10. Next Sprint Readiness

Sprint 45 can now build on:

- typed native tool contracts with schema enforcement
- deterministic and attributable policy/scope failure semantics
- replay-oriented tool execution traces
- baseline performance guardrails for tool dispatch and validation latency
