# Hivemind Architectural Audit (SOLID-focused)
Date: 2026-03-06
Scope: `src/core`, `src/adapters`, `src/native`, `src/storage`, `src/cli`, `src/server`, `tests`, CI, and architecture docs.

## Executive Summary
Hivemind has a strong **macro-architecture** and a weaker **micro-architecture**.

Top-level partitioning is good: `core`, `adapters`, `native`, `storage`, `cli`, and `server` are meaningful boundaries. The event-sourced model, replayability, and observability are real strengths.

The main risk is **implementation concentration inside key orchestration hubs**. Largest hotspots reviewed:
- `src/core/registry/flow.rs` (~6321 LOC)
- `src/core/registry/governance.rs` (~4582 LOC)
- `tests/integration.rs` (~4667 LOC)
- `src/core/state.rs` (~1466 LOC)
- `src/native/tool_engine.rs` (~1231 LOC)
- `src/server/routes.rs` (~895 LOC)

Overall assessment:
- **Architecture direction:** strong
- **Public-boundary SOLID:** moderate
- **Internal implementation SOLID:** mixed / weak in hotspots
- **Maintainability risk:** medium, trending high if concentration continues
- **Extendability:** good at subsystem level, weaker inside core hubs

## What Hivemind Is Doing Well
### 1. Clear top-level boundaries
- `core` = domain + orchestration state
- `adapters` = runtime abstraction layer
- `native` = native runtime contracts/harness
- `storage` = persistence
- `cli` / `server` = delivery surfaces

### 2. Strong event-sourced core
`AppState::replay` and the event model create a strong source-of-truth pattern that helps auditability, reproducibility, debugging, and future replay-based validation.

### 3. Good seam abstractions
`EventStore`, `RuntimeAdapter`, and `ModelClient` are healthy architectural seams. They improve substitutability and testing, and they avoid hard-coupling the system to one storage or runtime implementation.

### 4. Robustness mindset is visible in code
The codebase consistently emphasizes structured errors, observability, replayability, verification gates, and safety/scope enforcement.

### 5. CI and docs are solid foundations
CI enforces build/test/clippy, and the architecture docs are clear and aligned with system intent.

## Main Architectural Findings
### 1. `Registry` is still a god-facade
Although split across submodules, `Registry` still centralizes projects, tasks, graphs, flows, governance, runtime selection, verification, worktrees, and merge operations.

Impact:
- **SRP:** too many reasons to change
- **ISP:** callers depend on a very broad surface

### 2. Event replay logic is concentrated in a giant reducer
`src/core/state.rs` contains a large `apply_mut` match across many event types. The event-sourced direction is good; the reducer concentration is not.

Impact:
- harder review and regression isolation
- more accidental coupling between aggregates
- higher merge/conflict pressure

### 3. HTTP routing is concentrated in one large controller-like module
`src/server/routes.rs` effectively acts as a monolithic router/controller.

Impact:
- **SRP:** routing, parsing, validation, and orchestration are packed together
- **OCP:** adding endpoints means editing a central switchboard

### 4. Dependency inversion is only partial
There are good abstractions, but top-level surfaces still construct concrete dependencies directly:
- CLI handlers call `Registry::open()`
- `server::handle_api_request()` also opens `Registry` directly

Server tests partially mitigate this via `handle_api_request_inner(..., &registry)`, but composition is not yet consistently externalized.

### 5. Runtime adapter extension is only partially open/closed
The runtime trait is good, but adapter selection still depends on central string matching and a concrete `SelectedRuntimeAdapter` enum.

Result:
- adding a new adapter still requires editing core dispatch code
- the abstraction exists, but the extension path remains centralized

### 6. Native subsystem has a new monolith risk
`src/native/mod.rs` is conceptually clean, especially the deterministic loop and `ModelClient` trait. But `src/native/tool_engine.rs` is becoming a concentration point for tool contracts, validation, approval policy, sandbox policy, network policy, and execution dispatch.

### 7. Test strategy is strong but structurally overloaded
There is real integration coverage, route coverage, targeted state/adaptor tests, and CI enforcement. But `tests/integration.rs` is itself a monolith, which hurts readability and future maintenance.

### 8. Hivemind does not yet fully apply its own architecture philosophy to itself
Hivemind can model constitutions and dependency rules for managed projects, but I did not find comparable automated architecture-boundary enforcement for Hivemind itself in CI.

That is the largest gap between stated architectural philosophy and internal guardrails.

## SOLID Assessment
- **S — Single Responsibility:** good at module level; weak in `Registry`, `state.rs`, `server/routes.rs`, `native/tool_engine.rs`, and some large handlers.
- **O — Open/Closed:** good in intent; partial in practice. Traits exist, but extensions still hit central dispatch code.
- **L — Liskov Substitution:** generally good at the trait level (`EventStore`, `RuntimeAdapter`, `ModelClient`).
- **I — Interface Segregation:** mixed; external module boundaries are reasonable, but `Registry` is too broad.
- **D — Dependency Inversion:** mixed; abstractions exist, but application composition still reaches directly for concrete defaults too often.

## Priority Recommendations
### Priority 1: shrink orchestration hubs
First targets:
1. Split `Registry` into narrower services/facades (`ProjectService`, `FlowService`, `GovernanceService`, `RuntimeService`, etc.)
2. Split `AppState::apply_mut` into aggregate-specific reducers
3. Split `server/routes.rs` into route groups/resources
4. Split `tests/integration.rs` into scenario-focused files

### Priority 2: introduce a real composition root
Create a small application wiring layer that constructs the registry/store, runtime factory, server dependencies, and CLI command dependencies. This would improve DIP, testing, and replaceability.

### Priority 3: replace central adapter switching with a registry/factory model
Move from enum/string-switch construction toward a runtime factory registry to materially improve OCP.

### Priority 4: enforce architecture boundaries on Hivemind itself
Add CI-backed rules for allowed dependencies between top-level modules, explicit “core must not depend on cli/server” checks, or architecture tests / repo-local constitution-like policy.

### Priority 5: treat `#[allow(clippy::too_many_lines)]` as explicit debt
CI is good, but the local suppressions identify refactoring targets. They should be tracked as temporary debt, not a steady-state design strategy.

## Bottom Line
Hivemind is architecturally promising and already has strong foundations: event sourcing, observability, runtime abstraction, and a clear system philosophy.

The main risk is not bad direction; it is **implementation concentration**. Keep the current top-level architecture, but make the next phase explicitly about **shrinking orchestration hubs and enforcing boundaries on Hivemind itself**.
