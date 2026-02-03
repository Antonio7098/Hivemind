# Hivemind Architecture

> **Principle:** Planning is deterministic. Execution is observable. Runtimes are replaceable.

This document describes the high-level architecture of **Hivemind**, focusing on system boundaries, state ownership, execution semantics, and the trade‑offs between wrapper-based and native runtimes. It intentionally avoids premature low-level design while being explicit about responsibilities and invariants.

---

## 1. Core Concepts & Invariants

### 1.1 Foundational Invariants

Hivemind is built on the following non-negotiable principles:

* **Projects are the top-level unit of organization**
* **Task planning is static and acyclic**
* **Task execution is dynamic and stateful**
* **Verification is explicit and authoritative**
* **Observability is the source of truth**
* **Runtimes are interchangeable compute backends**

Violating any of these leads to non-determinism, loss of trust, or un-debuggable systems.

---

## 2. High-Level System Overview

At a high level, Hivemind consists of five major subsystems:

1. **Project & Repo Registry**
2. **Planning Layer (TaskGraph)**
3. **Execution Layer (TaskFlow Engine)**
4. **Runtime Adapters**
5. **Observability & Event System**

These systems communicate exclusively through **events and persisted state**, never hidden memory.

---

## 3. Projects, Repositories, and Scope

### 3.1 Project

A **Project** is a logical container defined by the user. It may include:

* One or more repositories
* Zero or more non-repo directories
* Documentation (overviews, plans, constraints)
* TaskFlows and free tasks

Projects are **not defined by repositories** and do not live inside them by default.

Projects are stored in a **global Hivemind project registry**, local to the user’s machine.

### 3.2 Repositories

* A repository may belong to **multiple projects**
* A project may span **multiple repositories**
* Repositories are always treated as **read/write resources**, never as state containers

Git remains the source of truth for code history, but **not** for task or execution state.

### 3.3 Scope & Worktrees

Agents operate within an explicit **scope contract**, which may include:

* Repository paths
* Subdirectories
* Worktrees
* Read-only vs read-write permissions

Overlapping scopes trigger:

* Warnings
* Planner intervention
* Automatic worktree creation (when configured)

Scope is enforced by the execution engine, not by agent goodwill.

---

## 4. Planning Layer: TaskGraph

### 4.1 TaskGraph Definition

The **TaskGraph** is a static, immutable **Directed Acyclic Graph (DAG)** produced by the Planner.

It represents **intent**, not execution.

Each node (Task) defines:

* Description & objective
* Success criteria (machine + natural language)
* Verifier definition
* Retry policy
* Required scope/worktree

Edges define **hard dependencies** only.

Once execution begins, the TaskGraph does not change.

### 4.2 Planner Responsibilities

The Planner:

* Translates user intent into tasks and dependencies
* Resolves scope conflicts
* Decides when parallelism is safe
* Optionally creates worktrees

The Planner **does not execute tasks** and **does not participate in retries**.

Once the TaskFlow starts, the Planner’s role is complete.

---

## 5. Execution Layer: TaskFlow Engine

### 5.1 TaskFlow

A **TaskFlow** is a runtime instance of a TaskGraph.

It binds:

* Tasks
* Agents
* Runtimes
* Worktrees
* State

Multiple TaskFlows may exist for the same TaskGraph.

### 5.2 Execution Model

TaskFlow execution consists of two interacting mechanisms:

1. **Graph Scheduler**

   * Releases tasks only when dependencies succeed
2. **Per-Task State Machine**

   * Manages attempts, verification, retries, and failure

### 5.3 Task Execution State Machine

Each task runs as a finite state machine:

```
PENDING
  ↓
RUNNING (Worker Agent)
  ↓
VERIFYING (Verifier Agent)
  ↓
SUCCESS
    OR
RETRY → RUNNING
    OR
FAILED
    OR
ESCALATED (Human)
```

**Verifier → Worker is not a graph edge.**
It is a **local retry transition** within the task’s execution state.

Retry limits, escalation rules, and failure conditions are enforced by the engine.

---

## 6. Agents

### 6.1 Agent Types

Hivemind defines explicit agent roles:

* **Planner Agent** – creates TaskGraphs
* **Worker Agents** – perform tasks
* **Verifier Agents** – validate outcomes
* **Merge Agent** – proposes and/or applies merges
* **Freeflow Agent** – unscoped, conversational

Agents are instantiated per task attempt and have **no implicit memory**.

### 6.2 Agent Context

Agents receive:

* Task definition
* Relevant project docs
* Prior attempt summaries
* Explicit verifier feedback

They do **not** receive hidden runtime context.

---

## 7. Runtime Architecture

### 7.1 Runtime Definition

A **Runtime** is an execution backend (e.g. Claude Code, Codex CLI, OpenCode).

Runtimes are treated as:

> Non-deterministic compute oracles with observable side effects

### 7.2 Wrapper-Based Runtime (MVP)

In the wrapper model, Hivemind:

* Launches runtime CLIs
* Sandboxes them in scoped directories/worktrees
* Observes file system changes
* Captures diffs via git
* Logs tool invocations and outputs

Undo and rollback are performed using git resets or worktree disposal.

This approach prioritizes speed and leverage over semantic precision.

### 7.3 Native Runtime (Future)

A native runtime would:

* Own prompt assembly
* Route tools explicitly
* Represent edits as patch objects
* Enable atomic, reversible changes
* Support AST-aware verification

This is intentionally deferred until TaskFlow semantics are proven.

---

## 8. State Model

### 8.1 What State Is

State is everything that is:

* Observable
* Persisted
* Replayable
* Auditable

### 8.2 What State Is Not

* Hidden model context
* Chain-of-thought
* Ephemeral runtime memory

### 8.3 Persisted State Includes

* TaskGraph
* TaskExecution states
* Attempts
* Agent inputs/outputs
* File diffs
* Tool calls
* Verification results
* Human interventions

State is append-only and event-backed.

---

## 9. Observability & Events

Hivemind is event-native.

Every meaningful transition emits an event:

* Task started
* Attempt completed
* Verification failed
* Retry scheduled
* Diff applied
* Task succeeded/failed

Events are:

* Structured
* Persisted
* Replayable

Stageflow provides:

* Event taxonomy
* DAG scheduling
* Replay and inspection

---

## 10. Merge Protocol

Merging is a **first-class, explicit step**.

By default:

* No automatic merge
* Human-in-the-loop

Optional merge agents may:

* Propose patches
* Run checks
* Require approval

Merges are never implicit side effects of task success.

---

## 11. Interfaces

### 11.1 CLI (Primary)

* CLI is the authoritative interface
* All functionality is CLI-addressable
* UI is a projection of CLI state

### 11.2 UI (Secondary)

UI provides:

* TaskGraph visualization
* TaskFlow execution view
* Diffs and history
* Manual interventions

UI never owns state.

---

## 12. Evolution Strategy

Hivemind is designed to evolve safely:

1. Wrapper runtimes first
2. Observability-driven learning
3. Hybrid interception
4. Native runtime (only when justified)

This ensures momentum without architectural debt.

---

## 13. Summary

Hivemind is not an “agent framework”.

It is a **deterministic planning system with controlled, observable execution**.

The architecture prioritizes:

* Trust
* Debuggability
* Human authority
* Long-term evolution

Everything else is secondary.
