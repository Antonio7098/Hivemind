---
title: Nested Workflow Lineage
description: Sprint 67 parent-child workflow execution, mapping, and observability rules
order: 17
---

# Nested Workflow Lineage

Sprint 67 adds explicit workflow-step driven child workflow execution.

## Boundary Rule

Child workflows are explicit orchestration boundaries.

- a parent workflow step of kind `workflow` declares the child workflow definition id up front
- parent inputs are copied into child initialization inputs through the existing step-input binding contract
- child runs do not share live mutable context with the parent
- child outputs return only through explicit parent workflow-step output bindings or context patches

There is no hidden shared memory between parent and child runs.

## Lineage Contract

Every child run records:

- `workflow_run_id`
- `root_workflow_run_id`
- `parent_workflow_run_id`
- `parent_step_id`

The root run id is inherited from the top-level parent. This makes event filtering and subtree inspection replay-safe without inference.

## Completion Mapping

Parent workflow steps may map child data back through:

- `WorkflowStepOutputBinding` with `child_context_key`
- `WorkflowContextPatchBinding` with `child_context_key`

This keeps the return path declarative and evented.

## Failure Policy

Workflow child steps carry an explicit terminal policy.

- `complete`
- `fail_step`
- `abort_parent`

Current runtime handling distinguishes success, failure, and cancellation/abort branches. Timeout policy is declared now so the contract remains explicit when timeout-specific child termination is added to runtime execution.

## Inspection Guidance

Operators should inspect:

1. `hivemind workflow status <run-id>` for the root `WorkflowRun` plus an explicit `child_runs` tree carrying `workflow_name`, `parent_step_name`, and recursive descendants
2. `/api/workflow-runs/inspect?workflow_run_id=<run-id>` for the same inspect payload used by the CLI rather than a raw run projection
3. `hivemind events list --workflow-run <root-run-id>` for full nested lineage; when the supplied run id is a root run, the CLI resolves it to `root_workflow_run_id=<run-id>` so child events are included without additional flags
4. `hivemind workflow run-list` plus explicit `root_workflow_run_id` / `parent_workflow_run_id` fields when validating subtree reconstruction
