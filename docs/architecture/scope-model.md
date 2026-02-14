---
title: Scope Model
description: Safety, isolation, and parallelism
order: 7
---

# Hivemind — Scope Model (scope.md)

> **Core idea:** Scope is an explicit, enforceable capability contract that makes parallel agent execution safe.

This document defines the **Scope model** in Hivemind: how scopes are declared, compared, enforced, and observed. Scope is the primary mechanism by which Hivemind prevents unsafe concurrency, unintended side effects, and silent corruption.

This is a **behavioral and safety contract**, not an implementation guide.

---

## 1. Why Scope Exists

Agentic systems fail when:
- Multiple actors modify the same surface implicitly
- Boundaries are informal or advisory
- Conflicts are discovered too late

Hivemind introduces **Scope** to ensure that:
- Every agent knows *what it is allowed to touch*
- The system can reason about *parallel safety*
- Violations are detectable, attributable, and fatal

Scope is not a hint. It is law.

---

## 2. Definition of Scope

A **Scope** is a declarative capability contract between:
- An agent
- A task
- A project

It defines **what operations are permitted** during execution.

Scopes are:
- Declared before execution
- Enforced during execution
- Verified after execution

---

## 3. Scope Dimensions

Scopes are multi-dimensional. Each dimension is evaluated independently.

### 3.1 Filesystem Scope

Defines which files or directories may be accessed.

- Paths are project-relative
- Glob-based matching is allowed
- Deny rules override allow rules

Filesystem permissions:
- `read`
- `write`
- `deny`

Write permission implies read permission.

---

### 3.2 Repository Scope

Defines which repositories within a project may be accessed.

- Read-only vs read-write is explicit
- Repositories may be shared across projects with different modes

---

### 3.3 Execution Scope

Defines what commands or tools an agent may execute.

Examples:
- Allow test runners
- Deny package installation
- Restrict shell access

Execution scope is conservative by default.

---

### 3.4 Git Scope

Defines git-level permissions.

Examples:
- May commit
- May create branches
- Read-only

Git scope is enforced by the TaskFlow engine and merge protocol.

---

## 4. Scope Declaration

Scopes are assigned during planning and attached to tasks.

Scope declaration includes:
- Allowed paths
- Forbidden paths
- Repository modes
- Execution permissions
- Worktree preference

Once execution begins, scopes are immutable.

---

## 5. Scope Compatibility

Before scheduling tasks in parallel, Hivemind evaluates **scope compatibility**.

Compatibility is deterministic and rule-based.

---

### 5.1 Compatibility Levels

Scopes may be:

- **Compatible** — safe to run in parallel
- **Soft Conflict** — potentially unsafe, requires decision
- **Hard Conflict** — unsafe, must isolate or serialize

---

### 5.2 Filesystem Compatibility Rules

| Scope A | Scope B | Result |
|------|------|--------|
| Read | Read | Compatible |
| Read | Write | Soft Conflict |
| Write (disjoint paths) | Write (disjoint paths) | Compatible |
| Write (overlapping paths) | Write (overlapping paths) | Hard Conflict |

Deny rules always produce a Hard Conflict.

---

### 5.3 Repository Compatibility Rules

- Two read-only scopes → Compatible
- Read-only + read-write → Soft Conflict
- Two read-write scopes → Hard Conflict

---

### 5.4 Execution Compatibility Rules

- Overlapping dangerous commands → Hard Conflict
- Disjoint execution permissions → Compatible

---

## 6. Planner vs Engine Responsibilities

### Planner Responsibilities

- Assign scopes
- Detect potential conflicts
- Decide when isolation is preferable

Planner decisions are advisory and visible.

---

### Engine Responsibilities

- Enforce scopes at runtime
- Prevent unsafe scheduling
- Halt execution on violation

The engine never assumes agent goodwill.

---

## 7. Enforcement Mechanisms

Scope enforcement occurs at multiple layers.

**Phase 1 (Wrapper Runtimes):**
- **Post-hoc detection** via filesystem observation and git diff inspection
- Violations are **detected after execution**, not prevented
- Detected violations emit **ScopeViolationDetected** event and halt execution

**Phase 2+ (Interception / Native Runtime):**
- **Runtime interception** — restrict commands before execution
- True **prevention** becomes possible with deeper integration

*See `docs/design/scope-enforcement.md` for detailed enforcement mechanics.*

This distinction is important: Phase 1 scope enforcement is reliable **detection**, not prevention. Violations are caught and made fatal (Principle 2: fail fast via post-hoc check), but we do not claim to prevent execution before it occurs.

---

## 8. Scope Violations

Scope violations are:
- Fatal to the current attempt
- Attributed to task and agent
- Never ignored

On violation:
- Attempt fails
- Task may retry (policy-dependent)
- Human may intervene

---

## 9. Scope & Worktrees

Worktrees are derived from scope decisions.

Rules:
- Each task attempt executes in its own isolated worktree/branch
- Compatible scopes → eligible for parallel dispatch
- Soft conflicts → eligible for parallel dispatch with warning telemetry
- Hard conflicts → serialized in scheduler dispatch

Worktrees are an enforcement mechanism, not a primary abstraction.

---

## 10. Scope & Verification

Verification re-evaluates scope compliance:
- Diffs must fall within allowed paths
- Git actions must respect permissions

Verification failures due to scope are non-recoverable by retry alone.

---

## 11. Observability

All scope-related decisions emit events:

- `ScopeAssigned`
- `ScopeValidated`
- `ScopeConflictDetected`
- `TaskSchedulingDeferred`
- `ScopeViolationDetected`

This ensures scope decisions are inspectable and debuggable.

---

## 12. Invariants

The Scope system enforces:

- No implicit access
- No silent overlap
- No execution outside declared boundaries

Breaking these invariants is a system error.

---

## 13. Summary

Scope is the **primary safety mechanism** in Hivemind.

By making capabilities explicit, comparable, and enforceable, Hivemind enables:
- Safe parallelism
- Clear accountability
- Predictable execution

**Important:** Enforcement mechanisms vary by phase. Phase 1 is detection-based; Phase 2+ adds prevention. See `docs/design/scope-enforcement.md` for operational details.

Without scope, agent orchestration is guesswork. With scope, it is engineering.

