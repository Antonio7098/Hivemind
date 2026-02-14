---
title: Commit & Branch Model
description: Git integration and branch management
order: 3
---

# Hivemind — Commit & Branch Model (commits.md)

> **Core principle:** Commits are execution artifacts first, version-control history second.

This document defines how Hivemind uses **Git commits and branches** to support observability, diffs, undo/redo, retries, and safe integration — without polluting project history or over-constraining user workflows.

This is a **conceptual model**, not a Git implementation guide.

---

## 1. Why Git Is Part of the Execution Model

Hivemind intentionally leverages Git because it already provides:
- Content-addressed snapshots
- Cheap branching
- Diffing and history
- Reversible operations

However, Git is used as a **mechanical substrate**, not as the source of orchestration truth.

---

## 2. The Two Classes of Commits

Hivemind distinguishes **two fundamentally different commit types**.

### 2.1 Execution Commits (Checkpoint Commits)

Execution commits are:
- Created *during* task execution
- Owned by a single task
- Ephemeral and rewritable
- Never merged directly

They exist to:
- Capture incremental progress
- Enable fine-grained diffs
- Support undo/redo and retries

Execution commits are part of **TaskExecution state**, not project history.

---

### 2.2 Integration Commits

Integration commits are:
- Created *after* task success
- Owned by the merge protocol
- Human-approved by default
- The only commits allowed onto long-lived branches

Integration commits represent finalized intent.

---

## 3. Branching Model: The Real Question

You asked the right question:

> *Do we have one branch per task? One per TaskFlow? Is this too opinionated?*

The honest answer:

> **There is no way to get clean diffs, undo, retries, and observability without isolating execution somehow.**

Branches are the *least-bad* isolation mechanism we have.

---

## 4. Recommended Default: One Execution Branch per Task

### 4.1 The Default Model

By default:

- Each **task** executes on its own **execution branch**
- Branches are created from the TaskFlow base revision
- Execution commits live only on that branch

Example naming (illustrative):
```
exec/taskflow-42/task-add-auth
```

---

### 4.2 Why This Works

This model gives you:

- Perfect diff attribution (task ↔ commits)
- Independent retries per task
- Safe parallelism
- Clear failure isolation

It aligns directly with:
- TaskExecution FSM
- Scope enforcement
- Execution commit ownership

---

## 5. Is One Branch per Task “Too Much”? (Honest Analysis)

### Costs

- More branches (ephemeral)
- Slight conceptual overhead
- Requires branch lifecycle management

### Benefits

- Deterministic diffs
- Trivial undo (drop branch)
- Clean retries (reset branch)
- No accidental cross-task coupling

Given Hivemind’s goals (trust, observability, safety), **this is the correct tradeoff**.

---

## 6. Alternative: One Branch per TaskFlow (Why It Breaks)

A single execution branch per TaskFlow:

### Pros
- Fewer branches
- Simpler Git surface

### Cons (Severe)
- Interleaved diffs across tasks
- Impossible to attribute changes cleanly
- Retry rollback affects unrelated tasks
- Verification ambiguity

This model fundamentally conflicts with:
- Per-task retries
- Parallel execution
- Scope isolation

It is not recommended beyond trivial flows.

---

## 7. Checkpoints Within a Task

Within a task execution branch:

- Execution commits are created at **checkpoints**
- Checkpoints may be:
  - Tool-defined
  - Verifier-requested
  - Time-based

Checkpoints allow:
- Partial verification
- Targeted rollback
- High-resolution diffs

---

## 8. Retry Semantics with Branches

On retry:

- Branch is reset to last accepted checkpoint
- New execution commits overwrite prior ones
- Failed attempts remain visible via events

No retry ever mutates another task’s branch.

---

## 9. Merging Execution Results

Execution branches are **never merged directly**.

On task success:

- Merge protocol:
  - Selects execution commits
  - Squashes, splits, or edits as needed
  - Produces integration commit(s)

Integration commits may be:
- One per task (default)
- Multiple (advanced, explicit)

---

## 10. Multi-Repo TaskFlows

In multi-repo projects:

- Each repo gets its own execution branch per task
- Task success is atomic at TaskFlow level
- Integration commits are created per repo

This avoids cross-repo Git coupling.

---

## 11. Branch Lifecycle

Execution branches:
- Created at task start
- Updated during execution
- Archived or deleted on completion

Retention policy is configurable.

Branches are machine-managed and not user-editable.

---

## 12. Opinionated by Design (and Why That’s OK)

Yes — this model is opinionated.

But that opinionation buys:
- Safety
- Predictability
- Debuggability
- User trust

Hivemind is not trying to be a general Git UI.
It is an **agent orchestration system**.

---

## 12. Execution Surface Ownership (Human vs Agent)

During task execution, ownership of the execution surface is **explicit**.

### 12.1 Ownership Rule

While a task is in a RUNNING or VERIFYING state:

- Its execution branch and worktree are **owned by the TaskFlow engine**
- Agent-driven changes are the only permitted mutations
- Human edits to the same execution surface are not supported by default

This rule exists to preserve:
- unambiguous diffs
- deterministic retries
- verifier correctness
- clear attribution of changes

### 12.2 Human Editing During Execution

Human edits must occur in one of the following locations:

- the main project branch
- a separate user-managed branch
- a separate worktree outside the task’s execution scope

Direct human edits to an active execution branch are considered unsafe.

### 12.3 Violation Handling

If external (human) modifications are detected within an execution branch:

- An explicit `ExternalModificationDetected` event is emitted
- The current attempt is marked invalid
- The task may:
  - fail immediately, or
  - escalate to human intervention (policy-dependent)

Silent interleaving of human and agent edits is never permitted.

### 12.4 Rationale

Allowing uncontrolled human edits during agent execution introduces:

- ambiguous diffs
- non-replayable state
- unverifiable outcomes
- corrupted retry semantics

Hivemind prioritizes correctness and trust over convenience.

---

## 13. Invariants

The commit & branch model enforces:

- Execution commits never hit main
- Tasks own their branches
- Integration commits are explicit
- Undo is mechanical
- History remains clean

Violating these invariants is a system error.

---

## 14. Summary

Hivemind’s commit model intentionally:
- Uses **one execution branch per task** by default
- Treats commits as execution artifacts
- Separates execution from integration

This is not excess — it is the minimum structure required to make agentic work observable, reversible, and safe.

