---
title: Workflow Context Data Plane
description: Sprint 66 workflow context, step-input resolution, and append-only bag semantics
order: 16
---

# Workflow Context Data Plane

Sprint 66 adds a deterministic workflow-scoped data plane on top of the Sprint 65 execution bridge.

The design goal is narrow: allow workflow steps to exchange typed data without introducing hidden mutable memory, implicit merge behavior, or branch races.

## Three-Layer Contract

The data plane is intentionally split into three layers:

- workflow context: run-scoped data state owned by the workflow run
- step context: deterministic per-step resolved input snapshot
- attempt context: immutable runtime input assembled for a worker attempt

These layers are related, but they are not interchangeable.

## Workflow Context

`WorkflowContextState` contains:

- `schema`
- `schema_version`
- `initialization_inputs`
- `current_snapshot`
- `snapshots`

Each `WorkflowContextSnapshot` carries:

- `revision`
- `snapshot_hash`
- typed `values`
- `reason`
- optional triggering `step_id` and `step_run_id`

### Initialization

Workflow context is initialized during `workflow run-create`.

The initialization path is explicit:

1. normalize typed initialization inputs
2. create revision `1`
3. derive deterministic `snapshot_hash`
4. emit `WorkflowContextInitialized`
5. emit `WorkflowContextSnapshotCaptured`

## Step Context

`WorkflowStepContextSnapshot` records the exact inputs a step used.

It includes:

- `workflow_run_id`
- `step_id`
- `step_run_id`
- `context_snapshot_hash`
- `output_bag_hash`
- resolved `inputs`
- binding `resolutions`
- `snapshot_hash`

This snapshot is the authority for:

- task-attempt workflow manifest injection
- join-step native execution
- output binding materialization
- workflow context patch materialization

## Output Bag

Parallel outputs do not mutate shared context keys directly. They append to `WorkflowOutputBag`.

Each `WorkflowOutputBagEntry` carries:

- `workflow_run_id`
- `producer_step_id`
- `producer_step_run_id`
- optional `branch_step_id`
- optional `join_step_id`
- `output_name`
- `tags`
- typed `payload`
- `event_sequence`

The bag hash is derived from the full ordered entry list. Changing append order changes the hash.

## Input Sources

Step inputs can be resolved from:

- context keys
- literals
- bag selectors plus reducers

Bag selectors can constrain:

- `output_name`
- producer step ids
- tags
- expected schema
- expected schema version

## Reducer Rules

Sprint 66 supports these deterministic reducers:

- `single`
  Requires exactly one selected entry. More or fewer entries is an explicit error.
- `ordered_list`
  Preserves selected entry order and returns a typed list payload.
- `keyed_map`
  Produces a deterministic object keyed by producer step id, producer step name, or output name.
- `reduce_function.concat_text_newline`
  Concatenates string payloads in selected-entry order with newline separators.

### Failure Semantics

Reducers fail loudly when:

- no single producer exists for `single`
- more than one producer exists for `single`
- payload schema does not match the selector contract
- payload content does not satisfy reducer-specific type rules

There is no hidden winner-selection behavior.

## Context Mutation Rule

Workflow context mutation is restricted to declared event boundaries.

In Sprint 66:

- task steps may emit output bag entries
- join steps may emit output bag entries
- join steps may patch workflow context explicitly

Non-join steps may not declare workflow context patches. This is enforced during step authoring.

## Attempt-Context Integration

For workflow-backed task attempts, the immutable attempt manifest now includes:

- workflow run id
- step id
- step run id
- workflow context schema/version
- context snapshot hash
- step input snapshot hash
- output bag hash
- selected output entry ids

This keeps workflow-derived execution input additive to the existing attempt manifest rather than replacing it.

## Inspection Guidance

To debug workflow data flow:

1. inspect `workflow status` for current context, step snapshots, and output bag
2. inspect `events list --workflow-run` for `workflow_context_*`, `workflow_step_inputs_resolved`, and `workflow_output_appended`
3. inspect the attempt-context manifest for workflow-backed attempts when validating worker input determinism

If those three views disagree, the implementation is wrong.
