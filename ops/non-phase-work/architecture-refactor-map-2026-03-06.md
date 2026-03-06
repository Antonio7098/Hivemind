# Architecture Refactor Map + Tracker
Date: 2026-03-06
Source: `ops/reports/architecture-audit-solid-2026-03-06.md`

## Purpose
Execute one bounded refactor cycle that reduces Hivemind's main architectural maintenance risks **without changing product direction**.

This is not a roadmap. It is one ordered execution process with guardrails, exit criteria, and a clear finish line.

## Tracker Summary
Overall status:
- [ ] Not started
- [ ] In progress
- [ ] Complete

Working fields:
- Current focus:
- Owner:
- Start date:
- Last updated:
- Active blockers:

Execution checklist:
- [ ] 1. Establish guardrails first
- [ ] 2. Create a real composition root
- [ ] 3. Break up `Registry` by capability
- [ ] 4. Split event replay by aggregate
- [ ] 5. Decompose server routing
- [ ] 6. Open runtime extension properly
- [ ] 7. Reduce native tool engine concentration
- [ ] 8. Split the integration test monolith

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
- [ ] Hivemind enforces its own architecture boundaries in CI
- [ ] CLI/server composition is centralized
- [ ] `Registry` is substantially thinner
- [ ] `AppState` mutation logic is split by aggregate
- [ ] server routing is modularized
- [ ] runtime extension no longer depends on a broad central switch
- [ ] `native/tool_engine.rs` is materially decomposed
- [ ] `tests/integration.rs` is no longer a monolith

## What Not To Do
- no large-batch rename-only churn
- no speculative abstraction layers without moving real responsibility first
- no simultaneous redesign of domain, transport, and persistence models
- no feature expansion unless required for compatibility preservation

## Progress Log
- YYYY-MM-DD — Started item N. Notes:
- YYYY-MM-DD — Completed item N. Evidence:
- YYYY-MM-DD — Blocker found. Impact / decision:

## Final Note
The aim is not to make Hivemind look more abstract. The aim is to make it easier to change, easier to reason about, and harder to accidentally damage.
