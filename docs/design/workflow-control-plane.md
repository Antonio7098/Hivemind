---
title: Workflow Control Plane
description: Sprint 68 conditional, wait, and signal semantics
order: 17
---

# Workflow Control Plane

Sprint 68 adds deterministic control-flow primitives to the workflow engine without introducing hidden scheduler state or opaque prompt parsing.

## Conditional Steps

`conditional` steps evaluate explicit typed expressions only.

Allowed data sources:

- workflow context keys
- append-only output bag selectors plus deterministic reducers

Disallowed data sources:

- free-text runtime transcripts
- implicit "latest output" lookups
- non-attributable worker side effects

Current condition forms:

- `context_key_exists`
- `context_value_equals`
- `bag_count_at_least`
- `bag_value_equals`

Each branch declares:

- a stable branch name
- a typed condition expression
- optional `activate_step_ids` used to define branch-specific downstream subtrees

Evaluation rules:

1. Branches are evaluated in definition order.
2. The first `true` branch wins.
3. If no branch matches, `default_branch` is used when configured.
4. If no branch matches and no default exists, the step fails loudly.

Every evaluation emits `WorkflowConditionEvaluated` with:

- workflow/step lineage
- typed evaluation inputs
- boolean result
- chosen branch name when matched

## Wait Steps

`wait` steps convert explicit external conditions into durable workflow state.

Supported wait modes:

- `signal`
- `human_signal`
- `timer`

Wait activation emits `WorkflowWaitActivated` and stores durable `wait_status` on the step run:

- wait condition
- activation time
- timer resume deadline when applicable
- completion metadata once satisfied

No hidden scheduler wakeups are allowed. A wait step advances only when:

- a matching signal is appended
- a timer deadline is observed during tick

Completion emits `WorkflowWaitCompleted`, after which the step transitions to `succeeded`.

## Signals

Signals are first-class workflow events.

Signal contract:

- `signal_name`
- `idempotency_key`
- optional typed payload
- optional target `step_id`
- `emitted_by`
- `emitted_at`

Rules:

- duplicate `idempotency_key` values for the same workflow run are rejected
- signal payload schema checks are enforced when the wait definition declares them
- `human_signal` waits require `emitted_by == "human"`
- accepted signals remain visible on `workflow status`

CLI/API surface:

- CLI: `hivemind workflow signal <run> <signal> --idempotency-key <key> [--step-id <step>]`
- API: `POST /api/workflow-runs/signal`

## Operator Guidance

To debug a stuck wait:

1. Inspect `workflow status <run-id>` and confirm the step is `waiting`.
2. Confirm the `wait_status.condition` matches the intended signal name or timer.
3. Confirm `signals[]` includes the expected signal and target `step_id`.
4. Inspect workflow-filtered events for `workflow_wait_activated`, `workflow_signal_received`, and `workflow_wait_completed`.

To debug branch selection:

1. Inspect `WorkflowConditionEvaluated` events in sequence order.
2. Verify the typed inputs match the expected workflow context or reduced bag values.
3. Confirm skipped subtrees correspond to non-selected branch step ids.
