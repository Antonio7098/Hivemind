# Architecture Refactor Map + Tracker
Date: 2026-03-06
Source: `ops/reports/architecture-audit-solid-2026-03-06.md`

## Purpose
Execute one bounded refactor cycle that reduces Hivemind's main architectural maintenance risks **without changing product direction**.

This is not a roadmap. It is one ordered execution process with guardrails, exit criteria, and a clear finish line.

## Tracker Summary
Overall status:
- [ ] Not started
- [x] In progress
- [ ] Complete

Working fields:
- Current focus: verify `verification/process/task.rs` is fully decomposed and finish core execution hotspot reduction pass
- Owner: Augment Agent
- Start date: 2026-03-11
- Last updated: 2026-03-18
- Active blockers: None

Execution checklist:
- [x] 1. Establish guardrails first
- [x] 2. Create a real composition root
- [x] 3. Break up `Registry` by capability
- [x] 4. Split event replay by aggregate
- [x] 5. Decompose server routing
- [x] 6. Open runtime extension properly
- [x] 7. Reduce native tool engine concentration
- [x] 8. Split the integration test monolith

## Update Rules
- mark the checklist item only when its exit check is met
- record proof in the progress log with PRs, commits, or file references
- if work is partially done, update `Current focus` rather than checking the box
- if a blocker appears, add it to `Active blockers` before changing scope

## Target Outcome
By the end of this process, Hivemind should have:
- a narrower composition surface
- smaller orchestration hubs
- cleaner dependency direction
- better extension points for runtimes and delivery surfaces
- architecture checks applied to Hivemind itself

## Non-Negotiable Rules
1. Do not rewrite the system.
2. Do not change event semantics unless unavoidable.
3. Do not mix this work with unrelated feature work.
4. Keep every step shippable.
5. Preserve behavior with tests before and after each move.
6. Prefer extraction and delegation before redesign.
7. If a change increases indirection without reducing responsibility, stop.

## Ordered Work
### 1. Establish guardrails first
Moves:
- define allowed dependency directions for `core`, `storage`, `adapters`, `native`, `cli`, and `server`
- add CI-backed checks so `core` cannot depend on `cli` or `server`
- record `too_many_lines` hotspots as explicit refactor debt

Exit check:
- a PR can fail if architecture boundaries are broken
- Tracking note: link the CI rule, architecture test, or policy file here when complete.

### 2. Create a real composition root
Moves:
- extract registry/store/runtime construction from CLI handlers
- extract registry/store/runtime construction from server entrypoints
- centralize default dependency assembly in one small module
- keep existing behavior unchanged

Exit check:
- CLI and server no longer decide concrete wiring inside request/command handlers
- Tracking note: record the composition module and the entrypoints changed.

### 3. Break up `Registry` by capability
Moves:
- extract `ProjectService`
- extract `FlowService`
- extract `GovernanceService`
- extract `RuntimeService`
- move methods behind those services before deciding whether `Registry` stays as a thin facade

Exit check:
- most new code depends on smaller services, not the full `Registry`
- `Registry` loses direct ownership of multiple unrelated responsibilities
- Tracking note: record which services were extracted and which callers moved.

### 4. Split event replay by aggregate
Moves:
- isolate project event application
- isolate task/graph event application
- isolate flow/governance event application
- keep `AppState::replay` as the composition point, not the mutation dumping ground

Exit check:
- `AppState::apply_mut` becomes orchestration over smaller reducers, not a giant match body
- Tracking note: list the reducer modules added and any compatibility wrappers retained.

### 5. Decompose server routing
Moves:
- group endpoints by capability area
- extract shared parsing and validation helpers
- keep one top-level router, but remove business-heavy branching from it

Exit check:
- adding an endpoint mostly means touching one route group, not a central monolith
- Tracking note: record route-group modules and any extracted request/response helpers.

### 6. Open runtime extension properly
Moves:
- separate runtime discovery/selection from runtime behavior
- replace central string/enum dispatch with a registration mechanism where practical
- keep compatibility wrappers if CLI or persisted config expects current names

Exit check:
- adding a runtime requires minimal change outside the runtime registration point
- Tracking note: record the registration mechanism and compatibility path.

### 7. Reduce native tool engine concentration
Moves:
- split tool schema/contract handling from execution dispatch
- split approval and sandbox policy from tool execution
- isolate network policy decisions from command execution code

Exit check:
- `tool_engine` is no longer the sole owner of tool policy plus execution mechanics
- Tracking note: record the new policy, validation, and execution units.

### 8. Split the integration test monolith
Moves:
- separate CLI, server, runtime, storage, and governance scenarios
- keep helpers shared and scenarios isolated
- preserve coverage before deleting old structure

Exit check:
- failures identify capability areas quickly instead of pointing to one giant test file
- Tracking note: record the new test files and removed monolithic sections.

## Execution Discipline
Run the work in order:
- guardrails first prevent drift
- composition root next makes later splits easier
- service extraction before reducer/router changes lowers risk
- runtime and native cleanup come after dependency direction is clearer
- test splitting should happen continuously, then be finalized near the end

For each item:
1. lock expected behavior with targeted tests
2. extract the smallest stable seam
3. route callers through the new seam
4. delete superseded logic only after green tests
5. stop and consolidate before starting the next item

## Success Criteria
This process is complete when:
- [x] Hivemind enforces its own architecture boundaries in CI
- [x] CLI/server composition is centralized
- [x] `Registry` is substantially thinner
- [ ] `AppState` mutation logic is split by aggregate
- [x] server routing is modularized
- [x] runtime extension no longer depends on a broad central switch
- [x] `native/tool_engine.rs` is materially decomposed
- [x] `tests/integration.rs` is no longer a monolith

## What Not To Do
- no large-batch rename-only churn
- no speculative abstraction layers without moving real responsibility first
- no simultaneous redesign of domain, transport, and persistence models
- no feature expansion unless required for compatibility preservation

## Progress Log
- 2026-03-11 — Started bounded architecture refactor cycle. Notes: began with repo guardrails, composition root extraction, and service boundary cleanup.
- 2026-03-11 — Completed items 1-3 and 5-6. Evidence: `.github/workflows/ci.yml`; `scripts/check_architecture.py`; `src/app.rs`; `src/server.rs`; `src/server/routes.rs`; service-oriented `src/core/registry/*` slices; runtime registration/factory extraction in `src/core/registry/runtime/management/support/factory.rs`.
- 2026-03-18 — Completed item 8 and tightened test architecture validation. Evidence: `tests/runtime_scope.rs`; `tests/query_views.rs`; `tests/flow_lifecycle.rs`; `tests/integration_remainder.rs`; `src/native/tests/{agent_loop,budget_compaction,checkpoint_completion,support}.rs`; targeted architecture smoke tests in `.github/workflows/ci.yml`; `cargo test --test runtime_scope`; `cargo test --test query_views`; `cargo test --test flow_lifecycle`; `cargo test --test integration_remainder`; `cargo test native::tests`.
- 2026-03-18 — Reduced native hotspot concentration further while item 7 remains in progress. Evidence: `src/native/turn_items/budget_compaction.rs`; `src/native/prompt_assembly/sections.rs`; `src/native/tool_engine/run_command_tool/policy.rs`; `cargo test native::tests`.
- 2026-03-18 — Completed item 7 by separating tool-engine policy from execution paths. Evidence: `src/native/tool_engine/run_command_tool/policy.rs`; `src/native/tool_engine/policy_eval/{network,approval}.rs`; `src/native/tool_engine/policy_eval.rs`; `cargo test native::tests`.
- 2026-03-18 — Tightened native observability contracts after the tool-engine split. Evidence: `src/native/contracts.rs`; `src/native/mod.rs`; `src/native/agent_loop.rs`; `src/native/adapter/observer.rs`; `cargo test native::tests`.
- 2026-03-18 — Substantially reduced the core tick runtime execution hotspot. Evidence: `src/core/registry/flow/execution/tick/once/runtime/{observations,adapter_lifecycle,filesystem,interactive,environment}.rs`; `src/core/registry/flow/execution/tick/once/runtime.rs`; `cargo test flow_lifecycle`.
- 2026-03-18 — Reduced the core verification task execution hotspot. Evidence: `src/core/registry/flow/verification/process/task/{scope_validation,check_runner}.rs`; `src/core/registry/flow/verification/process/task.rs`; `cargo test flow_lifecycle`.
- 2026-03-18 — Split event replay reducers into narrower aggregate-specific units. Evidence: `src/core/state/apply/attempt/{checkpoints,checks,lifecycle,runtime}.rs`; `src/core/state/apply/flow/{lifecycle,runtime}.rs`; `src/core/state/apply/task.rs`; `src/core/state/apply.rs`.
- 2026-03-18 — Completed Section 3 (Reducers and Derived State) by verifying cross-aggregate coordination is isolated. Evidence: `src/core/state/apply.rs`; `cargo test --lib`.
- 2026-03-18 — Verified EventPayload fragmentation and taxonomy, completing Section 2. Evidence: `src/core/events/payload/fragments/`; `docs/architecture/event-model.md`.
- 2026-03-18 — Completed Section 13 docs/scripts/process alignment goals.

## Final Note
The aim is not to make Hivemind look more abstract. The aim is to make it easier to change, easier to reason about, and harder to accidentally damage.
