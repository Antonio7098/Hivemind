# Architecture Checklist
Date: 2026-03-11
Source: `ops/reports/architecture-audit-solid-2026-03-06.md`

## Purpose
Turn the expanded architecture audit into an execution checklist that covers the entire repository.

Use this as a working tracker for improving SOLID qualities, modularity, extensibility, dependency direction, and long-term maintainability without changing product direction.

## Status
- [ ] Not started
- [x] In progress
- [ ] Complete

Working fields:
- Current focus: Sections 1 and 12 plus the remaining deeper core/native follow-up hotspots
- Owner: Augment Agent
- Start date: 2026-03-11
- Last updated: 2026-03-11
- Active blockers: None

## How To Use This Checklist
- check an item only when the exit condition is actually true
- prefer small, behavior-preserving refactors over large redesigns
- attach proof in PRs/commits/file references as work completes
- if a section is partially done, update `Current focus` rather than checking the whole section

## Global Success Criteria
- [x] Hivemind has an explicit composition root
- [ ] `Registry` is no longer the default dependency for most callers
- [x] adding a runtime does not require editing a broad central switch
- [ ] core reducers/projections are decomposed enough to shrink hotspot risk
- [x] CLI and server are transport shells over narrower services
- [x] architecture guardrails exist in CI
- [ ] test structure reflects subsystem boundaries
- [ ] docs, tooling, and process support the architecture rather than lag behind it

## 1. Repo-Level Guardrails, Packaging, and Build
Goal: make architecture visible and enforceable at repo level.

- [x] define allowed dependency directions between `core`, `storage`, `adapters`, `native`, `cli`, and `server`
- [x] add CI-backed architecture checks for forbidden dependencies
- [x] track file-size / hotspot budgets for known concentration points
- [ ] treat new `#[allow(clippy::too_many_lines)]` uses as explicit debt
- [ ] decide whether single-crate layout remains acceptable for the next refactor cycle
- [ ] document criteria for a future workspace split without doing it prematurely

Exit condition:
- architecture boundary drift can be detected automatically in CI

## 2. `src/core/events` and Event Model
Goal: preserve event-sourced truth while keeping event definitions maintainable.

- [ ] keep `EventPayload` fragmentation healthy and avoid re-centralizing payload growth
- [ ] review event taxonomy for accidental overlap or duplicated concepts
- [ ] verify event naming stays consistent across CLI/server/native/runtime surfaces
- [ ] ensure event docs remain aligned with implementation and generated payload assembly

Exit condition:
- event definitions remain explicit, modular, and aligned with architecture docs

## 3. `src/core/state` Reducers and Derived State
Goal: keep replay/state derivation modular instead of sliding back into monolithic reducers.

- [ ] audit `state/apply` modules for remaining oversized reducers
- [ ] split large aggregate-specific apply logic into narrower reducer units where needed
- [ ] keep `AppState::replay` as orchestration over reducers, not a dumping ground
- [ ] isolate cross-aggregate coordination logic from aggregate mutation logic
- [ ] ensure state tests remain organized by aggregate or reducer responsibility

Exit condition:
- state replay is composed from smaller reducers with clear ownership boundaries

## 4. `src/core/registry` Public Surface and Services
Goal: reduce `Registry` from a broad god-facade into narrower service-oriented seams.

- [x] identify the minimum stable capability slices inside `Registry`
- [x] extract `ProjectService`
- [x] extract `TaskService`
- [x] extract `GraphService`
- [x] extract `FlowService`
- [x] extract `RuntimeService`
- [x] extract `GovernanceService`
- [x] extract `WorktreeService`
- [x] move new callers to smaller services rather than the full `Registry`
- [ ] decide whether `Registry` remains only as a thin composition facade

Exit condition:
- most new or modified code depends on smaller service surfaces, not the full registry

## 5. `src/core/flow`, `scheduler`, `verification`, and Runtime Projection Paths
Goal: shrink the specialized orchestration hotspots that remain inside core execution paths.

- [x] break down `src/core/registry/runtime/management/projection.rs`
- [ ] review `src/core/registry/flow/execution/tick/once/runtime.rs` for extractable phases
- [ ] review `src/core/registry/flow/verification/process/task.rs` for smaller processing units
- [x] separate projection assembly from orchestration decisions where practical
- [x] preserve replayability and observability while thinning execution-path files

Exit condition:
- core execution/projection hotspots are reduced without losing determinism or observability

## 6. `src/core/scope`, `enforcement`, `worktree`, `graph_query`, `context_window`, `skill_registry`
Goal: preserve and tighten the already healthier support/domain subsystems.

- [ ] confirm dependency direction stays inward toward core concepts rather than outward to delivery layers
- [ ] avoid moving unrelated orchestration logic into these support modules
- [ ] add architecture tests for these subsystems if they become dependency magnets
- [ ] keep tests and docs scoped to each subsystem's real responsibility

Exit condition:
- these modules remain focused support/domain units rather than future catch-all buckets

## 7. `src/adapters` Runtime Extension Model
Goal: make runtime extension truly open/closed in practice.

- [x] replace centralized string/enum adapter construction with a registration/factory model
- [ ] define runtime descriptor metadata and capabilities in one extensible place
- [ ] preserve compatibility for persisted/runtime config names
- [ ] keep `RuntimeAdapter` trait stable unless a real design gap is proven
- [ ] ensure OpenCode-derived adapters share implementation without leaking policy upward
- [ ] add tests for runtime registration, discovery, and compatibility behavior

Exit condition:
- adding a runtime mostly means implementing and registering it, not editing broad dispatch code

## 8. `src/native` Deterministic Runtime and Tool Engine
Goal: keep the native subsystem powerful without letting it become the next monolith.

- [x] split `agent_loop` into clearer phases if turn preparation / transition / result handling remain entangled
- [ ] reduce concentration in `turn_items` and `prompt_assembly`
- [x] separate tool contract/schema handling from tool execution mechanics
- [ ] separate approval/sandbox/network policy from command execution code
- [x] reduce concentration in `native/adapter/runtime.rs`
- [ ] keep `ModelClient`, `AgentLoopObserver`, and observability contracts explicit
- [ ] ensure native tests remain deterministic and grouped by subsystem responsibility

Exit condition:
- native runtime responsibilities are distributed across smaller focused units rather than a few heavy files

## 9. `src/storage` Event Store and Persistence
Goal: preserve the cleanest subsystem and prevent persistence concerns from leaking upward.

- [ ] keep `EventStore` narrow and stable
- [ ] ensure new query requirements do not force callers to depend on backend details
- [ ] keep SQLite/JSONL/memory implementations behaviorally substitutable
- [ ] maintain targeted storage tests per backend and shared contract behavior

Exit condition:
- storage remains a model subsystem with a small public surface and strong substitutability

## 10. `src/cli` Command and Handler Architecture
Goal: make CLI handlers depend on services, not concrete default wiring.

- [x] extract registry/store/runtime construction out of CLI handlers
- [x] route handlers through a composition root or application-service layer
- [ ] reduce size of large handler hotspots
- [ ] keep output formatting centralized and separate from business logic
- [ ] ensure commands/handlers reflect capability boundaries cleanly
- [ ] split oversized CLI test scenarios by feature area where needed

Exit condition:
- CLI command handlers are thin adapters over smaller application services

## 11. `src/server` HTTP, Query, Chat, and SSE Surfaces
Goal: make server code a delivery shell over services and read models.

- [x] extract registry/store/runtime construction from server entrypoints and SSE paths
- [x] reduce concentration in `src/server/routes/chat.rs`
- [x] reduce concentration in `src/server.rs`
- [ ] separate HTTP parsing/encoding concerns from orchestration concerns
- [x] separate query-view assembly from transport concerns
- [x] keep route groups capability-oriented and locally extensible
- [x] extend server tests around service seams as route internals are refactored

Exit condition:
- server request handling mostly decodes requests, calls services, and encodes responses

## 12. `tests` and Validation Architecture
Goal: keep coverage high while making failures easier to localize and maintain.

- [ ] split `tests/integration.rs` by capability area
- [ ] split oversized `native` test modules where responsibility boundaries are clear
- [x] split oversized `server` test modules where route groups already exist
- [x] preserve helper reuse without rebuilding a new giant shared harness blob
- [ ] ensure CI still runs the smallest reliable test matrix for fast feedback
- [ ] decide which `hivemind-test/` scripts should remain manual versus become automated smoke coverage

Exit condition:
- test failures point to subsystem areas quickly rather than one monolithic scenario file

## 13. `docs`, `ops`, `scripts`, and Repository Process
Goal: keep architecture documentation and operational tooling aligned with implementation reality.

- [x] update architecture docs when service boundaries or dependency rules change
- [x] keep audit, checklist, and refactor map in sync as work lands
- [ ] extend `scripts/rust_fn_dependency_graph.py` or add tooling if it materially helps refactors
- [ ] decide whether subsystem-specific CODEOWNERS or review rules would improve architectural stewardship
- [ ] keep sprint/process reporting tied to architectural outcomes, not only code churn

Exit condition:
- architecture intent, process, and tooling reinforce the codebase structure instead of trailing behind it

## Recommended Execution Order
- [x] 1. repo guardrails and measurement
- [x] 2. composition root
- [x] 3. `Registry` service extraction
- [x] 4. runtime registration/factory model
- [ ] 5. core execution/projection hotspot reduction
- [ ] 6. native hotspot reduction
- [x] 7. CLI/service boundary cleanup
- [x] 8. server/service boundary cleanup
- [ ] 9. test suite restructuring
- [ ] 10. docs/process/tooling alignment

## Progress Log
- 2026-03-11 — Started execution on branch `architecture-checklist-2026-03-11`. Notes: beginning with repo guardrails, measurement, and composition root work.
- 2026-03-11 — Completed repo guardrails and first-stage composition root. Evidence: `src/app.rs`, CI architecture guardrails in `.github/workflows/ci.yml`, `scripts/check_architecture.py`, direct `Registry::open()` removed from active delivery code.
- 2026-03-11 — Completed runtime factory extraction. Evidence: `src/core/registry/runtime/management/support/factory.rs` now owns runtime adapter construction and `health.rs` no longer carries the central adapter switch.
- 2026-03-11 — Reduced delivery hotspots and validated behavior. Evidence: `src/server.rs` slimmed by moving SSE/response transport into `src/server/transport.rs`; `cargo test --all-features --lib`; `hivemind-test/test_runtime_projection.sh`; `hivemind-test/test_worktree.sh`.
- 2026-03-11 — Began splitting the integration test monolith. Evidence: shared helpers moved to `tests/support.rs`; worktree/flow smoke tests moved to `tests/worktree_flow.rs`; `cargo test --all-features --test integration --test worktree_flow`.
- 2026-03-11 — Reduced native adapter concentration. Evidence: progress observation extracted to `src/native/adapter/observer.rs`; `src/native/adapter/runtime.rs` reduced to 434 lines; `cargo test --all-features --lib`.
- 2026-03-11 — Completed delivery-layer service routing across CLI and server. Evidence: `src/app.rs`; `src/server.rs`; `src/server/routes.rs`; route modules now depend on app services instead of raw `Registry`; `python3 scripts/check_architecture.py`; targeted `cargo test` route checks.
- 2026-03-11 — Reduced core/native hotspots with submodule extraction. Evidence: `src/core/registry/runtime/management/projection/approval.rs`; `src/native/agent_loop/{directive_repair,checkpoint_support,budget_support}.rs`; `src/native/tool_engine/engine/{contracts,dispatch}.rs`; targeted `cargo test` for checkpoint and network policy paths.
- 2026-03-11 — Split server route and test hotspots. Evidence: `src/server/routes/chat/{scope,execution,view}.rs`; `src/server/tests/{chat,runtime_stream}.rs`; `cargo test api_chat_sessions_create_send_and_inspect_round_trip`; `cargo test api_runtime_stream_supports_detail_levels`.
- 2026-03-11 — Added build-script compatibility fragment to unblock validation on this branch. Evidence: `src/core/events/payload/fragments/workflow_execution.rs`; targeted `cargo test server::tests::api_version_ok`.
- YYYY-MM-DD — Completed section N. Evidence:
- YYYY-MM-DD — Blocker found. Impact / decision:

## Final Note
The target is not “more abstraction.”

The target is a codebase that is easier to change, easier to extend, easier to test, and harder to accidentally couple in the wrong direction.