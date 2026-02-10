# Hivemind — Error Model

> **Principle 4:** Errors must be classifiable, attributable, and actionable.

This document defines the **error taxonomy** for Hivemind. Every error answers three questions: What failed? Why? What can be done next?

Errors are structured data, not strings. They are first-class outcomes, not exceptions to be hidden.

---

## 1. Design Principles

### 1.1 Errors Are Facts

Errors are **observable facts** about execution, not exceptional conditions to be suppressed.

Every error:
- Is emitted as an event
- Is attributed to an origin
- Has a defined recovery path (even if that path is "abort")

### 1.2 No Stringly-Typed Errors

Errors are never free-form strings. They are structured objects with:
- A category
- A code
- A message
- Metadata

This enables:
- Programmatic handling
- Aggregation and analysis
- Deterministic retry logic

### 1.3 Errors Are Explicit

Silent errors do not exist. If something fails, it emits an error event.

Hidden failures are considered **system bugs**, not acceptable behavior.

---

## 2. Error Structure

Every Hivemind error conforms to the following structure:

```
Error:
  category: ErrorCategory
  code: string (unique within category)
  message: string (human-readable)
  origin: ErrorOrigin
  recoverable: boolean
  recovery_hint: string | null
  context: map<string, any>
  timestamp: datetime
  correlation: CorrelationIds
```

### 2.1 Field Semantics

| Field | Purpose |
|-------|---------|
| `category` | High-level classification |
| `code` | Machine-readable identifier |
| `message` | Human-readable explanation |
| `origin` | What component produced this error |
| `recoverable` | Can retry potentially succeed? |
| `recovery_hint` | Suggested next action |
| `context` | Structured diagnostic data |
| `timestamp` | When the error occurred |
| `correlation` | Task, attempt, flow identifiers |

---

## 3. Error Categories

Errors are classified into **mutually exclusive categories**.

### 3.1 SystemError

Failures in Hivemind itself.

Examples:
- `SYSTEM_STATE_CORRUPTION` — derived state inconsistent with events
- `SYSTEM_EVENT_WRITE_FAILED` — event log write failed
- `SYSTEM_INVARIANT_VIOLATED` — architectural invariant broken

Properties:
- Origin: always `system`
- Recoverable: rarely
- Severity: critical

SystemErrors indicate bugs in Hivemind, not user or agent mistakes.

---

### 3.2 RuntimeError

Failures in the execution runtime (Claude Code, Codex CLI, etc.).

Examples:
- `RUNTIME_CRASHED` — process exited unexpectedly
- `RUNTIME_TIMEOUT` — execution exceeded time limit
- `RUNTIME_CONNECTION_FAILED` — could not start runtime
- `RUNTIME_OUTPUT_MALFORMED` — could not parse runtime output

Properties:
- Origin: `runtime:<adapter_name>`
- Recoverable: often (via retry)
- Severity: high

RuntimeErrors are expected. Runtimes are non-deterministic.

---

### 3.3 AgentError

Failures attributed to agent behavior.

Examples:
- `AGENT_SCOPE_VIOLATION` — agent modified files outside scope
- `AGENT_NO_CHANGES` — agent completed without producing work
- `AGENT_INVALID_OUTPUT` — agent output could not be processed

Properties:
- Origin: `agent:<agent_id>`
- Recoverable: sometimes (via retry with feedback)
- Severity: medium

AgentErrors may be retry-recoverable with verifier feedback.

---

### 3.4 ScopeError

Failures related to scope enforcement.

Examples:
- `SCOPE_VIOLATION_WRITE` — wrote to forbidden path
- `SCOPE_VIOLATION_READ` — read from forbidden path
- `SCOPE_VIOLATION_EXECUTE` — ran forbidden command
- `SCOPE_CONFLICT_UNRESOLVED` — parallel execution blocked by conflict

Properties:
- Origin: `scope_enforcer`
- Recoverable: no (scope violations are fatal to attempt)
- Severity: high

ScopeErrors are **always fatal** to the current attempt. They may trigger retry but never silent continuation.

---

### 3.5 VerificationError

Failures during verification.

Examples:
- `VERIFICATION_CHECK_FAILED` — automated check returned failure
- `VERIFICATION_AGENT_REJECTED` — verifier agent issued HARD FAIL
- `VERIFICATION_TIMEOUT` — verification exceeded time limit
- `VERIFICATION_CRASH` — verifier process crashed

Properties:
- Origin: `verifier` or `check:<check_name>`
- Recoverable: depends on failure type
- Severity: medium to high

VerificationErrors determine retry vs fail transitions.

---

### 3.6 GitError

Failures in git operations.

Examples:
- `GIT_CHECKOUT_FAILED` — could not checkout branch
- `GIT_COMMIT_FAILED` — could not create commit
- `GIT_MERGE_CONFLICT` — merge produced conflicts
- `GIT_WORKTREE_FAILED` — could not create worktree

Properties:
- Origin: `git`
- Recoverable: sometimes
- Severity: high

GitErrors often require human intervention.

---

### 3.7 UserError

Failures caused by invalid user input or configuration.

Examples:
- `USER_INVALID_CONFIG` — configuration file malformed
- `USER_INVALID_SCOPE` — scope definition is invalid
- `USER_MISSING_DEPENDENCY` — required resource not found

Properties:
- Origin: `user`
- Recoverable: no (requires user correction)
- Severity: medium

UserErrors block execution until corrected.

---

### 3.8 PolicyError

Failures due to policy limits.

Examples:
- `POLICY_RETRY_LIMIT_EXCEEDED` — maximum retries reached
- `POLICY_TIMEOUT_EXCEEDED` — flow-level timeout reached
- `POLICY_RESOURCE_EXHAUSTED` — resource quota exceeded

Properties:
- Origin: `policy`
- Recoverable: no (policy is authoritative)
- Severity: medium

PolicyErrors are deliberate terminations, not failures.

---

## 4. Error Origins

Every error is attributed to an **origin** identifying the responsible component.

### 4.1 Origin Format

Origins follow the format: `<component>` or `<component>:<identifier>`

Examples:
- `system`
- `runtime:claude-code`
- `agent:worker-abc123`
- `verifier`
- `check:pytest`
- `scope_enforcer`
- `git`
- `user`
- `policy`

### 4.2 Attribution Rules

- SystemErrors originate from `system`
- RuntimeErrors originate from `runtime:<adapter>`
- AgentErrors originate from `agent:<agent_id>`
- VerificationErrors originate from `verifier` or `check:<name>`
- ScopeErrors originate from `scope_enforcer`

Attribution is never ambiguous.

---

## 5. Recoverability

### 5.1 Recoverable Errors

An error is recoverable if:
- Retry may succeed
- The failure is transient or non-deterministic
- Feedback may guide a different approach

Examples:
- `RUNTIME_TIMEOUT` — may succeed with more time
- `AGENT_NO_CHANGES` — may succeed with clearer instructions
- `VERIFICATION_CHECK_FAILED` — may succeed after agent correction

### 5.2 Non-Recoverable Errors

An error is non-recoverable if:
- The failure is deterministic given current state
- Policy explicitly forbids retry
- The failure indicates corruption or invariant violation

Examples:
- `SCOPE_VIOLATION_WRITE` — violation is a fact, not transient
- `POLICY_RETRY_LIMIT_EXCEEDED` — policy is authoritative
- `SYSTEM_STATE_CORRUPTION` — system cannot continue

### 5.3 Recovery Hints

Recoverable errors should include a `recovery_hint`:

```
recovery_hint: "Retry with extended timeout"
recovery_hint: "Provide clearer success criteria"
recovery_hint: "Check git remote connectivity"
```

Non-recoverable errors may include hints for human intervention:

```
recovery_hint: "Abort TaskFlow and investigate scope definition"
recovery_hint: "Manual merge conflict resolution required"
```

---

## 6. Error → State Transitions

Errors trigger explicit state transitions.

### 6.1 Task Execution Transitions

| Error Category | Typical Transition |
|----------------|-------------------|
| RuntimeError (recoverable) | RUNNING → RETRY |
| RuntimeError (fatal) | RUNNING → FAILED |
| AgentError (recoverable) | VERIFYING → RETRY |
| AgentError (fatal) | VERIFYING → FAILED |
| ScopeError | RUNNING → FAILED |
| VerificationError (soft) | VERIFYING → RETRY |
| VerificationError (hard) | VERIFYING → FAILED |
| PolicyError | Any → FAILED |
| SystemError | Any → FAILED (with escalation) |

### 6.2 TaskFlow Transitions

| Error Category | Typical Transition |
|----------------|-------------------|
| SystemError | TaskFlow → ABORTED |
| PolicyError (flow-level) | TaskFlow → FAILED |
| Task FAILED (blocking) | Downstream tasks remain PENDING |

---

## 7. Error Events

Errors are emitted as structured events.

### 7.1 Event Format

```
ErrorOccurred:
  error: Error (as defined above)
```

Correlation identifiers are carried in the event metadata (CorrelationIds):

```
metadata.correlation:
  project_id: string | null
  graph_id: string | null
  flow_id: string | null
  task_id: string | null
  attempt_id: string | null
```

### 7.2 Event Properties

- ErrorOccurred events are emitted for failures from state-changing or side-effectful commands
- ErrorOccurred events are correlated to execution context via event metadata correlation IDs
- ErrorOccurred events may be followed by state transition events

---

## 8. Error Aggregation

For analysis and debugging, errors support aggregation.

### 8.1 By Category

Count errors by category to identify systemic issues:
- Many RuntimeErrors → runtime instability
- Many ScopeErrors → scope definitions too narrow
- Many VerificationErrors → success criteria unclear

### 8.2 By Origin

Count errors by origin to identify problematic components:
- Many errors from `runtime:codex-cli` → adapter issue
- Many errors from `check:pytest` → test instability

### 8.3 By Correlation

Filter errors by TaskFlow, task, or attempt for debugging specific failures.

---

## 9. Invariants

The error model enforces:

- Every state-changing failure emits an ErrorOccurred event
- Every error has a category
- Every error has an origin
- Every error has a code unique within its category
- ScopeErrors are always fatal to the attempt
- SystemErrors always escalate

Violating these invariants is itself a SystemError.

---

## 10. Summary

Hivemind's error model ensures that:

- Failures are structured and classifiable
- Failures are attributed to responsible components
- Failures have defined recovery paths
- Failures are observable and auditable

> Principle 4: Every error should answer: What failed? Why? What can be done next?

This model makes that principle concrete.
