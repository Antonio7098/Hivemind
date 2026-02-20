---
title: TaskFlow
description: Execution semantics and FSMs
order: 9
---

# Hivemind — TaskFlow Specification (taskflow.md)

> **Core idea:** TaskFlow is a deterministic scheduler over a static plan, combined with controlled, observable execution loops.

This document defines **TaskFlow**, the execution engine of Hivemind. It specifies how tasks are released, executed, verified, retried, paused, resumed, and completed — without introducing hidden state or implicit behavior.

This is a **behavioral contract**, not an implementation guide.

---

## 1. Purpose of TaskFlow

TaskFlow exists to:
- Execute planned work safely
- Coordinate parallel agents
- Enforce dependencies and scopes
- Support retries and human intervention
- Provide full observability and replay

TaskFlow does **not** plan work. It executes a plan.

---

## 2. Inputs to TaskFlow

A TaskFlow is created from:

- A **TaskGraph** (static DAG of intent)
- Project context
- Runtime bindings
- Initial configuration (retry limits, merge policy)

Once created, these inputs are **immutable** for the lifetime of the TaskFlow.

At attempt start, worker input is assembled through an attempt-scoped active context window with explicit operations (`add`, `expand`, `prune`, `snapshot`) and then frozen into an immutable attempt context manifest (constitution, resolved template system prompt/skills/documents, graph summary, and retry links).

---

## 3. Core Components

TaskFlow consists of three cooperating mechanisms:

1. **Graph Scheduler** — releases tasks based on dependency satisfaction
2. **Task Execution FSM** — manages attempts, verification, and retries
3. **Control Plane** — pause, resume, abort, and human overrides

Each mechanism operates exclusively via events.

---

## 4. TaskGraph Scheduling Rules

### 4.1 Dependency Resolution

A task becomes *eligible* for execution when:
- All upstream dependencies have reached `SUCCESS`

Failure states do **not** unblock downstream tasks.

### 4.2 Inter-Flow Dependencies

TaskFlows may depend on other TaskFlows in the same project.

- A dependent flow may not start until all declared upstream flows are `COMPLETED`
- Dependency edges are only mutable while the dependent flow is in `CREATED`
- Dependency cycles are rejected

When a completed flow unblocks dependent flows that are in `auto` run mode, those flows are started automatically.

---

### 4.3 Scheduling Guarantees

- A task is scheduled at most once at a time
- Tasks may execute in parallel if:
  - Dependencies are satisfied
  - Scopes are compatible
- Parallel dispatch width is policy-driven:
  - Per-project runtime policy (`max_parallel_tasks`)
  - Optional per-tick override (`flow tick --max-parallel`)
  - Optional global cap (`HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL`)
- Scope conflict handling is deterministic:
  - Hard conflicts are serialized and deferred in the current tick
  - Soft conflicts are allowed but explicitly emitted as warning telemetry

Scheduling emits explicit events (`TaskReady`, `TaskBlocked`, `ScopeConflictDetected`, `TaskSchedulingDeferred`).

---

### 4.4 Run Modes

TaskFlow run mode:

- `manual` (default): flow requires explicit `start/resume/tick` control
- `auto`: flow progresses automatically when runnable

Task run mode:

- `auto` (default): task can be dispatched when ready
- `manual`: task remains ready but is skipped by automatic scheduling until mode is changed

---

## 5. Task Execution State Machine

Each task node has its own execution lifecycle.

### 5.1 States

```
PENDING
RUNNING
VERIFYING
SUCCESS
RETRY
FAILED
ESCALATED
```

---

### 5.2 State Semantics

- **PENDING** — task not yet eligible
- **RUNNING** — worker agent executing
- **VERIFYING** — verifier evaluating output
- **SUCCESS** — task completed and accepted
- **RETRY** — task scheduled for another attempt
- **FAILED** — task irrecoverably failed
- **ESCALATED** — human intervention required

---

### 5.3 State Transition Rules

Key transitions:

- `PENDING → RUNNING` when scheduled
- `RUNNING → VERIFYING` when worker completes
- `VERIFYING → SUCCESS` on pass
- `VERIFYING → RETRY` on soft failure
- `VERIFYING → FAILED` on hard failure
- `RETRY → RUNNING` if retries remain
- `FAILED → ESCALATED` if manual recovery is allowed

No other transitions are permitted.

---

## 6. Attempts

### 6.1 Attempt Definition

An **Attempt** is a single worker execution.

Properties:
- Each attempt is immutable
- Attempts are append-only
- Attempts do not alter the TaskGraph

---

### 6.2 Attempt Lifecycle

```
AttemptStarted
  ↓
AttemptCompleted | AttemptCrashed
```

Attempts may produce execution artifacts (diffs, checkpoint commits).

---

## 7. Verification Loop

### 7.1 Verification Invocation

Verification is triggered automatically after every attempt.

Verification consists of:
- Automated checks
- Verifier agent evaluation

---

### 7.2 Verification Outcomes

Verification is not a single judgment. It has a defined authority hierarchy:

1. **Automated Checks (Primary Gate)**
   - Deterministic, authoritative
   - Required checks must pass
   - Cannot be overridden by verifier

2. **Verifier Agent (Advisory)**
   - LLM-based evaluation
   - Produces PASS / SOFT FAIL / HARD FAIL
   - Can trigger retry but cannot approve alone
   - May be wrong; is advisory

3. **Human Authority (Ultimate)**
   - Can override any automated decision
   - Can approve despite check failures
   - Can reject despite verifier approval

**Important:** Automated checks are the primary gate. Verifier agents provide advisory guidance. See `docs/design/verification-authority.md` for complete authority model.

---

### 7.3 Retry Policy

Retry behavior is defined per task:
- Maximum retries
- Conditions for retry (check failure, soft verifier fail)
- Escalation rules

Retry transitions are explicit and bounded.

When retrying, agents receive **explicit retry context** (not implicit memory) including prior attempt diffs, check results, and verifier feedback. Retry context is delivered alongside immutable attempt context manifest hashes so retries can reference prior manifests deterministically. See `docs/design/retry-context.md`.

---

## 8. Execution Artifacts

### 8.1 Execution Commits (Checkpoints)

During execution, TaskFlow may create **execution commits**:

- Owned by the task
- Ephemeral and rewritable
- Used for diffing, rollback, and retry

Execution commits:
- Are never merged directly
- Are discarded or archived on completion

---

### 8.2 Integration Commits

On task success:
- Merge protocol may generate one or more integration commits
- These commits represent finalized work

Integration commits are separate from TaskFlow execution.

---

## 9. Pause, Resume, and Abort

### 9.1 Pause

Pausing a TaskFlow:
- Stops scheduling new tasks
- Allows running attempts to complete or be halted

---

### 9.2 Resume

Resuming:
- Re-evaluates task eligibility
- Continues from last known state

---

### 9.3 Abort

Aborting a TaskFlow:
- Stops all scheduling
- Preserves state and artifacts
- Requires explicit human action

---

## 10. Failure Semantics

Failures are first-class outcomes.

- Failed tasks block downstream dependencies
- Failures are never hidden or auto-resolved
- Human escalation is explicit

TaskFlow does not attempt to recover silently.

---

## 11. Human Intervention

Humans may:
- Pause or resume TaskFlows
- Abort tasks
- Override retry limits
- Approve merges

Human actions emit events and override automation.

---

## 12. Observability Guarantees

TaskFlow guarantees that:

- Every state transition emits an event
- Execution history is inspectable
- Partial progress is preserved
- Debugging does not require runtime context
- Concurrency governance decisions are event-visible (conflict + defer telemetry)

---

## 13. Invariants

TaskFlow enforces the following invariants:

- TaskGraph is immutable during execution
- Verification precedes success
- Retries are bounded
- No implicit merges
- No hidden state

Breaking these invariants is considered a system error.

---

## 14. Summary

TaskFlow is:
- A deterministic executor of planned intent
- An event-driven state machine
- A safety layer over agentic execution

By separating *what should happen* from *how execution unfolds*, TaskFlow enables powerful automation without sacrificing control or trust.
