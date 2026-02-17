---
title: State Model
description: Persisted state and data structures
order: 8
---

# Hivemind — State Model (state.md)

> **Core rule:** If Hivemind cannot observe it, persist it, and replay it, it is *not* state.

This document defines what **state** means in Hivemind, what is explicitly *not* state, and how state is structured to support observability, retries, debugging, and long‑running agentic workflows.

This is a **conceptual contract**, not a storage schema.

---

## 1. Why State Matters in Hivemind

Hivemind is not a chat system. It is an **orchestration system**.

That means:
- Execution may span minutes, hours, or days
- Failures are expected
- Humans may intervene mid‑flow
- Runtimes may change or be replaced

To support this, Hivemind must have a **precise, minimal, trustworthy notion of state**.

State enables:
- Pause / resume
- Retry without corruption
- Auditability
- Explainability (“why did this happen?”)
- Reproducible reasoning about failures

---

## 2. The Prime Distinction: Observed vs Hidden

### 2.1 Observed State (Hivemind State)

State is **only** what Hivemind can:
- Observe directly
- Persist explicitly
- Replay deterministically

If any of these are false, the data does not qualify as state.

---

### 2.2 Hidden State (Explicitly Excluded)

The following are **not state** and must never be relied upon:

- Model context windows
- Internal runtime memory
- Chain‑of‑thought
- Implicit tool memory inside runtimes
- Unlogged environment mutations

These are treated as **non‑deterministic inputs**.

Hivemind must function correctly even if they are lost.

---

## 3. Layers of State

Hivemind state is layered. Each layer has different mutability rules.

```
Project State
 ├─ TaskGraph State (static)
 │   └─ Task Definitions
 └─ TaskFlow State (dynamic)
     ├─ TaskExecution State (per task)
     │   └─ Attempt State
     └─ Execution Artifacts
```

---

## 4. Project State

### Definition

Project state represents **long‑lived organizational context**.

### Includes
- Project identity
- Associated repositories
- Project‑level configuration
- Project constitution projection metadata (digest, schema/version, last updated timestamp)
- Linked documentation
- Task registry

### Properties
- Mutable over time
- Independent of execution
- Does not encode progress

Project state answers:
> “What is this project?”

---

## 5. TaskGraph State (Static)

### Definition

The **TaskGraph** represents planned intent.

It is created by the Planner and is **immutable once execution begins**.

### Includes
- Task nodes
- Dependencies (edges)
- Success criteria
- Retry policies
- Required scopes

### Properties
- Static
- Acyclic
- Versioned

TaskGraph state answers:
> “What was supposed to happen?”

---

## 6. TaskFlow State (Dynamic)

### Definition

A **TaskFlow** is a runtime instance of a TaskGraph.

Multiple TaskFlows may exist for the same TaskGraph.

### Includes
- TaskFlow identity
- Start / pause / resume timestamps
- Global execution status
- Event log reference

TaskFlow state answers:
> “What is happening (or happened) during this run?”

---

## 7. TaskExecution State (Per Task)

### Definition

Each task node has its own execution state machine.

### States

```
PENDING
RUNNING
VERIFYING
SUCCESS
RETRY
FAILED
ESCALATED
```

### Includes
- Current state
- Attempt count
- Verifier outcomes
- Blocking reason (if any)

### Properties
- Mutable
- Event‑driven
- Fully replayable

TaskExecution state answers:
> “What happened to *this* task?”

---

## 8. Attempt State

### Definition

An **Attempt** represents a single worker execution.

Attempts are **ephemeral but recorded**.

### Includes
- Attempt ID
- Agent identity
- Runtime used
- Start / end timestamps
- Exit status
- Structured output summary

### Properties
- Append‑only
- Never mutated
- Used for debugging and audit

Attempt state answers:
> “What did the agent try this time?”

---

## 9. Execution Artifacts

Execution artifacts are **materialized side effects**.

They are first‑class state.

### Includes

#### 9.1 Execution Commits (Checkpoints)
- Git commits created during task execution
- Owned by the task
- Ephemeral and rewritable
- Never merged directly

Used for:
- Fine‑grained diffs
- Rollback
- Retry from known points

---

#### 9.2 Integration Commits
- Created by the merge protocol
- Represent finalized work
- Only commits allowed onto main branches

---

#### 9.3 Diffs
- Derived from execution commits or filesystem snapshots
- Always attributable to:
  - Task
  - Attempt
  - Scope

---

## 10. Verification State

### Definition

Verification state captures **judgment**, not execution.

### Includes
- Automated check results
- Verifier agent decision
- Failure reasons
- Confidence level (optional)

### Properties
- Immutable per attempt
- Authoritative

Verification state answers:
> “Was this correct, and why?”

---

## 11. Human Intervention State

### Definition

Human actions are explicit state transitions.

### Includes
- Manual pause / resume
- Retry overrides
- Merge approvals
- Task aborts

Human actions are:
- Persisted
- Audited
- Replayable

---

## 12. Event Log (The Spine)

All state transitions are driven by **events**.

### Event properties
- Append‑only
- Ordered
- Timestamped
- Attributed

**Important:** State is derived from the event log, but events reference execution artifacts (files, commits) without containing them. The event log is the source of truth for *what happened*, while artifacts are the source of truth for *what was produced*.

The event log is the **single source of truth for orchestration state**.

---

## 13. What State Enables (Explicitly)

Because of this model, Hivemind can:
- Pause and resume TaskFlows safely
- Retry tasks without corruption
- Inspect full execution history
- Explain failures precisely
- Swap runtimes without losing correctness

---

## 14. What Hivemind Refuses to Do

Hivemind explicitly refuses to:
- Persist hidden runtime memory
- Reconstruct model context
- Depend on implicit agent state
- Hide partial execution

This is a deliberate design choice.

---

## 15. Summary

Hivemind’s state model is:
- Minimal
- Explicit
- Observable
- Replayable

It treats runtimes as **non‑deterministic executors** and elevates **effects and decisions** to first‑class state.

This discipline is what makes long‑running, agentic workflows trustworthy.
