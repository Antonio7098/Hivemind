---
title: Workflow Spec Binding
description: Sprint 71 topology-aligned workflow spec binding rules
order: 19
---

# Workflow Spec Binding

Sprint 71 adds a workflow-spec layer that carries intent, constraints, acceptance criteria, verification posture, and execution context.

The workflow spec is **not** the executable workflow definition.
It is a topology-aligned governance layer attached to the existing workflow graph.

## Core Rule

- `WorkflowDefinition` and `WorkflowStepDefinition` own execution topology
- `WorkflowSpecBinding` owns recursive intent/governance context
- spec nodes must bind onto existing workflow ids and step ids
- nested workflow spec nodes must bind to explicit child-workflow steps and their child workflow definitions

There is one shared topology.
There are not two separate trees that happen to look similar.

## Node Kinds

Sprint 71 keeps the spec model narrow:

- `workflow` spec node
- `task` spec node

A `workflow` node binds to a workflow definition and, for nested workflows, to the parent step that launches it.

A `task` node binds to a concrete non-workflow step inside a workflow definition.

## Required Governance Fields

Every spec node must declare:

- stable `id`
- `title`
- `intent`
- at least one acceptance criterion
- verification posture

These fields exist so reports, inspections, and execution context can point back to an attributable spec node instead of free text or hidden conventions.

## Binding Validation

Binding fails loudly when:

- a spec node omits a required workflow or step binding
- the root workflow node does not bind to the target workflow definition
- a workflow step is missing a corresponding spec node
- multiple spec nodes bind to the same step
- a `task` spec node binds to a workflow-launch step
- a nested `workflow` spec node binds to the wrong child workflow definition
- required governance fields are empty

This preserves Hivemind's rule that ambiguous authoring is an error, not a heuristic.

## Execution Context Propagation

Each spec node may declare typed `execution_context` values.

For a bound task step:

1. collect the spec-node path from the root workflow spec node to the task spec node
2. merge `execution_context` maps in path order
3. expose the merged values on the workflow step snapshot and workflow-backed attempt context

This makes spec context available to execution without mutating workflow topology or introducing hidden memory.

## Inspection Surface

Sprint 71 exposes bound spec data through existing workflow-owned surfaces:

- workflow inspection returns the persisted `WorkflowSpecBinding`
- workflow-run inspection returns the bound workflow-spec node for the current subtree plus the spec binding hash
- workflow-backed step snapshots record bound spec node ids and merged spec context hash
- workflow-backed attempt manifests include spec binding and spec context hashes

Execution remains driven through normal `workflow/*` operations.
Spec binding adds context. It does not add a second runtime control path.
