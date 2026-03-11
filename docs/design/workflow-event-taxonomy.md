# Workflow Event Taxonomy, Identity, and Projection Rules

## Purpose

Sprint 64 introduces the workflow domain as a new event-sourced execution surface that lives alongside legacy `TaskFlow`.
This document defines the initial workflow event family, required lineage identifiers, and projection rules.

## Event Families

### Definition events

- `WorkflowDefinitionCreated`
- `WorkflowDefinitionUpdated`

These events carry the full `WorkflowDefinition` snapshot.
The latest definition event wins during replay.

### Run lifecycle events

- `WorkflowRunCreated`
- `WorkflowRunStarted`
- `WorkflowRunPaused`
- `WorkflowRunResumed`
- `WorkflowRunCompleted`
- `WorkflowRunAborted`

`WorkflowRunCreated` carries the full `WorkflowRun` snapshot.
Subsequent lifecycle events mutate state through explicit projection logic rather than hidden runtime memory.

### Step lifecycle events

- `WorkflowStepStateChanged`

This event captures:

- `workflow_run_id`
- `step_id`
- `step_run_id`
- target `state`
- optional `reason`

It is the authoritative record for step run progression inside a workflow run.

## Required Correlation Identity

Workflow-native events extend correlation/lineage with explicit identifiers:

- `workflow_id`
- `workflow_run_id`
- `root_workflow_run_id`
- `parent_workflow_run_id`
- `step_id`
- `step_run_id`

Rules:

1. `workflow_run_id` identifies the current run being mutated.
2. `root_workflow_run_id` always points at the top-most run in the lineage tree.
3. `parent_workflow_run_id` is `None` for root runs and reserved for nested workflows later.
4. `step_id` and `step_run_id` are set only for step-scoped events.
5. Legacy graph/flow/task identifiers remain available and unchanged for non-workflow events.

## Projection Rules

`AppState` projects workflow data into:

- `workflows: HashMap<Uuid, WorkflowDefinition>`
- `workflow_runs: HashMap<Uuid, WorkflowRun>`

Replay rules:

- definition create/update replaces the stored definition snapshot by `definition.id`
- run create inserts the run snapshot by `run.id`
- start/pause/resume/complete/abort mutate the stored run only if it exists
- step state changes mutate the matching `WorkflowStepRun` and refresh run timestamps
- replay must remain deterministic regardless of whether legacy flow events are interleaved

## Explicit Sprint 64 Non-Goals

Sprint 64 does **not** introduce:

- workflow execution scheduling beyond manual lifecycle control
- nested child workflow execution
- append-only workflow context/output bag behavior
- migration of legacy `TaskFlow` history or ownership

Those land in later Phase 5 sprints.

