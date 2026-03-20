# Architecture Refactor Summary

## The Issue
Prior to this refactoring cycle, Hivemind's architecture had accumulated structural issues that threatened long-term maintainability, extensibility, and testability. Specifically:
- **Monolithic God-Facades**: The `Registry` struct was a default, broad dependency for most callers, mingling unrelated domain logic.
- **Oversized Execution Hotspots**: Core execution paths, event reducers, and the native tool engine loop had grown into large, entangled files that handled orchestration, state mutation, and side effects all at once.
- **Leaky Layering**: Delivery layers (CLI and HTTP Server) had direct dependencies on wiring, registry extraction, and cross-cutting orchestrations instead of acting as thin transport shells.
- **Monolithic Testing**: Tests were heavily centralized (e.g., a massive `integration.rs` suite and oversized native tests), meaning failures were hard to localize.
- **Implicit Dependency Drift**: There were no programmatic boundaries preventing core logic from depending on external delivery layers or adapters.

## What We Did
We executed a structured, repo-wide architectural refactoring pass tracking against our SOLID goals, achieving the following:

1. **Composition Root & Guardrails**:
   - Introduced an explicit Composition Root to handle wiring.
   - Added CI-backed architecture guardrails (`scripts/check_architecture.py`) that strictly enforce dependency direction rules, track file-size budgets, and enforce explicit tracking of `too_many_lines` suppression.
2. **Service Extraction (`Registry`)**:
   - Decomposed the `Registry` god-object into narrow, focused domain services (`ProjectService`, `TaskService`, `GraphService`, `FlowService`, `RuntimeService`, `GovernanceService`, `WorktreeService`).
   - `Registry` now acts purely as a composition root/factory for these narrower services.
3. **Execution & State De-concentration**:
   - Split monolithic event reducers into aggregate-specific application logic.
   - Decomposed core runtime execution hotspots, isolating artifact inspection, projection assembly, and check execution from pure orchestration logic.
4. **Native Subsystem Refactoring**:
   - Separated prompt assembly, budget compaction, and tool policy evaluation (approval/sandbox/network) from the core agent loop and command-runner tool.
5. **Delivery Layer Clean-up**:
   - Extracted all registry, store, and runtime construction out of CLI handlers and server entrypoints. Route handlers are now thin wrappers calling into app services.
6. **Test Suite Restructuring**:
   - Split `tests/integration.rs` into targeted capability suites (`runtime_scope.rs`, `query_views.rs`, `flow_lifecycle.rs`, etc.).
   - Split the `native` test suite into focused subsystem modules.

## How It Helped
- **Easier to Change & Extend**: Adding a new runtime adapter or tool command no longer requires modifying central switch statements or monolithic files.
- **Easier to Test**: Test failures now point to precise subsystem boundaries. Localized unit tests are much faster and more isolated.
- **Harder to Accidentally Couple**: The `check_architecture.py` script automatically fails the build if a developer introduces a forbidden dependency across architectural boundaries or exceeds predefined file size budgets without explicit acknowledgment.
- **Cleaner Core**: Delivery layers remain entirely agnostic of domain orchestration, meaning the CLI and Server can evolve (or be replaced) without impacting core logic.

## Remaining Architectural Debt
During the refactor, we opted to preserve behavior rather than forcing complete rewrites of large functions. As a result, functions that still exceed the standard line limit are now explicitly marked with `// ARCH_DEBT: legacy oversized function` to permit `allow(clippy::too_many_lines)` under CI checks. 

A programmatic search (`Select-String -Pattern "ARCH_DEBT"`) reveals the following known legacy oversized functions that remain as debt to be tackled in future passes:

**CLI & Server Layer:**
- `src/server/routes/chat/session/mutate.rs`
- `src/server/event_ui/categories.rs`
- `src/server/event_ui/types.rs`
- `src/cli/handlers/global/skills.rs`
- `src/cli/handlers/project/governance/mod.rs`
- `src/cli/handlers/events/native_summary/builder.rs`
- `src/cli/handlers/events/labels/mod.rs`

**Core Flow & Governance & State:**
- `src/core/registry/flow/execution/progress.rs`
- `src/core/registry/flow/checkpoint/completion.rs`
- `src/core/registry/governance/constitution/commands.rs` (2 instances)
- `src/core/registry/governance/constitution/validate.rs`
- `src/core/registry/governance/introspection/diagnose.rs`
- `src/core/registry/governance/recovery/restore.rs`
- `src/core/registry/governance/recovery/snapshot.rs`
- `src/core/registry/graph/snapshot/refresh.rs`
- `src/core/registry/graph/management/wiring.rs`
- `src/core/registry/graph/constitution/rules.rs`
- `src/core/registry/graph/constitution/snapshot.rs`
- `src/core/registry/runtime/management/project.rs`
- `src/core/registry/events/recover.rs`
- `src/core/state/catalog/governance.rs`
- `src/core/state/catalog/runtime.rs`
- `src/core/state/apply/graph.rs`
- `src/core/context_window/pruning.rs`
- `src/core/graph_query/index.rs`
- `src/core/graph_query/query_engine/tests.rs`

**Native Subsystem:**
- `src/native/adapter/runtime.rs` (2 instances)
- `src/native/tool_engine/run_command_tool.rs`
- `src/native/tool_engine/policy_eval.rs`
- `src/native/tool_engine/engine/dispatch.rs`
- `src/native/tool_engine/exec_sessions/commands.rs`
- `src/native/prompt_assembly.rs`
- `src/native/turn_items.rs`
- `src/native/turn_items/budget_compaction.rs`

**Adapters:**
- `src/adapters/opencode/interactive.rs`
- `src/adapters/opencode/runtime_impl.rs`

*(Note: There are also equivalent occurrences mapped in the `ops/registry_migration/` scripts which mirror the original oversized functions).*

These files and functions are explicitly tracked within `scripts/check_architecture.py`. Any further structural growth in these files will cause CI to fail, enforcing that we slowly dismantle this remaining debt over time rather than compounding it.
