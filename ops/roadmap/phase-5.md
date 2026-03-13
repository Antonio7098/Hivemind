# Phase 5: Workflow Engine, Recursive Orchestration, And Workflow Context

**Goal:** Replace `TaskFlow` as the primary execution model with a first-class workflow engine that supports flat and nested workflows, deterministic workflow context, append-only parallel outputs, and a workflow-native CLI/API/runtime surface while preserving Hivemind's event-authority model.

**Input studies and source artifacts:**

- `../ideaspace/workflow-overhaul-implementation-plan.md`
- `docs/architecture/taskflow.md`
- `docs/architecture/event-model.md`
- `src/core/flow.rs`
- `src/core/registry/context.rs`
- `src/core/registry/context/support.rs`
- `src/core/registry/flow/`

> **Principles 1, 2, 3, 5, 8, 9, 10, 11, 15:** Observability as truth, fail loud, reliability over cleverness, explicit FSM/domain boundaries, absolute attribution, mandatory checks, preserve failures, foundations first, no hidden magic.

**Scope note:** Phase 5 is a forward migration of the execution architecture, not a historical data migration. We do **not** need in-place conversion of existing `TaskFlow`, task, attempt, merge, or worktree state. Legacy flow/task history may remain readable during transition, but new workflow functionality should be built without requiring migration of already-existing runs.

**Workflow context rule:** Parallel branches must never race on shared mutable keys. Parallel outputs land in an **append-only output bag** with explicit producer lineage, event sequence, and schema metadata. Downstream steps consume bag contents through deterministic selectors/reducers or explicit join steps; no implicit last-writer-wins behavior is allowed.

**Manual validation rule:** Every sprint in this phase must include manual smoke testing in `@hivemind-test` using a real project/repository fixture. At least one smoke path per sprint must exercise the new workflow surface end-to-end via CLI (and API where applicable), not just unit/integration tests.

**Out of scope for Phase 5:** cron/schedules, broad trigger automation, arbitrary shared mutable workflow memory, and historical run migration tooling.

---

## Sprint 64: Workflow Domain Foundation And Event Model

**Goal:** Introduce the core workflow domain, event family, projections, and lineage model without yet replacing the execution substrate.

### 64.1 Workflow domain model
- [x] Add first-class workflow domain types for:
  - [x] `WorkflowDefinition`
  - [x] `WorkflowRun`
  - [x] `WorkflowStepDefinition`
  - [x] `WorkflowStepRun`
  - [x] workflow/step status enums and transition guards
- [x] Keep step kinds minimal in the first cut: `Task`, `Workflow`, `Conditional`, `Wait`, `Join`
- [x] Separate workflow structure/template terminology from current governance prompt/document template terminology to avoid semantic overload

### 64.2 Event and correlation contracts
- [x] Add workflow event families for create/update/start/pause/resume/abort/complete and step lifecycle transitions
- [x] Extend correlation/lineage contracts to represent workflow-native identity, including at minimum:
  - [x] `workflow_run_id`
  - [x] `root_workflow_run_id`
  - [x] `parent_workflow_run_id`
  - [x] `step_id`
  - [x] `step_run_id`
- [x] Preserve event filtering and replay semantics while adding workflow-aware queryability

### 64.3 Projection and registry foundation
- [x] Add workflow projections to `AppState` (or a clearly bounded adjacent state module) without entangling them with legacy `TaskFlow` projection code
- [x] Add registry methods for workflow definition/run CRUD and inspect/list/status flows
- [x] Keep legacy `flow/*` surfaces operational during construction, but isolate new workflow logic behind new workflow-native registry entry points

### 64.4 Automated validation and test coverage
- [x] Add unit tests for workflow and step transition guards
- [x] Add serialization/deserialization roundtrips for all new workflow event payloads
- [x] Add replay tests proving workflow state is fully reconstructable from events
- [x] Add event-filter tests covering workflow lineage fields and mixed legacy/new event streams

### 64.5 Documentation
- [x] Add/refresh architecture docs describing the new workflow domain and its relationship to legacy `TaskFlow`
- [x] Add design docs for workflow event taxonomy, identity, and projection rules
- [x] Document explicit non-goals for this sprint (no execution cutover yet, no nested execution yet)

### 64.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 64 manual checklist under `@hivemind-test`
- [x] Smoke test workflow definition/run CRUD via CLI against a real repository fixture
- [x] Validate workflow inspect/list/status output and event stream visibility for the new workflow event family
- [x] Publish Sprint 64 manual test report artifact in `@hivemind-test`

### 64.7 Exit Criteria
- [x] Workflow domain types and event contracts exist and replay deterministically
- [x] Workflow runs are inspectable without relying on legacy flow projections
- [x] Workflow lineage is queryable from events without inference
- [x] Automated and manual smoke validation are completed and documented

---

## Sprint 65: Flat Workflow Execution Bridge On Top Of Existing Attempt Machinery

**Goal:** Execute flat workflows using current attempt/worktree/verification infrastructure before introducing nesting or advanced control flow.

### 65.1 Flat workflow scheduler
- [x] Implement workflow scheduling for dependency-driven execution of `Task` steps only
- [x] Preserve deterministic release semantics already proven in current `TaskFlow` scheduling
- [x] Explicitly model ready/running/verifying/retry/success/failure states for workflow step runs

### 65.2 Leaf execution bridge
- [x] Reuse existing attempt launch, retry, verification, and checkpoint machinery for leaf `Task` steps where safe
- [x] Reuse existing worktree preparation and scope enforcement paths for leaf execution while making workflow ownership explicit
- [x] Reuse existing merge-related task output attribution where possible, but keep merge orchestration itself out of this sprint

### 65.3 Workflow-native command surface (minimal)
- [x] Add initial workflow CLI/API commands for `create`, `list`, `inspect`, `start`, `tick`, `pause`, `resume`, and `abort`
- [x] Keep contracts machine-readable and parallel to existing Hivemind command patterns
- [x] Do not require any conversion of existing flows/tasks to use the new path

### 65.4 Automated validation and test coverage
- [x] Add unit tests for flat workflow scheduling and dependency release semantics
- [x] Add integration tests covering workflow-run launch through leaf attempt completion, verification, and retry
- [x] Add regression tests proving legacy flow execution remains operational while workflow execution is introduced
- [x] Add replay tests for workflow step state progression through success, retry, and failure paths

### 65.5 Documentation
- [x] Update CLI/API docs for the new workflow command surface
- [x] Add architecture notes describing the bridge from workflow step runs to attempt/worktree/verification subsystems
- [x] Document current limitations: flat workflows only, no child workflows, no workflow context yet

### 65.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 65 manual checklist under `@hivemind-test`
- [x] Smoke test a real repository workflow with at least three leaf task steps and one dependency edge
- [x] Smoke test pause/resume/abort and retry behavior through the workflow CLI surface
- [x] Validate workflow events, attempts, and verification outcomes remain attributable in CLI and event inspection paths
- [x] Publish Sprint 65 manual test report artifact in `@hivemind-test`

### 65.7 Exit Criteria
- [x] Flat workflows can execute real leaf work using existing runtime/attempt machinery
- [x] Workflow step scheduling is deterministic and replay-safe
- [x] New workflow CLI/API surface is usable for real smoke paths
- [x] Automated and manual smoke validation are completed and documented

---

## Sprint 66: Workflow Context, Step Inputs, And Append-Only Output Bag

**Goal:** Add deterministic workflow-scoped context and solve parallel output composition with an append-only output bag rather than shared mutable writes.

### 66.1 Workflow context model
- [x] Introduce a typed workflow context model with explicit initialization inputs, schema/version markers, and deterministic snapshot hashes
- [x] Define the three-layer contract explicitly:
  - [x] workflow context = run-scoped data plane
  - [x] step context = resolved per-step deterministic input snapshot
  - [x] attempt context = rendered runtime input delivered to a worker
- [x] Restrict context mutation to explicit event boundaries (step completion, child completion, signal receipt, human override, or other declared workflow actions)

### 66.2 Append-only output bag semantics
- [x] Introduce an append-only workflow output bag for step-produced outputs, with each entry carrying:
  - [x] producer `step_run_id`
  - [x] `workflow_run_id`
  - [x] optional branch/join lineage
  - [x] typed payload or blob reference
  - [x] schema version / output name / tags
  - [x] event sequence ordering
- [x] Disallow direct parallel mutation of shared named context keys by default
- [x] Require downstream steps to consume bag entries via explicit selectors, reducers, or join-step bindings
- [x] Define deterministic reducer rules for common fan-in patterns (single producer required, ordered list collect, keyed map collect, explicit reduce function)

### 66.3 Attempt-context integration
- [x] Extend attempt-context assembly so workflow-derived step input is injected as an explicit, hashed section of the attempt manifest
- [x] Keep current constitution/prompt/skills/documents/graph-summary inputs deterministic and additive, not replaced
- [x] Emit explicit events for workflow context initialization, step input resolution, output-bag append, and context snapshotting

### 66.4 Automated validation and test coverage
- [x] Add unit tests for context initialization, patching, snapshot hashing, and schema validation
- [x] Add property/integration tests for parallel append ordering and reducer determinism
- [x] Add tests proving no hidden last-writer-wins semantics exist for parallel outputs
- [x] Add attempt-manifest tests proving workflow context becomes part of deterministic attempt input hashes

### 66.5 Documentation
- [x] Add design docs for workflow context lifecycle, bag semantics, reducer rules, and step input resolution
- [x] Update architecture docs to explain how workflow context feeds the existing attempt-context system
- [x] Add operator-facing guidance on how to inspect context snapshots and bag entries during debugging

### 66.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 66 manual checklist under `@hivemind-test`
- [x] Smoke test a workflow with parallel branches that each append outputs to the bag and a downstream join that consumes them deterministically
- [x] Validate workflow context inspection and attempt-context inspection show attributable workflow-derived inputs
- [x] Validate failure behavior for invalid reducers, duplicate single-producer expectations, and schema mismatches
- [x] Publish Sprint 66 manual test report artifact in `@hivemind-test`

### 66.7 Exit Criteria
- [x] Workflow context is evented, inspectable, and replay-safe
- [x] Parallel branches write only through append-only bag semantics
- [x] Downstream step inputs are deterministic and hashable
- [x] Automated and manual smoke validation are completed and documented

---

## Sprint 67: Nested Workflows, Inheritance, And Lineage

**Goal:** Support workflow steps that launch child workflows with explicit input/output mappings and full lineage visibility.

### 67.1 Child workflow execution
- [x] Add `Workflow` step kind that launches a child workflow run from a parent workflow step
- [x] Require explicit input binding from parent workflow context/bag into child initialization inputs
- [x] Default to copy-in child context rather than live shared mutable state

### 67.2 Parent/child output and failure policy
- [x] Add explicit child completion output mapping back into the parent context/bag
- [x] Add configurable parent behavior for child success/failure/cancellation/timeout
- [x] Preserve explicit retry semantics for child workflow invocation without inventing hidden cross-run recovery state

### 67.3 Observability and queryability
- [x] Extend event/UI/query surfaces to inspect parent/child workflow lineage and subtree state
- [x] Ensure runtime/native event correlation remains attributable through nested workflow execution
- [x] Add workflow tree inspection output that makes nesting understandable without reading raw events only

### 67.4 Automated validation and test coverage
- [x] Add integration tests for parent launching child workflows and consuming child outputs
- [x] Add replay tests for nested workflows across success, retry, and failure branches
- [x] Add regression tests for lineage queries, filtering, and inspect views on nested runs
- [x] Add tests proving child context isolation from parent mutable state outside declared output mappings

### 67.5 Documentation
- [x] Add architecture/design docs for nested workflow lineage, inheritance, and failure propagation
- [x] Update operator docs for nested workflow inspection, debugging, and replay expectations
- [x] Document the rule that child workflows are explicit orchestration boundaries, not hidden scheduler implementation details

### 67.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 67 manual checklist under `@hivemind-test`
- [x] Smoke test a parent workflow that launches at least two child workflows and joins their outputs
- [x] Validate parent/child lineage in inspect commands, event streams, and runtime attribution paths
- [x] Validate child failure handling and retry semantics in a real repository scenario
- [x] Publish Sprint 67 manual test report artifact in `@hivemind-test`

### 67.7 Exit Criteria
- [x] Child workflows can be launched and observed deterministically
- [x] Parent/child input-output mappings are explicit and replay-safe
- [ ] Nested lineage is inspectable from CLI/API/events without guesswork
- [ ] Automated and manual smoke validation are completed and documented

---

## Sprint 68: Conditional Steps, Wait Semantics, And Workflow Signals

**Goal:** Add deterministic control-flow primitives driven by typed/evented data rather than implicit runtime output parsing.

### 68.1 Conditional execution
- [x] Implement `Conditional` step evaluation against typed workflow context and/or append-only bag reductions only
- [x] Reject branch conditions that depend on opaque free-text runtime output without explicit typed projection
- [x] Emit explicit events for condition evaluation inputs, result, and chosen path

### 68.2 Wait and signal model
- [x] Implement `Wait` step semantics for explicit workflow signals, bounded timers, or human/operator events
- [x] Add signal event contracts with dedupe/idempotency keys where needed
- [x] Ensure wait/resume behavior remains replay-safe and attributable

### 68.3 Control-plane safety
- [x] Support subtree pause/resume/abort behavior with explicit state transitions and no hidden scheduler wakeups
- [x] Preserve human authority at signal/override boundaries
- [x] Ensure signal handling cannot bypass scope, verification, or merge governance

### 68.4 Automated validation and test coverage
- [x] Add unit tests for condition evaluation and signal/wait transition guards
- [x] Add integration tests for signal-driven resume, timer expiry, and human override paths
- [x] Add negative tests proving invalid conditions and duplicate/late signals fail loudly with explicit errors
- [x] Add replay tests for conditional and wait-heavy workflow graphs

### 68.5 Documentation
- [x] Add design docs for condition syntax, allowed data sources, signal contracts, and wait semantics
- [x] Update CLI/API docs for raising signals and inspecting blocked/waiting workflow state
- [x] Add operator guidance for debugging stuck waits, duplicate signals, and branch selection

### 68.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 68 manual checklist under `@hivemind-test`
- [x] Smoke test a workflow that exercises both conditional branching and a wait-for-signal path in a real repository fixture
- [x] Validate pause/resume/abort and signal delivery remain visible and attributable in CLI/API/event inspection paths
- [x] Publish Sprint 68 manual test report artifact in `@hivemind-test`

### 68.7 Exit Criteria
- [x] Conditional and wait semantics are explicit, typed, and replay-safe
- [x] Signals resume workflows without hidden state or scheduler ambiguity
- [x] Human/operator interventions remain bounded and attributable
- [x] Automated and manual smoke validation are completed and documented

---

## Sprint 69: Workflow-Native Runtime, Merge, Worktree, And Public Surface Cutover

**Goal:** Make workflows the primary execution surface across CLI/API/observability while updating operational subsystems to key off workflow-native identity.

### 69.1 Runtime and worktree ownership cutover
- [x] Refactor runtime launch/selection paths so workflow run + step run identity becomes the primary execution owner
- [x] Update worktree naming/layout and inspection to reflect workflow-native ownership without requiring historical worktree migration
- [x] Preserve scope enforcement, retry, and checkpoint behavior under workflow-native ownership

### 69.2 Merge and integration semantics
- [x] Define and implement workflow-native merge preparation using successful leaf task outputs as the integration unit
- [x] Keep merge governance explicit and attributable even when work is produced by nested workflows
- [x] Ensure merge/report projections can explain which workflow/step lineage produced each integrated change

### 69.3 CLI/API/UI cutover
- [x] Promote `workflow/*` commands and API routes as the primary execution interface
- [x] Decide and document whether legacy `flow/*` remains available as transitional compatibility/read-only surface
- [x] Update server/UI state assembly and inspect views so workflow state is first-class rather than a derived afterthought

### 69.4 Automated validation and test coverage
- [ ] Add integration tests for workflow-native execution through runtime, verification, worktree, and merge prepare/approve/execute paths
- [x] Add regression tests for inspect/list/status/event query surfaces after cutover
- [ ] Add compatibility tests for any retained legacy `flow/*` read or alias behavior
- [ ] Add performance/replay tests proving workflow-native ownership does not break existing observability guarantees

### 69.5 Documentation
- [x] Update quickstart, CLI semantics, architecture, and operations docs to center workflows as the execution model
- [x] Add migration/operator docs describing the new primary surface and any retained legacy behavior
- [x] Update examples and tutorials to use workflows rather than `TaskFlow` as the default execution story

### 69.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 69 manual checklist under `@hivemind-test`
- [x] Smoke test workflow-native execution from project setup through verification and merge in a real repository fixture
- [x] Validate workflow inspect, attempts, runtime stream, worktree inspection, and merge inspection all reflect workflow-native ownership cleanly
- [x] Publish Sprint 69 manual test report artifact in `@hivemind-test`

### 69.7 Exit Criteria
- [x] Workflow is the primary execution surface for new runs
- [x] Runtime, worktree, and merge subsystems operate under workflow-native identity
- [x] Public CLI/API/docs are aligned with the workflow-first model
- [x] Automated and manual smoke validation are completed and documented

---

## Sprint 70: End-to-End Hardening, Replay Proof, And Phase Closeout

**Goal:** Prove the full workflow engine under real-project, multi-step, nested, context-heavy usage before declaring the migration complete.

### 70.1 End-to-end scenario matrix
- [ ] Validate end-to-end workflow scenarios covering at least:
  - [ ] flat workflow with retries and verification
  - [ ] nested workflow with child outputs
  - [ ] parallel branches with append-only bag fan-in
  - [ ] condition + wait/signal flow
  - [ ] workflow-native merge path
- [ ] Include at least one multi-repo workflow scenario if Phase 5 codepaths claim multi-repo support

### 70.2 Replay, recovery, and failure hardening
- [ ] Prove full replay determinism across workflow, context, signal, and nested-run events
- [ ] Add recovery tests for interrupted runs, partial child completion, and operator resume flows
- [ ] Validate failures remain first-class and inspectable at workflow, step, attempt, and merge boundaries

### 70.3 Documentation and closeout package
- [ ] Finalize docs across `docs/overview/`, `docs/architecture/`, `docs/design/`, and `docs/operations/` as needed
- [ ] Update `hivemind/changelog.json` with Phase 5 deliverables
- [ ] Add a Phase 5 validation/closeout report under `ops/reports`

### 70.4 Automated validation and test coverage
- [ ] Run and pass `cargo fmt --all`
- [ ] Run and pass `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Run and pass `cargo test --all-features`
- [ ] Add/retain targeted stress and replay tests for nested workflows, bag fan-in, and workflow-native merge/reporting

### 70.5 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 70 manual checklist under `@hivemind-test`
- [ ] Run at least one real-project end-to-end smoke exercise that uses the workflow system to build, modify, verify, and merge meaningful code changes
- [ ] Validate CLI/API/docs/manual test artifacts all agree on the workflow-first operating model
- [ ] Publish Sprint 70 manual test report artifact in `@hivemind-test`

### 70.6 Exit Criteria
- [ ] Workflow engine behavior is proven end-to-end on real repository smoke paths
- [ ] Replay, recovery, and failure semantics are stable and inspectable
- [ ] Documentation and changelog are aligned with the implemented system
- [ ] Automated and manual smoke validation are completed and documented

---

## Summary: Phase 5 Capability Coverage

| Capability | Target Sprint |
|------------|---------------|
| Workflow domain/events/projections | 64 |
| Flat workflow execution bridge | 65 |
| Workflow context + append-only output bag | 66 |
| Nested workflows + lineage | 67 |
| Conditionals + waits + signals | 68 |
| Workflow-native runtime/worktree/merge/public surface | 69 |
| End-to-end hardening and phase closeout | 70 |

---

## Phase 5 Completion Gates

- [ ] Sprint 64-70 implementation and manual test reports are present under `@hivemind-test`
- [ ] `workflow/*` is the primary execution surface for new work in Hivemind
- [ ] Workflow context and append-only output bag behavior are documented, inspectable, and replay-safe
- [ ] Nested workflow lineage is queryable in CLI/API/event inspection paths
- [ ] Runtime, verification, worktree, and merge observability remain explicit after workflow cutover
- [ ] `hivemind/changelog.json` is updated with Phase 5 deliverables
- [ ] Documentation is updated and aligned with implementation:
  - [ ] `docs/overview/`
  - [ ] `docs/architecture/`
  - [ ] `docs/design/`
  - [ ] `docs/operations/` (if introduced/expanded)
- [ ] `ops/reports` contains a Phase 5 closeout validation summary
- [ ] Roadmap checkboxes and completion status reflect final validated state

---

## Phase 5 Principle Checkpoints

After each sprint, verify:

1. **Event Authority:** Is workflow state still derived from events rather than hidden runtime memory?
2. **Determinism:** Are workflow context snapshots, reducers, and branch decisions replay-stable?
3. **No Hidden Writes:** Did parallel work append outputs instead of mutating shared state implicitly?
4. **Operator Clarity:** Can parent/child lineage, step inputs, bag outputs, waits, and failures be inspected without guesswork?
5. **Fail Loud:** Do invalid bindings, reducer mismatches, signal mistakes, and workflow transition violations surface as explicit typed failures?
6. **CLI-First:** Can the new workflow model be operated and smoke tested through CLI/API without internal-only escape hatches?
7. **No Accidental Migration Coupling:** Did implementation avoid blocking on historical `TaskFlow`/task migration work that Phase 5 explicitly excludes?

---

## Follow-On Candidates (Not Required For Phase 5)

- [ ] schedules, cron, and external trigger automation on top of the workflow engine
- [ ] richer typed reducers and schema-driven output contracts for workflow bag consumption
- [ ] collaborative workflow authoring UX and visual workflow editors
- [ ] workflow template marketplace/reuse distribution beyond local registry semantics
