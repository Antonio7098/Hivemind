# Hivemind Architectural Audit (SOLID-focused, expanded)
Date: 2026-03-11
Supersedes: initial audit dated 2026-03-06
Scope: `Cargo.toml`, `build.rs`, `src/core`, `src/adapters`, `src/native`, `src/storage`, `src/cli`, `src/server`, `tests`, `docs`, `ops`, `scripts`, `.github`, and `hivemind-test`.

## Executive Summary
Hivemind currently has a **strong architectural direction, good top-level modularity, and improving internal structure**, but it still carries a set of concentrated orchestration hotspots that limit how “fully SOLID” the implementation feels day to day.

The most important update since the original audit is this: **the codebase has already improved materially**.

The earlier giant files that drove the first audit have been split:
- `src/core/state.rs` is now a relatively small facade/types file with `app`, `apply`, `catalog`, and `runtime` submodules.
- `src/server/routes.rs` is now a route dispatcher with route-group submodules instead of a single controller-like blob.
- `src/core/registry/flow.rs`, `src/core/registry/governance.rs`, and `src/core/registry/runtime.rs` are now split facades over deeper folders.

So the present architecture problem is **no longer “the whole system lives in a few monster files.”**
It is now more specific:

1. `Registry` is still a very broad public orchestration facade.
2. runtime construction still depends on centralized enum/string dispatch.
3. CLI/server composition still opens concrete defaults directly.
4. several focused but still-large modules are becoming the new concentration points.
5. the repository has strong architectural intent, but weak automated enforcement of its own boundaries.

Current overall assessment:
- **Architecture direction:** strong
- **Top-level modularity:** strong
- **Subsystem extendability:** good
- **Internal SOLID discipline:** mixed, but improving
- **Maintainability risk:** medium
- **Refactor urgency:** targeted, not wholesale

## Coverage Map
This audit covers every major repository area:
- package/build/release topology
- `src/core`
- `src/adapters`
- `src/native`
- `src/storage`
- `src/cli`
- `src/server`
- `tests`
- `docs`
- `ops`, `scripts`, `.github`, and `hivemind-test`

## Architecture Scorecard by Area
| Area | Current state | Strength | Main risk |
|---|---|---|---|
| package / crate layout | good | simple shipping model | boundaries are conventional, not compile-time hard |
| build / codegen | good | `build.rs` keeps event payload definition modular | codegen is narrow and not used for wider boundary control |
| `src/core` | good | event-sourced model is excellent | `Registry` and some reducers/execution paths remain broad |
| `src/adapters` | good | clear runtime seam via `RuntimeAdapter` | adapter addition still hits central switch logic |
| `src/native` | moderate-good | explicit contracts, deterministic loop, strong observability | tool engine / prompt / turn-management hotspots |
| `src/storage` | strong | clean `EventStore` seam and multiple backends | future query growth could leak store concerns upward |
| `src/cli` | moderate-good | thin entrypoint, feature-split handlers | large handlers + direct `Registry::open()` |
| `src/server` | moderate-good | route groups and injectable inner handler | chat/query/SSE logic still concentrated |
| `tests` | good confidence, weaker structure | broad coverage | very large scenario files and suites |
| `docs` | strong | architecture intent is clearly documented | docs are not yet enforced by repo-local checks |
| `ops` / `scripts` / CI | good foundation | visible architecture/refactor culture | lacks architecture-specific guardrails |

## What Has Improved Since the Initial Audit
The original report correctly identified dangerous implementation concentration, but parts of it are now outdated.

### Structural improvements already completed
- `src/core/state.rs` dropped from a giant reducer file to a facade plus submodules.
- `src/server/routes.rs` dropped to a focused dispatcher with `chat`, `projects`, `tasks`, `graphs`, `flows`, `operations`, `governance`, and `queries` route modules.
- registry domains were split into dedicated subtrees (`flow`, `governance`, `graph`, `runtime`, `tasks`, `worktree`, `context`, `events`).
- event payload definition is now fragmented and generated via `build.rs`, which is a healthy maintainability step.

### What that means architecturally
Hivemind is **not stalled in monolith collapse**. It is already in the next stage of maturity: preserving good macro-boundaries while paying down the remaining hotspots without losing coherence.

## Area-by-Area Assessment

### 1. Package, crate, build, and release topology
`Cargo.toml` defines a **single Rust crate** that exports library and CLI surfaces from one package. That keeps shipping, installation, and discoverability straightforward.

This is a legitimate choice at Hivemind’s current size, but it has an architectural tradeoff: the boundaries between `core`, `adapters`, `native`, `cli`, `server`, and `storage` are mostly **discipline-based module boundaries**, not hard crate/workspace boundaries.

Strengths:
- simple developer ergonomics
- straightforward release pipeline
- low friction for internal refactors

Risks:
- easier accidental cross-layer dependency drift
- harder to enforce “core must not know about delivery/runtime details” at compile time

Positive note: `build.rs` is doing useful work by generating the `EventPayload` enum from payload fragments. That is a good example of using tooling to keep a central abstraction maintainable rather than letting one file balloon.

Release/distribution architecture is also mature: GitHub Actions plus `cargo-dist` support multi-target release artifacts and npm publication.

### 2. `src/core` — the domain center
This remains Hivemind’s strongest architectural area in terms of intent.

The top-level `core` map is coherent and meaningful:
- `events`
- `state`
- `graph`
- `flow`
- `scope`
- `enforcement`
- `verification`
- `diff`
- `registry`
- `scheduler`
- `worktree`
- `graph_query`
- `skill_registry`
- `runtime_event_projection`
- `context_window`

That is a strong domain vocabulary. The repository is not just “code organized by technical layer”; it is organized around recognizable concepts.

#### 2.1 `core/events`
This is a strength.

Why:
- event sourcing is the real source of truth for the system
- payloads are explicit and typed
- replay is a first-class architectural assumption
- runtime/model/event metadata are not treated as incidental logging

The generated payload assembly via `build.rs` is particularly healthy because it keeps the event taxonomy centralized **without** requiring a single unmaintainable payload file.

#### 2.2 `core/state`
This area has improved significantly.

`src/core/state.rs` now acts as a public state/types facade, while mutation/replay logic lives under `state/apply` and projection/catalog logic lives under `state/catalog`.

That is a clear architectural win.

Remaining risk:
- some aggregate reducers are still large enough to need `#[allow(clippy::too_many_lines)]`
- replay/application logic is still concentrated in a few aggregate-specific modules rather than deeply decomposed reducers

So the state architecture is now **conceptually solid and structurally better**, but it still has tactical debt in the reducer implementation layer.

#### 2.3 `core/registry`
This is still the single most important architectural pressure point.

Internally, the registry has been modularized well:
- `context`
- `events`
- `flow`
- `governance`
- `graph`
- `runtime`
- `tasks`
- `worktree`
- `types`
- `shared_types`

But externally, `Registry` is still a broad god-facade. It remains the place that callers reach for when they want almost anything operational:
- project operations
- task operations
- graph operations
- flow operations
- runtime configuration
- runtime health
- governance actions
- worktree actions
- merge-related actions

Architectural impact:
- **SRP:** too many reasons to change
- **ISP:** callers depend on a very wide capability surface
- **DIP:** application code depends on the default registry object rather than narrow service contracts

This is not a conceptual failure. It is the normal result of starting from a coherent orchestration hub and then letting it remain the default entrypoint too long.

#### 2.4 `core/graph`, `core/flow`, `core/scheduler`, `core/verification`
These areas are mostly healthy and aligned with the system model described in the docs.

The important nuance is that the **domain model is better than some of the execution-path implementations**.

Examples of focused-but-still-hot files include:
- `src/core/registry/flow/execution/tick/once/runtime.rs` (~620 LOC)
- `src/core/registry/flow/verification/process/task.rs` (~385 LOC)
- `src/core/registry/runtime/management/projection.rs` (~1646 LOC)

So the abstractions are good, but the “how work actually happens” path still has some concentration.

#### 2.5 `core/scope`, `core/enforcement`, `core/worktree`, `core/graph_query`, `core/context_window`, `core/skill_registry`, `core/runtime_event_projection`
These are generally **better factored than the orchestration hubs**.

They read like properly separated support/domain subsystems rather than catch-all utilities. That is an architectural positive because it means the codebase’s problem is not generalized chaos; it is a targeted handful of hubs that still need thinning.

### 3. `src/adapters` — runtime abstraction layer
This subsystem is architecturally sound in intent.

Strengths:
- `RuntimeAdapter` is a clear seam
- `AdapterConfig`, `ExecutionInput`, `ExecutionReport`, and telemetry types are explicit
- adapter wrappers for `codex`, `claude_code`, and `kilo` sensibly reuse the `OpenCode` implementation rather than duplicating process-management logic
- runtime environment and prompt formatting are isolated in their own modules

This is good SOLID work:
- strong **LSP** at the trait seam
- decent **SRP** in the support modules
- good reuse through composition

Main weakness:
- extension is still only partially open/closed
- runtime selection is still ultimately centralized in registry runtime management through `SelectedRuntimeAdapter` and string matching on `adapter_name`

That means adding a new adapter still requires editing core dispatch code, not just registering a new implementation.

Recommendation:
- keep the current traits and adapter implementations
- replace central selection with a runtime-factory registry keyed by descriptor metadata/capabilities

### 4. `src/native` — deterministic native runtime
This subsystem is one of the most interesting and promising parts of the repository.

Architectural strengths:
- explicit `ModelClient` contract
- explicit `AgentLoopObserver` hooks
- deterministic state machine (`init -> think -> act -> done`)
- runtime hardening and startup hardening are isolated concerns
- OpenRouter provider code is separated from the generic native model contract
- native adapter and trace types make observability a first-class design element

The native architecture is conceptually strong because it is **contract-driven, observable, and testable**.

However, this is also where the next monolith risk is accumulating.

Current hotspots include:
- `src/native/agent_loop.rs` (~917 LOC)
- `src/native/turn_items.rs` (~709 LOC)
- `src/native/prompt_assembly.rs` (~443 LOC, with `too_many_lines` suppression)
- `src/native/adapter/runtime.rs` (~612 LOC)
- `src/native/tool_engine/engine.rs` and adjacent policy/tool modules

The `tool_engine` subtree is at least structurally decomposed (`action`, `filesystem_tools`, `git_tools`, `graph_query_tool`, `policies`, `policy_eval`, `exec_sessions`, `run_command_tool`), which is good.

But the architectural risk remains:
- policy evaluation
- tool execution
- approval logic
- session management
- transport-specific translation

are still close enough together that this area could become the new “god subsystem” if growth continues unchecked.

### 5. `src/storage` — persistence and event log
This is currently the cleanest subsystem in the repository.

Why:
- `EventStore` is a narrow, understandable abstraction
- SQLite, JSONL, and memory backends are separated cleanly
- filter/helpers are isolated support modules
- the persistence model aligns directly with the event-sourced architecture

This is strong from a SOLID perspective:
- **SRP:** very good
- **LSP:** very good
- **DIP:** strong because callers can depend on the trait

I would treat `storage` as a model subsystem for the rest of the codebase: small public surface, explicit contracts, multiple implementations, and limited conceptual sprawl.

### 6. `src/cli` — command surface
The CLI is in decent shape overall.

Strengths:
- `src/main.rs` is thin and mostly orchestration-free
- command definitions are split from handlers
- output formatting is centralized
- feature areas are reflected in handler structure (`project`, `task`, `graph`, `flow`, `runtime`, `merge`, `verify`, `attempt`, `events`, `worktree`, etc.)

Main issues:
- several handlers are large and rely on direct `Registry::open()`
- the CLI still acts partly as a place where composition happens by reaching for concrete defaults
- some subareas such as global/event/reporting handlers are getting heavy enough to need explicit `too_many_lines` suppressions

Architecturally, the CLI is no longer a monolithic entrypoint problem. It is now an **application-service boundary problem**: handlers should ideally depend on a narrower injected service layer instead of directly constructing/opening the central registry.

### 7. `src/server` — HTTP/UI delivery surface
This area is improved relative to the original audit.

Good changes already present:
- `src/server/routes.rs` is now only ~180 LOC and dispatches to route groups
- request handling can be tested through `handle_api_request_inner(..., &registry)`
- API types are split into dedicated modules
- event UI shaping and query-view logic have named homes

That is real architectural progress.

Remaining issues:
- `src/server.rs` still mixes transport loop, request parsing, response generation, SSE wiring, and concrete registry opening
- `src/server/routes/chat.rs` (~946 LOC) is now the largest server-side hotspot
- `src/server/query_views.rs` (~314 LOC) is a growing read-model concentration point
- SSE endpoints still create/open concrete registry instances directly in the server loop

Interpretation:
the server is no longer “badly modularized,” but it is still **too delivery-centric and not service-layer-centric**.

The next step is not more route splitting alone. The next step is:
- query services for read models
- chat/session application service(s)
- a transport shell that does little more than decode/encode HTTP and SSE

### 8. `tests` — quality strength, structure weakness
The repository has substantial test investment, and that is a major positive.

Evidence:
- focused unit tests across `core`, `adapters`, `native`, `storage`, `server`, and CLI command areas
- `server/tests.rs` exercises the HTTP surface
- adapter/native/storage modules have dedicated tests
- CI runs build, tests, formatting, and clippy

But structurally, tests are overloaded:
- `tests/integration.rs` is ~4667 LOC
- `src/native/tool_engine/tests.rs` is ~1565 LOC
- `src/native/tests.rs` is ~1324 LOC
- `src/server/tests.rs` is ~588 LOC

That means confidence is reasonably high, but maintainability of the test suite is lower than it should be.

`hivemind-test/` is also valuable: it contains real-runtime and manual shell-based validation scripts. That is good operational coverage, but it sits outside the normal CI path and should be thought of as an auxiliary validation layer rather than the primary test architecture.

### 9. `docs` — architecture intent is unusually strong
The docs are one of the codebase’s best assets.

There is a clear structure:
- `docs/architecture/` for system model and invariants
- `docs/design/` for operational semantics
- `docs/overview/` for product framing and onboarding

The architecture docs are aligned with the code’s real design themes:
- determinism
- replayability
- explicit scope
- runtime replaceability
- event-sourced truth

This matters because it means the system has a coherent internal theory, not just code.

Gap:
- the documentation is stronger than the enforcement
- CI validates build/test/lint quality, but not architectural dependency rules or hotspot budgets

### 10. `ops`, `scripts`, CI, release, and repository process
This area shows a healthy engineering culture.

Positive signals:
- `ops/reports/` contains a large history of sprint and architecture reports
- `ops/non-phase-work/architecture-refactor-map-2026-03-06.md` shows explicit design debt tracking
- `scripts/rust_fn_dependency_graph.py` exists to reason about internal function dependency structure
- `.github/workflows/ci.yml` enforces build/test/lint
- `.github/workflows/release.yml` provides a mature release pipeline

This is all good.

But the missing piece is still important: **the repository does not yet enforce its own intended architecture automatically**.

Examples of missing guardrails:
- no CI-backed dependency layering checks
- no architecture tests asserting allowed module relationships
- no explicit budget or ratchet for `#[allow(clippy::too_many_lines)]`
- no hotspot-size reporting in CI
- `CODEOWNERS` is effectively single-owner/global rather than subsystem-based

## Current Hotspot Inventory
The largest production-code hotspots I found are now more targeted than in the original audit.

### Production hotspots
- `src/core/registry/runtime/management/projection.rs` (~1646 LOC)
- `src/server/routes/chat.rs` (~946 LOC)
- `src/native/agent_loop.rs` (~917 LOC)
- `src/cli/handlers/events/native_summary.rs` (~802 LOC)
- `src/native/turn_items.rs` (~709 LOC)
- `src/adapters/opencode/runtime_impl.rs` (~679 LOC)
- `src/core/registry/flow/execution/tick/once/runtime.rs` (~620 LOC)
- `src/native/adapter/runtime.rs` (~612 LOC)
- `src/server.rs` (~487 LOC)

### Test hotspots
- `tests/integration.rs` (~4667 LOC)
- `src/native/tool_engine/tests.rs` (~1565 LOC)
- `src/native/tests.rs` (~1324 LOC)
- `src/server/tests.rs` (~588 LOC)
- `src/storage/event_store/tests.rs` (~463 LOC)

### Architectural interpretation of the hotspot list
This is actually a better shape than the original audit.

Before, the biggest risk was a few all-purpose top-level god files.
Now, the main risk is narrower but still important: a handful of specialized areas are carrying too much operational detail.

That is a healthier refactor situation because it is easier to fix surgically.

## Evidence of Partial Dependency Inversion
The codebase has good abstractions, but application composition is still only partial.

Examples found directly in the code:
- `src/server.rs` opens `Registry` directly in `handle_api_request()` and in SSE request handling
- CLI handlers such as `src/cli/handlers/common.rs`, `flow.rs`, `graph.rs`, `runtime.rs`, and `worktree.rs` call `Registry::open()` directly

This is the key DIP gap in the current architecture.

The abstractions are there; the composition root is not.

## Evidence of Partial Open/Closed Compliance
The adapter layer is well abstracted, but runtime extension remains centralized.

Concrete evidence:
- `src/core/registry/shared_types/runtime.rs` defines `SelectedRuntimeAdapter` as a concrete enum
- `src/core/registry/runtime/management/support/health.rs` builds adapters through a central `match runtime.adapter_name.as_str()` switch

So the code is open to variation in theory, but not yet fully open to extension in practice.

## SOLID Assessment
### S — Single Responsibility
Good at the subsystem/module level.
Mixed at the large-file/application-service level.

Strong examples:
- `storage::event_store`
- adapter support modules
- many `core` support domains (`scope`, `enforcement`, `worktree`, `graph_query`)

Weak examples:
- `Registry`
- runtime projection/selection paths
- server chat route handling
- some native runtime/tool-engine files
- very large scenario tests

### O — Open/Closed
Good in intention, partial in implementation.

Strong:
- traits like `EventStore`, `RuntimeAdapter`, `ModelClient`
- route-group decomposition
- fragmented event payload generation

Weak:
- centralized runtime adapter construction
- broad public facade growth patterns

### L — Liskov Substitution
Generally good.

This is one of the healthier SOLID letters in the codebase. The trait seams are real and useful, especially in storage/runtime/model integration.

### I — Interface Segregation
Mixed.

Subsystems are reasonably segregated, but the main public orchestration entrypoint is not. `Registry` is the standout interface-segregation problem.

### D — Dependency Inversion
Mixed, improving slowly.

The codebase has the right abstractions, but CLI/server still reach directly for concrete defaults. That keeps the architecture more coupled than it needs to be.

## What to Preserve
Do **not** refactor away the things that are already working architecturally:
- event-sourced state derivation
- explicit error taxonomy
- runtime abstraction via traits
- native observability and deterministic loop model
- build-time event payload fragmentation
- route-group and state-submodule modularization already achieved
- strong written architecture docs

The next phase should be **targeted thinning and boundary hardening**, not a wholesale redesign.

## Target Architecture Direction
The most sensible target from here is:

1. keep the current top-level subsystem map
2. add a real application composition layer
3. narrow `Registry` behind smaller service interfaces
4. move runtime creation to a registry/factory plugin model
5. split the remaining specialized hotspots into service/query/reducer units
6. add architecture enforcement in CI

In other words: keep the macro-architecture, strengthen the micro-architecture.

## Priority Recommendations
### Priority 1 — introduce a real composition root
Create a small `app` or `bootstrap` layer responsible for wiring:
- event store
- registry/services
- runtime factories
- server dependencies
- CLI dependencies

This is the single highest-leverage improvement for DIP and testability.

### Priority 2 — split `Registry` into narrower service contracts
Suggested direction:
- `ProjectService`
- `TaskService`
- `GraphService`
- `FlowService`
- `RuntimeService`
- `GovernanceService`
- `WorktreeService`

`Registry` can remain as an internal facade/composition object for a while, but external callers should stop depending on the whole surface.

### Priority 3 — replace adapter switching with a factory registry
Move from:
- `SelectedRuntimeAdapter` enum + string matching

Toward:
- runtime descriptors
- factory trait objects or registered constructors
- capability metadata per adapter

That will materially improve OCP and reduce the amount of core code touched when adding runtimes.

### Priority 4 — split the new hotspots before they become the next monoliths
First production targets:
1. `src/core/registry/runtime/management/projection.rs`
2. `src/server/routes/chat.rs`
3. `src/native/agent_loop.rs`
4. `src/native/tool_engine/engine.rs` and adjacent policy/tool execution pieces
5. `src/cli/handlers/events/native_summary.rs`

First test targets:
1. `tests/integration.rs`
2. `src/native/tool_engine/tests.rs`
3. `src/native/tests.rs`
4. `src/server/tests.rs`

### Priority 5 — add architecture guardrails to CI
Examples:
- forbid dependencies from `core` to `cli`/`server`
- fail CI on new `too_many_lines` suppressions without justification
- generate hotspot/size reports and ratchet them down over time
- add architecture tests for allowed module relationships

This is where Hivemind can start applying its own governance philosophy to itself.

### Priority 6 — consider crate/workspace splitting later, not first
Do **not** start with a workspace split.

Do it only after service boundaries are cleaner. Otherwise the repository will just move today’s broad facades across crate boundaries and gain ceremony without real modularity.

Candidate future crates once seams stabilize:
- `hivemind-core-domain`
- `hivemind-storage`
- `hivemind-runtime-adapters`
- `hivemind-native-runtime`
- `hivemind-interfaces` (CLI/server)

## Bottom Line
Hivemind is in **good architectural shape overall**.

The codebase already has the hard parts that many systems never achieve:
- a coherent domain model
- event-sourced truth
- explicit safety/scope concepts
- strong observability
- replaceable runtime seams
- solid docs and operational discipline

The remaining issues are real, but they are now mostly **second-stage architecture problems**:
- public facades that are too broad
- extension points that are still centrally wired
- delivery layers that still self-compose concrete dependencies
- a handful of specialized hotspots that need one more decomposition pass

So the correct conclusion is not “the architecture is weak.”
The correct conclusion is:

**Hivemind has a strong architecture that is now ready for boundary hardening, service extraction, and automated self-governance.**
