# Workflow Event Taxonomy, Identity, and Projection Rules

## Purpose

Sprint 64 introduced the workflow domain as a new event-sourced execution surface that lives alongside legacy `TaskFlow`.
Sprint 65 extends that foundation with a synthetic flow bridge for flat `task` step execution.
This document defines the workflow event family, required lineage identifiers, and projection rules.

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
6. Bridged synthetic flow/task/attempt events must preserve workflow lineage so event filters can attribute runtime activity back to the owning workflow run.

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
- bridged flow/task execution events may be interleaved with workflow-native events, but workflow reconciliation must still produce the same `WorkflowRun`/`WorkflowStepRun` states

## Sprint 65 Bridge Notes

When `workflow tick` executes a flat workflow:

- the registry creates a synthetic graph and synthetic flow keyed to the workflow run
- each `task` step gets a synthetic task id and continues through the legacy attempt lifecycle
- workflow-native status is recovered from the bridged task execution states during sync
- retry currently remains operational through the synthetic task retry path rather than a dedicated workflow-native retry command
- unsupported step kinds are rejected before synthetic execution starts

## Current Non-Goals

The current implementation does **not** introduce:

- workflow execution scheduling beyond manual lifecycle control
- nested child workflow execution
- append-only workflow context/output bag behavior
- full migration of legacy `TaskFlow` runtime ownership out of the synthetic bridge

Those land in later Phase 5 sprints.
