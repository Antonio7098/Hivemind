---
title: CLI Capabilities
description: Command-line interface specification
order: 2
---

# Hivemind — CLI Capability Specification (cli.md)

> **Core principle:** If it cannot be done via the CLI, it is not a real feature.

This document defines the **capabilities** exposed by the Hivemind CLI. It intentionally avoids concrete flag syntax in favor of **semantic operations**. The CLI is the authoritative interface; all UI layers are projections over these capabilities.

This design enables:
- Automation-first workflows
- Agent-driven operation of Hivemind itself
- Deterministic testing and iteration
- Headless execution (CI, cron, other agents)

---

## 1. CLI Design Principles

1. **Completeness** — all functionality must be CLI-accessible
2. **Scriptability** — commands produce machine-readable output
3. **Idempotence** — repeated commands are safe where possible
4. **Explicitness** — no hidden defaults, no magic side effects
5. **Composability** — commands can be chained by humans or agents

---

## 2. Global Capabilities

These commands operate across projects and flows.

### 2.1 System Introspection

Capabilities:
- Show Hivemind version
- Show active runtimes
- Show adapter health
- Show global event stream

Purpose:
- Debugging
- Agent self-awareness

---

### 2.2 Event Access

Capabilities:
- Stream events (live)
- Query historical events by correlation (project / graph / flow / task / attempt)
- Query events by time window (`--since`, `--until`)
- Inspect full event payload by event ID
- Replay events into derived state

Purpose:
- Observability
- Failure analysis
- Deterministic testing

---

## 3. Project Management

### 3.1 Project Lifecycle

Capabilities:
- Create project
- List projects
- Inspect project
- Update project metadata
- Archive project

---

### 3.2 Repository Management

Capabilities:
- Attach repository to project
- Detach repository from project
- Set repository access mode (RO / RW)
- List repositories in project

---

### 3.3 Project Context

Capabilities:
- Attach documentation
- View project overview
- Validate project configuration

---

### 3.4 Project Governance Artifacts

Capabilities:
- Initialize/migrate/inspect governance storage for a project
- Initialize/show/validate/update project constitution via `hivemind constitution ...`
- Manage project documents (`create`, `list`, `inspect`, `update`, `delete`) with immutable revisions
- Explicitly include/exclude project documents for task execution context
- Manage project notepad (`create`, `show`, `update`, `delete`) as non-executional context

Constraints:
- Exactly one constitution is allowed per project (canonical `constitution.yaml`)
- Constitution mutations require explicit confirmation and actor-attributed audit metadata
- Project document attachment is explicit and task-scoped
- Project notepad content is never injected into runtime prompts by default

---

## 4. Task Management (Lightweight)

### 4.1 Task Lifecycle

Capabilities:
- Create task
- Update task
- List tasks
- Inspect task
- Close task

Tasks may exist without execution semantics.

---

### 4.2 Task ↔ Agent Association

Capabilities:
- Assign agent to task
- Unassign agent
- Run task manually

---

## 5. TaskGraph & Planning

### 5.1 TaskGraph Creation

Capabilities:
- Create TaskGraph
- Add tasks to graph
- Add dependencies
- Assign scopes
- Validate TaskGraph

---

### 5.2 Planner Invocation

Capabilities:
- Invoke planner agent
- Review proposed TaskGraph
- Accept or reject plan

Planner runs are explicit and non-executing.

---

## 6. TaskFlow Execution

### 6.1 TaskFlow Lifecycle

Capabilities:
- Create TaskFlow from TaskGraph
- Start TaskFlow
- Tick TaskFlow deterministically (`flow tick`)
- Override per-tick scheduling width (`flow tick --max-parallel`)
- Set flow run mode (`flow set-run-mode manual|auto`)
- Add inter-flow dependencies (`flow add-dependency`)
- Pause TaskFlow
- Resume TaskFlow
- Abort TaskFlow
- Inspect TaskFlow state

---

### 6.2 Task-Level Control

Capabilities:
- Inspect task execution state
- Set task run mode (`task set-run-mode manual|auto`)
- Retry task
- Abort task
- Escalate task to human

---

### 6.3 Attempt Inspection

Capabilities:
- List attempts
- Inspect attempt details
- View agent output

---

### 6.4 Worktree Management

Capabilities:
- List worktrees for a flow
- Inspect worktree for a task
- Clean up worktrees for a flow

Isolation guarantee:
- Each task attempt executes in an isolated worktree/branch owned by the flow engine

---

### 6.5 Concurrency Governance

Capabilities:
- Configure per-project concurrency policy (`project runtime-set --max-parallel-tasks`)
- Enforce optional global concurrency cap (`HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL`)
- Schedule multiple compatible tasks within a single tick
- Emit conflict/defer telemetry when scope policy limits scheduling

---

## 7. Verification & Retry

### 7.1 Verification Control

Capabilities:
- Inspect verification results
- Override verifier decision (human)
- Adjust retry limits (human)

---

## 8. Scope & Safety

### 8.1 Scope Inspection

Capabilities:
- Inspect assigned scopes
- Validate scope compatibility

---

### 8.2 Conflict Handling

Capabilities:
- Acknowledge soft conflicts
- Force isolation
- Override planner recommendations

---

## 9. Execution Commits & Diffs

### 9.1 Execution Branch Access

Capabilities:
- List execution branches
- Inspect execution branch
- Checkout execution branch (read-only)

---

### 9.2 Checkpoints & Diffs

Capabilities:
- List checkpoints
- Show diff for checkpoint
- Compare checkpoints

---

### 9.3 Undo & Rollback

Capabilities:
- Reset task to checkpoint
- Discard execution branch

---

## 10. Merge & Integration

### 10.1 Merge Preparation

Capabilities:
- Prepare integration commit(s)
- View merge summary

---

### 10.2 Merge Approval

Capabilities:
- Approve merge
- Reject merge
- Apply merge locally or via PR automation (`merge execute --mode local|pr`)
- Monitor CI and request auto-merge for PR mode

---

## 11. Runtime Control

### 11.1 Runtime Selection

Capabilities:
- Select runtime defaults globally (`runtime defaults-set`)
- Select runtime per project and role (`project runtime-set --role`)
- Select runtime per flow and role (`flow runtime-set --role`)
- Override runtime per task
- Override runtime per task and role (`task runtime-set --role`)
- Inspect runtime status with role-aware resolution (`runtime health --role --project/--flow/--task`)

---

### 11.2 Adapter Diagnostics

Capabilities:
- Inspect adapter logs
- Test adapter health

---

### 11.3 Interactive Runtime Sessions

Interactive runtime sessions are deprecated.

Capabilities:
- `flow tick --interactive` is retained only for compatibility and returns `interactive_mode_deprecated`
- Runtime input/interrupt event types remain in the event model for historical replay compatibility

Constraints:
- Non-interactive `flow tick` is the only supported execution mode
- The system of record remains events and derived state, not chat transcripts

---

## 12. Governance Registry (Global)

### 12.1 Skills

Capabilities:
- Create/list/inspect/update/delete global skill artifacts

### 12.2 System Prompts

Capabilities:
- Create/list/inspect/update/delete global system prompt artifacts

### 12.3 Templates

Capabilities:
- Create/list/inspect/update/delete global templates
- Instantiate template references for a target project
- Emit observable template instantiation lifecycle events with resolved IDs

### 12.4 Global Notepad

Capabilities:
- Create/show/update/delete global notepad content

Constraints:
- Global notepad is non-executional and non-validating

---

## 13. Automation & Scheduling

### 13.1 Automation Management

Capabilities:
- Attach automation to TaskFlow
- List automations
- Disable automation

---

### 13.2 Manual Triggering

Capabilities:
- Trigger automated TaskFlow manually

---

## 14. Agent Operating Hivemind (Meta-Operation)

Hivemind explicitly supports **agents operating Hivemind itself**.

Capabilities:
- Agents may invoke CLI commands
- Agent actions are attributed
- Scope applies to meta-operations

This enables:
- Self-healing workflows
- Automated recovery
- Meta-orchestration

---

## 15. Output & Contracts

All CLI commands must support:
- Human-readable output
- Machine-readable output (JSON or equivalent)

Errors must:
- Be explicit
- Be structured
- Never require interactive prompts by default

---

## 16. Invariants

The CLI enforces:

- No hidden state mutation
- No UI-only features
- No implicit execution

Breaking these invariants compromises the system.

---

## 17. Summary

The Hivemind CLI is:
- The authoritative interface
- The automation backbone
- The foundation for all UI layers

By exposing all functionality via the CLI, Hivemind enables:
- Agent-driven operation
- Deterministic testing
- Confident evolution

The UI does not extend Hivemind. It reflects it.
