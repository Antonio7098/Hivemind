---
title: PRD v0
description: Product requirements document v0
order: 5
---

# Hivemind — Product Requirements Document (PRD)

## 1. Vision

Hivemind is a **local-first, observable orchestration system for agentic software development**.

It enables solo developers to manage projects, tasks, and automated agent workflows with the same rigor, safety, and transparency as high-performing engineering teams — without surrendering control to opaque AI behavior.

Hivemind prioritizes:
- **Truth through observability**
- **Explicit structure over implicit magic**
- **Human authority at every critical boundary**

---

## 2. Goals

### Primary Goals
- Enable safe parallel agent execution on real codebases
- Make agent behavior observable, inspectable, and debuggable
- Support both lightweight task tracking and heavyweight automated workflows
- Work entirely **locally** using existing CLI-based agent runtimes

### Non-Goals (v0.x)
- Hosting or training models
- Cloud-first execution
- Fully autonomous code merging by default

---

## 3. Core Concepts

### 3.1 Project

A **Project** is the top-level organizational unit in Hivemind.

- Defined and owned by Hivemind (not Git)
- Represents a logical body of work
- May reference **one or many Git repositories**
- Holds:
  - Tasks
  - TaskFlows
  - Docs / context
  - Automations

Projects are defined in a **Hivemind project registry** outside of any repo.

---

### 3.2 Repository

A **Repository** is a versioned code source (e.g. Git).

- Repos may belong to **multiple projects**
- Project–repo relationships are explicit
- Each project defines its own permissions (read-only / read-write)

Repos are inputs to Hivemind, not owners of orchestration state.

---

### 3.3 Task

A **Task** is a unit of intent.

- May exist without automation
- May be tracked as a simple todo
- May optionally be executed by agents

A task with no assigned agent has **no execution semantics**.

---

### 3.4 TaskFlow

A **TaskFlow** is an optional, heavyweight execution construct.

It is:
- A deterministic, executable document
- A DAG of tasks and dependencies
- Fully observable and pausable

TaskFlow is used when guarantees are required:
- Parallel agents
- Verification
- Retries
- Auditability

---

### 3.5 Agents

Agents are execution actors.

Roles:
- **Planner Agent** — creates tasks, dependencies, scopes (outside execution)
- **Worker Agent** — performs work
- **Verifier Agent** — evaluates correctness and completion
- **Merge Agent** — prepares and governs merges (optional)
- **Freeflow Agent** — unscoped, non-project chat

Agents:
- Never own state
- Never decide structure mid-execution
- Always operate within explicit constraints

---

### 3.6 Scope

A **Scope** is a capability contract between an agent and a project.

Scopes define:
- Filesystem read/write permissions
- Execution permissions
- Git permissions

Scopes are:
- Declared before execution
- Enforced during execution
- Verified after execution

Scopes enable safe parallelism and clear accountability.

---

### 3.7 Worktrees

Worktrees are ephemeral execution surfaces.

- Created per TaskFlow and per repo
- Isolated when required by scope conflicts
- Shared when safe

Worktrees are a consequence of scopes, not a primary abstraction.

---

## 4. Execution Model

### 4.1 Planning Sprint (Outside Execution)

- Planner agent converts intent into:
  - Tasks
  - Subtasks
  - Dependencies
  - Scopes
- Output is an **immutable TaskFlow plan**

Once execution begins, the planner exits.

---

### 4.2 TaskFlow Execution

Each task follows a state lifecycle:

```
WAITING → RUNNING → VERIFYING → { SUCCESS | RETRY | FAILED }
```

- Tasks execute when dependencies are satisfied
- Parallelism is governed by scope compatibility

---

### 4.3 Verification Loop

When a task completes:
1. Automated checks run (tests, linters, etc.)
2. Verifier agent evaluates:
   - Diffs
   - Test results
   - Natural language criteria

Verifier outcomes:
- **PASS** → task completes
- **SOFT FAIL** → worker recalled with instructions
- **HARD FAIL** → task aborted

Retry count and authority are configurable per task.

---

### 4.4 Merge Protocol

Merging is **explicit and separate from execution**.

Defaults:
- No auto-merge
- User approval required

Optional:
- Merge agent may prepare PRs
- Auto-merge can be enabled per project or task

---

## 5. Observability

Observability is a first-class requirement.

Hivemind records:
- Agent actions
- Scope violations
- Task state transitions
- Diffs and file changes
- Verification outcomes

All execution is replayable and auditable.

---

## 6. Runtime Strategy

Hivemind is **CLI-first**.

- Uses existing agent runtimes:
  - Claude Code
  - OpenCode
  - Codex CLI
  - Gemini CLI
- Hivemind wraps and intercepts these tools
- Runtime configuration is abstracted behind Hivemind

Native runtime support may be added later.

---

## 7. Storage & Source of Truth

- Projects, TaskFlows, and orchestration state are owned by Hivemind
- Code remains owned by Git
- Hivemind artifacts may be stored:
  - In a central registry
  - And/or mirrored into repos for portability

No hidden state.

---

## 8. UI & UX Principles

- CLI is the primary interface for correctness
- UI is a projection of system state

Key views:
- Project overview
- Task list / Kanban
- TaskFlow document view
- Dependency graph
- Diff & verification views

TaskFlows may be viewed as living documents (similar to sprint plans).

---

## 9. User Stories

### US1: Simple Todo Tracking
A user creates tasks in a project with no agents attached. Tasks behave as a standard todo list.

---

### US2: Manual Agent Assistance
A user runs a worker agent on a single task, reviews diffs, and manually applies changes.

---

### US3: Structured TaskFlow
A user creates a TaskFlow with multiple tasks, dependencies, and verification, and runs it end-to-end.

---

### US4: Verification Failure & Retry
A verifier detects failure, recalls the worker with instructions, and retries until success or abort.

---

### US5: Parallel Scoped Agents
Multiple agents run in parallel on the same repo with non-overlapping scopes.

---

### US6: Scope Conflict Handling
Overlapping write scopes trigger warnings and optional worktree isolation.

---

### US7: Multi-Repo Project Execution
A TaskFlow safely orchestrates changes across backend and frontend repos.

---

### US8: Shared Repo Across Projects
A shared repo is used read-only in one project and read-write in another without conflict.

---

### US9: Automation
A TaskFlow is triggered on a schedule and reports results.

---

### US10: Pause & Resume
A failed TaskFlow is resumed from the point of failure after intervention.

---

## 10. Open Questions (Future Work)

- Native runtime implementation
- Advanced conflict resolution across projects
- Collaborative / multi-user support
- Distributed execution

---

## 11. Success Criteria

Hivemind succeeds if:
- Users trust what agents do
- Failures are explainable
- Parallelism feels safe
- The system remains usable without automation

---

**Hivemind is not an AI that codes.**  
**It is a system that makes agentic work legible, governable, and real.**

