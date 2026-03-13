---
title: Workflow Foundation
description: Phase 5 workflow domain and state model foundation
order: 8
---

# Workflow Foundation

This document describes the Phase 5 workflow domain introduced in Sprint 64 and extended in Sprint 65 with a flat execution bridge on top of the legacy `TaskFlow` attempt machinery.

## Purpose

The workflow domain introduces a new orchestration model built around explicit, replayable concepts:

- `WorkflowDefinition` — immutable workflow structure owned by a project
- `WorkflowRun` — event-sourced execution record for a workflow definition
- `WorkflowStepDefinition` — static step metadata and dependency edges
- `WorkflowStepRun` — per-run step lifecycle state

This foundation is intentionally additive. `TaskFlow` remains present while workflows are introduced as a parallel bounded domain.

## Current Guarantees

The current foundation and Sprint 65 bridge guarantee:

- workflow definitions and runs are stored as events
- workflow state is derived from replay, not hidden process memory
- workflow lineage has explicit correlation slots (`workflow_id`, `workflow_run_id`, `root_workflow_run_id`, `parent_workflow_run_id`, `step_id`, `step_run_id`)
- root steps become `ready` only after an explicit workflow start event
- invalid lifecycle transitions can be rejected before event append
- flat `task` workflow steps can execute through the existing flow/attempt/worktree/verification stack
- bridged flow, task, and attempt events remain attributable to the owning workflow run

## Explicit Non-Goals

The current implementation still does **not** add:

- workflow-context dataflow
- nested child workflow execution
- signal/wait orchestration
- workflow-native runtime/worktree/merge ownership without the synthetic flow bridge
- a dedicated workflow-native retry command

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

Sprint 65 exposes a usable workflow-facing control surface:

- CLI:
  - `hivemind workflow create|update|step-add|list|inspect`
  - `hivemind workflow run-create|run-list|status|start|tick|complete|pause|resume|abort|step-set-state`
- API:
  - `/api/workflows`
  - `/api/workflows/inspect`
  - `/api/workflows/create`
  - `/api/workflows/update`
  - `/api/workflows/steps/add`
  - `/api/workflow-runs`
  - `/api/workflow-runs/inspect`
  - `/api/workflow-runs/create`
  - `/api/workflow-runs/start`
  - `/api/workflow-runs/tick`
  - `/api/workflow-runs/complete`
  - `/api/workflow-runs/pause`
  - `/api/workflow-runs/resume`
  - `/api/workflow-runs/abort`
  - `/api/workflow-runs/steps/state`

The surface remains intentionally narrow: workflow execution currently supports flat `task` steps only, and retry is still bridged through the synthetic task path.

## Sprint 65 Execution Bridge

Sprint 65 maps a `WorkflowRun` onto a synthetic graph/flow/task topology so existing attempt execution, worktree preparation, checkpointing, verification, and retry machinery can be reused without migrating legacy flow internals.

Operationally this means:

- the workflow run owns the operator-facing lifecycle
- a synthetic graph and flow are created lazily on first `workflow tick`
- each workflow `task` step is mapped onto a synthetic task
- dependency edges are mirrored into the synthetic graph so legacy release semantics still drive readiness
- workflow status is reconciled from bridged task execution state during tick/lifecycle sync
- bridged flow/task/attempt events are stamped with workflow correlation ids so event inspection can attribute work back to the workflow run

This keeps the execution path replayable while avoiding a second runtime/worktree stack during Sprint 65.

## Design Constraint

Workflow additions must remain:

- **modular** — no hidden dependency on `TaskFlow` projection internals
- **observable** — every durable state change must be evented
- **deterministic** — replay must reconstruct the same workflow state
- **extensible** — later sprints must be able to add context, nesting, and orchestration primitives without rewriting the domain foundation
