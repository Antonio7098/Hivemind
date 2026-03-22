---
title: Workflow Foundation
description: Phase 5 workflow domain and state model foundation
order: 8
---

# Workflow Foundation

This document describes the Phase 5 workflow domain introduced in Sprint 64, extended in Sprint 65 with a flat execution bridge, extended in Sprint 66 with a deterministic workflow data plane, extended in Sprint 67 with explicit nested child workflows and lineage, extended in Sprint 68 with typed control-flow and workflow signals, and extended in Sprint 71 with a topology-aligned workflow-spec binding layer.

## Purpose

The workflow domain introduces a new orchestration model built around explicit, replayable concepts:

- `WorkflowDefinition` — immutable workflow structure owned by a project
- `WorkflowRun` — event-sourced execution record for a workflow definition
- `WorkflowStepDefinition` — static step metadata and dependency edges
- `WorkflowStepRun` — per-run step lifecycle state

This foundation is intentionally additive. `TaskFlow` remains present as a compatibility layer, but Sprint 69 promotes workflows to the primary execution surface for new runs.

## Current Guarantees

The current foundation and Sprint 65 bridge guarantee:

- workflow definitions and runs are stored as events
- workflow state is derived from replay, not hidden process memory
- workflow lineage has explicit correlation slots (`workflow_id`, `workflow_run_id`, `root_workflow_run_id`, `parent_workflow_run_id`, `step_id`, `step_run_id`)
- root steps become `ready` only after an explicit workflow start event
- invalid lifecycle transitions can be rejected before event append
- flat `task` workflow steps can execute through the existing flow/attempt/worktree/verification stack
- bridged flow, task, and attempt events remain attributable to the owning workflow run

Sprint 66 adds these guarantees:

- workflow runs carry a typed `WorkflowContextState` with explicit initialization inputs and deterministic snapshot hashes
- each step can materialize a deterministic `WorkflowStepContextSnapshot` that records the exact context hash, bag hash, resolved inputs, and selected output entry ids used for execution
- step-produced outputs are appended to a workflow-scoped `WorkflowOutputBag` rather than mutating shared context keys implicitly
- join steps can consume bag entries through selectors and reducers, emit new bag outputs, and patch workflow context explicitly at step-completion boundaries
- attempt-context assembly now injects a workflow-derived manifest section with workflow run ids and workflow/step hash references

Sprint 67 adds these guarantees:

- `workflow` steps launch explicit child `WorkflowRun` records rather than hidden scheduler internals
- child runs inherit `root_workflow_run_id`, stamp `parent_workflow_run_id` and `parent_step_id`, and remain queryable through normal workflow and event surfaces
- parent to child input transfer is copy-in only and uses the existing deterministic step input binding model
- child to parent output transfer is explicit through workflow-step output bindings and context patches that reference declared child context keys
- parent `workflow status` and `/api/workflow-runs/inspect` output include an explicit recursive `child_runs` tree with workflow and parent-step names so nested execution can be inspected without reconstructing raw events manually
- `events list --workflow-run <root-run-id>` resolves root runs to `root_workflow_run_id` filtering so subtree event inspection follows the root lineage by default

Sprint 68 adds these guarantees:

- `conditional` steps evaluate only typed workflow context values or explicit bag reductions
- condition results are evented with attributable inputs and chosen branch names
- `wait` steps enter a durable waiting state with explicit activation metadata rather than hidden scheduler sleep
- workflow signals are append-only, idempotent by explicit key, and queryable on the workflow run
- wait completion by signal or timer is evented and replay-safe
- workflow pause/resume/abort operations recurse through child workflow subtrees explicitly

Sprint 71 adds these guarantees:

- workflow topology remains owned by `WorkflowDefinition` / `WorkflowStepDefinition`; the spec tree is attached to that topology rather than redefining it
- a `WorkflowSpecBinding` can be validated and persisted against an existing workflow plus explicit child-workflow boundaries
- spec nodes bind explicitly to workflow ids and step ids, so the intent layer shares the same topology as execution
- workflow-run inspection can expose the bound workflow-spec node for the current workflow subtree together with the spec binding hash
- workflow-backed step snapshots and attempt-context manifests can include spec node ids and merged spec execution context without altering scheduler ownership or replay rules

## Explicit Non-Goals

The current implementation still does **not** add:

- historical migration of legacy `TaskFlow` runs into workflow runs
- a dedicated workflow-native retry command
- full inheritance/override propagation for all workflow-spec governance fields across nested specs

Those capabilities are layered in later Phase 5 sprints.

## Event Families

The initial workflow event family includes:

- `WorkflowDefinitionCreated`
- `WorkflowDefinitionUpdated`
- `WorkflowConditionEvaluated`
- `WorkflowWaitActivated`
- `WorkflowWaitCompleted`
- `WorkflowSignalReceived`
- `WorkflowRunCreated`
- `WorkflowRunStarted`
- `WorkflowRunPaused`
- `WorkflowRunResumed`
- `WorkflowRunCompleted`
- `WorkflowRunAborted`
- `WorkflowContextInitialized`
- `WorkflowContextSnapshotCaptured`
- `WorkflowStepInputsResolved`
- `WorkflowOutputAppended`
- `WorkflowStepStateChanged`

These events are projected into `AppState.workflows` and `AppState.workflow_runs`.

## CLI / API Surface

Sprint 65 exposes a usable workflow-facing control surface:

- CLI:
  - `hivemind workflow create|update|step-add|list|inspect`
  - `hivemind workflow spec-validate|spec-inspect|spec-bind|spec-clear`
  - `hivemind workflow run-create|run-list|status|start|tick|complete|pause|resume|signal|abort|step-set-state`
  - `hivemind workflow runtime-stream|worktree-list|worktree-inspect|worktree-cleanup`
  - `hivemind workflow merge-prepare|merge-approve|merge-execute`
- API:
  - `/api/workflows`
  - `/api/workflows/inspect`
  - `/api/workflows/create`
  - `/api/workflows/update`
  - `/api/workflows/steps/add`
  - `/api/workflows/spec/validate`
  - `/api/workflows/spec/bind`
  - `/api/workflow-runs`
  - `/api/workflow-runs/inspect`
  - `/api/workflow-runs/create`
  - `/api/workflow-runs/start`
  - `/api/workflow-runs/tick`
  - `/api/workflow-runs/complete`
  - `/api/workflow-runs/pause`
  - `/api/workflow-runs/resume`
  - `/api/workflow-runs/signal`
  - `/api/workflow-runs/abort`
  - `/api/workflow-runs/steps/state`
  - `/api/runtime-stream?workflow_run_id=<id>`
  - `/api/worktrees?workflow_run_id=<id>`
  - `/api/worktrees/inspect?workflow_run_id=<id>&step_id=<id>`
  - `/api/merge/*` with `workflow_run_id` as the primary owner selector

The surface remains intentionally narrow: nested child workflows are now supported, but retry is still bridged through explicit step-state transitions and the synthetic task path for leaf execution.

## Sprint 71 Spec Binding Layer

Sprint 71 adds a first-class workflow-spec layer for intent, constraints, verification posture, and execution context.

The rule is explicit:

- workflow topology is still owned by workflow definitions and child-workflow steps
- spec topology must mirror workflow topology through explicit workflow id and step id bindings
- nested workflow spec nodes bind to existing child workflow definitions instead of implicitly creating them
- binding validation fails loudly on missing step coverage, workflow mismatches, duplicate bindings, or invalid node governance fields

This keeps the spec tree and execution tree aligned without making the spec tree the runtime scheduler authority.

## Sprint 65 Execution Bridge

Sprint 65 maps a `WorkflowRun` onto a synthetic graph/flow/task topology so existing attempt execution, worktree preparation, checkpointing, verification, and retry machinery can be reused without migrating legacy flow internals.

Operationally this means:

- the workflow run owns the operator-facing lifecycle
- a synthetic graph and flow are created lazily on first `workflow tick`
- each workflow `task` step is mapped onto a synthetic task
- dependency edges are mirrored into the synthetic graph so legacy release semantics still drive readiness
- workflow status is reconciled from bridged task execution state during tick/lifecycle sync
- bridged flow/task/attempt events are stamped with workflow correlation ids so event inspection can attribute work back to the workflow run
- runtime stream, worktree inspection, and merge preparation can now be driven directly by `workflow_run_id` without requiring operators to discover the internal synthetic `flow_id`

This keeps the execution path replayable while avoiding a second runtime/worktree stack during Sprint 65.

## Sprint 66 Data Plane

Sprint 66 layers a workflow-native data plane over the execution bridge without replacing the underlying attempt machinery.

The contract is explicit:

- workflow context = run-scoped mutable data plane, changed only by declared events
- step context = deterministic resolved input snapshot for a specific step run
- attempt context = immutable runtime input assembled for a worker attempt

### Workflow context

`WorkflowContextState` is initialized when the run is created and immediately emits:

- `WorkflowContextInitialized`
- `WorkflowContextSnapshotCaptured`

The initial snapshot captures:

- context schema and schema version
- initialization inputs
- snapshot revision and hash

Only explicit patch application produces a new context snapshot. In Sprint 66, those patches are restricted to join-step completion.

### Output bag

Parallel branches do not race on shared context keys. They emit append-only bag entries with:

- `workflow_run_id`
- producer `step_id` and `step_run_id`
- optional `branch_step_id` / `join_step_id`
- output name, tags, schema, and schema version
- deterministic event-sequence ordering

Bag consumers must opt into explicit selectors and reducers. There is no implicit last-writer-wins merge of branch outputs.

### Step input resolution

Before a task attempt is launched or a join step is executed, the registry resolves all step inputs into a `WorkflowStepContextSnapshot`. That snapshot records:

- the workflow context hash used
- the output bag hash used
- the fully resolved input map
- the selected output entry ids for bag-derived inputs
- a deterministic step-input snapshot hash

This snapshot is emitted via `WorkflowStepInputsResolved` and then reused for output emission, join completion, and attempt-context assembly.

### Attempt-context integration

When a workflow-backed task attempt is launched, the immutable attempt manifest now includes a `workflow` section containing:

- `workflow_run_id`
- `step_id`
- `step_run_id`
- workflow context schema/version
- `context_snapshot_hash`
- `step_input_snapshot_hash`
- `output_bag_hash`
- selected `output_entry_ids`
- optional `spec_binding_hash`
- optional bound `spec_node_ids`
- optional `spec_context_hash`

That keeps workflow-derived execution input explicit and hashable without replacing the existing constitution, prompt, skills, documents, graph summary, or retry-link manifest inputs.

## Operator Inspection Guidance

During debugging, operators should inspect three surfaces together:

- `hivemind workflow status <run-id>` for current context snapshots, step snapshots, and output bag entries
- `hivemind events list --workflow-run <run-id>` for the sequence of `workflow_context_*`, `workflow_step_inputs_resolved`, and `workflow_output_appended` events
- attempt context inspection for workflow-backed attempts to confirm the immutable attempt manifest includes the expected workflow hashes

If a join step fails, inspect the reducer error together with the bag entry list. The system is designed to fail explicitly on duplicate single-producer expectations and schema mismatches rather than silently choosing a winner.

If a wait step appears stuck, inspect:

- `workflow status` for the current `wait_status` and recorded `signals`
- `events list --workflow-run <run-id>` for `workflow_wait_activated`, `workflow_signal_received`, and `workflow_wait_completed`
- the signal idempotency key to confirm whether the signal was rejected as a duplicate or simply targeted at a different step

## Design Constraint

Workflow additions must remain:

- **modular** — no hidden dependency on `TaskFlow` projection internals
- **observable** — every durable state change must be evented
- **deterministic** — replay must reconstruct the same workflow state
- **extensible** — later sprints must be able to add context, nesting, and orchestration primitives without rewriting the domain foundation
