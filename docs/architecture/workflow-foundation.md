---
title: Workflow Foundation
description: Phase 5 workflow domain and state model foundation
order: 8
---

# Workflow Foundation

This document describes the initial **Phase 5 / Sprint 64** workflow foundation added alongside legacy `TaskFlow`.

## Purpose

The workflow domain introduces a new orchestration model built around explicit, replayable concepts:

- `WorkflowDefinition` — immutable workflow structure owned by a project
- `WorkflowRun` — event-sourced execution record for a workflow definition
- `WorkflowStepDefinition` — static step metadata and dependency edges
- `WorkflowStepRun` — per-run step lifecycle state

This foundation is intentionally additive. `TaskFlow` remains present while workflows are introduced as a parallel bounded domain.

## Current Guarantees

The Sprint 64 foundation guarantees:

- workflow definitions and runs are stored as events
- workflow state is derived from replay, not hidden process memory
- workflow lineage has explicit correlation slots (`workflow_id`, `workflow_run_id`, `root_workflow_run_id`, `parent_workflow_run_id`, `step_id`, `step_run_id`)
- root steps become `ready` only after an explicit workflow start event
- invalid lifecycle transitions can be rejected before event append

## Explicit Non-Goals

Sprint 64 adds the domain foundation only. It does **not** yet add:

- workflow-context dataflow
- nested child workflow execution
- signal/wait orchestration
- workflow-native runtime/worktree/merge ownership

Those capabilities are layered in later Phase 5 sprints.

## Event Families

The initial workflow event family includes:

- `WorkflowDefinitionCreated`
- `WorkflowDefinitionUpdated`
- `WorkflowRunCreated`
- `WorkflowRunStarted`
- `WorkflowRunPaused`
- `WorkflowRunResumed`
- `WorkflowRunCompleted`
- `WorkflowRunAborted`
- `WorkflowStepStateChanged`

These events are projected into `AppState.workflows` and `AppState.workflow_runs`.

## CLI / API Surface

Sprint 64 introduces an initial workflow-facing control surface:

- CLI: `hivemind workflow ...` for create/list/inspect/update/step authoring/run lifecycle
- API: `/api/workflows*` and `/api/workflow-runs*` for definition/run CRUD and inspection

The surface is intentionally minimal and focused on definition/run lifecycle inspection and control.

## Design Constraint

Workflow additions must remain:

- **modular** — no hidden dependency on `TaskFlow` projection internals
- **observable** — every durable state change must be evented
- **deterministic** — replay must reconstruct the same workflow state
- **extensible** — later sprints must be able to add context, nesting, and orchestration primitives without rewriting the domain foundation