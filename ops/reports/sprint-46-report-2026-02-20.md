# Sprint 46: Graph Query Runtime Integration (UCP Substrate)

## 1. Sprint Metadata

- **Sprint**: 46
- **Title**: Graph Query Runtime Integration (UCP Substrate)
- **Branch**: `sprint/46-graph-query-runtime-integration`
- **Date**: 2026-02-20
- **Owner**: Antonio

## 2. Objectives

- Make UCP graph snapshot artifacts directly queryable through deterministic bounded primitives.
- Expose graph query inspection paths in CLI and native runtime tool execution.
- Enforce stale/missing/integrity gates before runtime graph-query use with actionable recovery hints.
- Emit query telemetry for observability (result size, traversal cost, truncation, duration, fingerprint).

## 3. Delivered Changes

- **Graph query substrate**
  - Added `src/core/graph_query.rs` and exported via `src/core/mod.rs`.
  - Implemented bounded deterministic primitives: `neighbors`, `dependents`, `subgraph`, `filter`.
  - Added partition/path matching utilities and deterministic aggregate fingerprint helper.

- **CLI graph-query surface**
  - Added `graph query` subcommands in `src/cli/commands.rs`.
  - Wired command execution and formatted output in `src/main.rs`.

- **Runtime/native tool integration**
  - Added `graph_query` built-in native tool in `src/native/tool_engine.rs`.
  - Added runtime tool context env plumbing in `src/native/adapter.rs` and registry tick path.
  - Reused registry snapshot freshness gates for native runtime pre-flight env injection.

- **Staleness and integrity gates**
  - Enforced stale/missing snapshot hard-fail with refresh hint in runtime graph query path.
  - Preserved per-repository canonical fingerprint verification and aggregate fingerprint validation in native tool pipeline.

- **Observability updates**
  - Added `GraphQueryExecuted` event payload in `src/core/events.rs`.
  - Wired event replay/no-op and event label/category mappings in `src/core/state.rs`, `src/main.rs`, and `src/server.rs`.
  - Added native tool-response telemetry bridge in registry to emit `GraphQueryExecuted` for successful native `graph_query` calls.

- **Testing updates**
  - Added graph query substrate unit tests in `src/core/graph_query.rs`.
  - Added native tool graph-query tests (success + stale failure) in `src/native/tool_engine.rs`.
  - Added runtime integration tests for native graph-query telemetry/stale failure in `src/core/registry.rs`.
  - Added CLI integration test for bounded query output in `tests/integration.rs`.

## 4. Codex Study and Crate/Practice Research

- Audited `@codex-main/codex-rs` runtime harness/tool trace tests (notably `core/tests/suite/tool_harness.rs`, permission/error tests) for deterministic tool lifecycle expectations.
- Reviewed `@codex-main/.npmrc` workspace policy defaults (node linker/peer/workspace settings).
- Attempted critical codex-rs harness test execution:
  - `cargo test -p codex-core --test all tool_harness::shell_tool_executes_command_and_streams_output -- --nocapture`
  - **Blocked** by missing `libcap` (`libcap.pc` not found) in local environment; source/test-surface audit completed despite runtime blocker.
- Reused existing crate strategy already present in project:
  - `schemars`/`jsonschema` for typed tool contract validation
  - `proptest` and integration regression coverage for determinism and guardrail behavior

## 5. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Runtime can query graph substrate via bounded tools | PASS | native `graph_query` tool + runtime env gating + registry/native integration tests |
| 2 | Query behavior is deterministic and observable | PASS | deterministic ordering/bounds tests + `GraphQueryExecuted` telemetry in CLI/native flows |
| 3 | Integrity/staleness gates prevent invalid graph context | PASS | stale snapshot hard-fail tests and manual stale path with refresh hint |
| 4 | Manual validation in `@hivemind-test` is completed and documented | PASS | Sprint 46 checklist/report and timestamped log artifacts |

**Overall: 4/4 criteria passed**

## 6. Validation

Executed in sprint branch:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test --all-features -- --ignored`
- `cargo llvm-cov --all-features --lcov --output-path lcov.info`
- `cargo llvm-cov --all-features --summary-only`
- `cargo test --doc`
- `cargo doc --no-deps --document-private-items`

Result: ✅ commands pass.

Coverage summary (`cargo llvm-cov --summary-only`):
- **TOTAL line coverage:** `67.67%`
- **TOTAL region coverage:** `68.11%`

## 7. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/test-report/sprint-46-manual-checklist.md`
- `hivemind-test/test-report/sprint-46-manual-report.md`
- `hivemind-test/test-report/2026-02-20-sprint46-worktree.log`
- `hivemind-test/test-report/2026-02-20-sprint46-execution.log`
- `hivemind-test/test-report/2026-02-20-sprint46-events.log`
- `hivemind-test/test-report/2026-02-20-sprint46-fs-events.log`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-manual.log`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-determinism.json`
- `hivemind-test/test-report/2026-02-20-sprint46-graph-query-event-slice.json`
- `hivemind-test/test-report/2026-02-20-sprint46-depth-bound.json`

Validated manually:

- Deterministic CLI query output and canonical fingerprint stability on repeated runs.
- Query depth bound rejection (`graph_query_depth_exceeded`).
- Runtime stale snapshot hard-fail (`graph_snapshot_stale`) with refresh hint.
- Runtime successful graph-query tool path with `graph_query_executed` (`source=native_tool_graph_query`).

## 8. Documentation and Release Artifacts

Updated:

- `ops/roadmap/phase-4.md` (Sprint 46 checklist + exit criteria completion)
- `docs/architecture/runtime-adapters.md`
- `docs/architecture/cli-capabilities.md`
- `docs/architecture/event-model.md`
- `docs/design/cli-semantics.md`
- `ops/reports/sprint-46-report-2026-02-20.md`
- `hivemind-test/test-report/sprint-46-manual-checklist.md`
- `hivemind-test/test-report/sprint-46-manual-report.md`
- `changelog.json` (`v0.1.42`, Sprint 46)
- `Cargo.toml` version bump to `0.1.42`
- `Cargo.lock` package version bump to `0.1.42`

## 9. Principle Checkpoint

- **1 Observability is truth**: query telemetry (`GraphQueryExecuted`) is emitted for both CLI and native paths with explicit cost/result metrics.
- **2 Fail fast, fail loud**: stale/missing/integrity violations fail deterministically with explicit error codes and refresh hint.
- **7 CLI-first**: graph query primitives are first-class CLI inspection commands.
- **13 Abstraction without loss**: graph-query substrate stays deterministic and bounded while preserving canonical fingerprint lineage.
- **15 No magic**: runtime graph-query eligibility is explicit via snapshot/constitution gate env surfaces and auditable events.
