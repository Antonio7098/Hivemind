---
title: Architecture Overview
description: High-level system architecture
order: 1
---

# Hivemind Architecture

> **Principle:** Planning is deterministic. Execution is observable. Runtimes are replaceable.

This document provides a **high-level architectural overview** of Hivemind. It defines system boundaries, core responsibilities, and invariants, while deliberately deferring detailed behavior to focused specification documents.

Hivemind is designed as a *control plane* for agentic development: it coordinates planning, execution, verification, and integration without relying on hidden state or implicit behavior.

---

## How to Read This Document

This file is the **map**, not the terrain.

Detailed specifications live elsewhere:

- `docs/architecture/state-model.md` — persisted state model
- `docs/architecture/event-model.md` — event taxonomy and replay guarantees
- `docs/architecture/taskflow.md` — execution semantics and FSMs
- `docs/architecture/scope-model.md` — safety, isolation, and parallelism rules
- `docs/architecture/commit-branch-model.md` — execution vs integration commit lifecycle
- `docs/architecture/runtime-adapters.md` — runtime abstraction strategy
- `docs/architecture/cli-capabilities.md` — authoritative system interface

This document intentionally avoids duplicating those details.

---

## 1. Core Concepts & Invariants

### 1.1 Foundational Invariants

Hivemind is built on the following non-negotiable principles:

- **Projects are the top-level unit of organization**
- **Task planning is static and acyclic**
- **Task execution is dynamic and stateful**
- **Verification is explicit and authoritative**
- **Observability is the source of truth**
- **Runtimes are interchangeable compute backends**

Violating these invariants leads to non-determinism, loss of trust, or systems that cannot be debugged.

---

## 2. High-Level System Overview

At a high level, Hivemind consists of the following major subsystems:

1. **Project & Repository Registry** — long-lived organizational context
2. **Planning Layer (TaskGraph)** — static, deterministic intent
3. **Execution Layer (TaskFlow Engine)** — controlled, stateful execution
4. **Scope & Safety Enforcement** — parallelism and isolation guarantees
5. **Runtime Adapters** — replaceable execution backends
6. **Event Log & State Derivation** — single source of truth
7. **CLI Interface (Authoritative)** — automation and control surface

These subsystems communicate exclusively via **events and derived state**, never hidden memory.

---

## 3. Projects, Repositories, and Scope

### 3.1 Projects

A **Project** is a logical container defined by the user. It may include:

- One or more repositories
- Zero or more non-repo directories
- One mandatory constitution artifact managed by Hivemind governance storage
- Documentation (overviews, plans, constraints)
- TaskGraphs, TaskFlows, and free tasks

Projects are **not defined by repositories** and do not live inside them by default.

Projects are stored in a **global Hivemind project registry**, local to the user’s machine.

---

### 3.2 Repositories

- A repository may belong to **multiple projects**
- A project may span **multiple repositories**
- Repositories are treated as **resources**, not state containers

Git remains the source of truth for code history, but **not** for orchestration state or execution history.

---

### 3.3 Scope & Worktrees

Agents operate within an explicit **scope contract** that defines what they are allowed to access and modify.

Scope may include:

- Repository access
- Filesystem paths
- Read vs write permissions
- Execution and git capabilities
- Worktree isolation requirements

Scope is a **first-class safety mechanism**.

Parallel execution decisions, worktree isolation, and retry safety are all derived from explicit scope compatibility rules. Scope violations are fatal to the current attempt and always observable via events.

Detailed scope semantics are defined in `scope-model.md`.

---

## 4. Planning Layer: TaskGraph

### 4.1 TaskGraph Definition

The **TaskGraph** is a static, immutable **Directed Acyclic Graph (DAG)** produced by the Planner.

It represents **intent**, not execution.

Each task node defines:

- Description and objective
- Success criteria (machine-checkable and/or natural language)
- Verifier definition
- Retry policy
- Required scope

Edges define **hard dependencies only**.

Once execution begins, the TaskGraph does not change.

---

### 4.2 Planner Responsibilities

The Planner:

- Translates user intent into tasks and dependencies
- Assigns scopes and detects conflicts
- Determines safe parallelism
- Optionally recommends worktree isolation

The Planner **does not execute tasks** and **does not participate in retries**.

Once a TaskFlow starts, the Planner’s role is complete.

---

## 5. Execution Layer: TaskFlow Engine

### 5.1 TaskFlow

A **TaskFlow** is a runtime instance of a TaskGraph.

It binds:

- Tasks
- Agents
- Runtimes
- Scopes and worktrees
- Execution state

Multiple TaskFlows may exist for the same TaskGraph.

---

### 5.2 Execution Model

TaskFlow execution consists of two cooperating mechanisms:

1. **Graph Scheduler** — releases tasks only when dependencies succeed
2. **Per-Task Execution State Machine** — manages attempts, verification, retries, and failure

Execution behavior is fully event-driven.

---

### 5.3 Task Execution Semantics

Each task executes as a finite state machine with bounded retries.

Verifier-to-worker transitions are **local execution loops**, not graph edges. This preserves DAG correctness while allowing controlled iteration.

Detailed execution semantics are defined in `taskflow.md`.

---

## 6. Agents

### 6.1 Agent Roles

Hivemind defines explicit agent roles:

- **Planner Agent** — creates TaskGraphs
- **Worker Agents** — perform task execution
- **Verifier Agents** — evaluate outcomes
- **Merge Agents** — prepare and apply integration commits
- **Freeflow Agents** — unscoped, conversational

Agents are instantiated per attempt and have **no implicit memory**.

---

### 6.2 Agent Context

Agents receive only explicit context:

- Task definition
- Frozen governance context manifest (constitution, system prompt, skills, documents, graph summary)
- Prior attempt summaries
- Verifier feedback

Hidden runtime context is never relied upon.

---

## 7. Execution Artifacts

Task execution produces explicit, inspectable artifacts:

- **Execution branches** (one per task by default)
- **Checkpoint (execution) commits**
- **Diffs and filesystem changes**

These artifacts are:
- Owned by the task
- Ephemeral and rewritable
- Never merged directly

Integration commits are created only via the merge protocol.

The full lifecycle is defined in `commit-branch-model.md`.

---

## 8. Runtime Architecture

### 8.1 Runtime Definition

A **Runtime** is an execution backend (e.g. Claude Code, Codex CLI, OpenCode).

Runtimes are treated as:

> Non-deterministic compute oracles with observable side effects

---

### 8.2 Wrapper-Based Runtimes (Initial Sprint)

In the wrapper model, Hivemind:

- Launches runtime CLIs
- Executes them in scoped worktrees
- Observes filesystem and process behavior
- Derives diffs and execution commits mechanically

This approach prioritizes leverage, speed, and replaceability.

---

### 8.3 Native Runtime (Future, Optional)

A native runtime may later:

- Own prompt assembly
- Produce structured patch objects
- Enable AST-aware verification

This evolution does **not** change TaskFlow, state, events, scope, or commit semantics.

---

## 9. State, Events, and Observability

Hivemind is **event-native**.

- Every meaningful transition emits an event
- All state is derived from the event log
- Execution history is replayable and auditable
- Operators can query governance-specific event slices by artifact/template/rule identifiers
- Governance diagnostics are explicit CLI outputs (missing artifacts, invalid references, stale snapshots)
- Governance replay/snapshot/repair workflows are CLI-first with explicit confirmation boundaries and recovery telemetry

The event log is the **single source of truth**.

Detailed models are defined in `state-model.md` and `event-model.md`.

---

## 10. Merge Protocol

Merging is a **first-class, explicit operation**.

By default:

- No automatic merges
- Human-in-the-loop approval

Merge agents may assist, but integration is never a side effect of task success.

---

## 11. Interfaces

### 11.1 CLI (Authoritative)

The CLI is the **authoritative interface**:

- All functionality is CLI-accessible
- All state transitions are explicit
- Commands are scriptable and automatable

Because of this, agents may operate **Hivemind itself**, enabling automation, recovery, and meta-orchestration.

---

### 11.2 UI (Secondary)

The UI is a projection over CLI-accessible state.

It provides visualization and interaction but owns no behavior or state.

---

## 12. Evolution Strategy

Hivemind evolves safely by design:

1. Wrapper runtimes first
2. Observability-driven learning
3. Incremental interception
4. Native runtime only when justified

This ensures progress without architectural drift.

---

## 13. Summary

Hivemind is not an agent framework.

It is a **deterministic planning system with controlled, observable execution**.

The architecture prioritizes:

- Trust
- Safety
- Debuggability
- Human authority
- Long-term evolution

Everything else is secondary.
