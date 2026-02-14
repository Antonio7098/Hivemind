# Hivemind — Verification Authority

> **Principle 9:** Automated checks are mandatory.
> **Principle 14:** Human authority at critical boundaries.

This document defines the **authority hierarchy** for verification in Hivemind. It addresses the fundamental question: when multiple verification sources disagree, who wins?

Verification is not a single judgment. It is a layered process with explicit authority levels.

---

## 1. The Authority Problem

### 1.1 The Challenge

Verification in Hivemind involves multiple sources:
- Automated checks (tests, linters, type checkers)
- Verifier agent (LLM-based evaluation)
- Human judgment

These sources may disagree:
- Tests pass but verifier says code is wrong
- Verifier approves but tests fail
- Human overrides automated systems

### 1.2 The Principle

From Principle 14:
> "Agents execute. Humans decide."

From Principle 9:
> "Automated verification is the default."

These principles establish a hierarchy:
1. Humans are the ultimate authority
2. Automated checks are the default gate
3. Verifier agents provide advisory judgment

---

## 2. Verification Authority Hierarchy

### 2.1 The Hierarchy

```
┌─────────────────────────────────────────┐
│           Human Authority               │  ← Ultimate (can override anything)
├─────────────────────────────────────────┤
│         Automated Checks                │  ← Primary gate (must pass by default)
├─────────────────────────────────────────┤
│         Verifier Agent                  │  ← Advisory (can trigger retry)
└─────────────────────────────────────────┘
```

### 2.2 Authority Semantics

| Authority Level | Can Pass | Can Fail | Can Override |
|-----------------|----------|----------|--------------|
| Human | Yes | Yes | All below |
| Automated Checks | Yes (if pass) | Yes (if fail) | None |
| Verifier Agent | No (alone) | Yes (trigger retry) | None |

---

## 3. Automated Checks

### 3.1 Definition

Automated checks are **deterministic, machine-executable validations**.

Examples:
- Test suites (pytest, jest, go test)
- Linters (eslint, ruff, clippy)
- Type checkers (mypy, tsc)
- Format checkers (black, prettier)
- Security scanners (bandit, semgrep)

### 3.2 Authority

Automated checks are **authoritative by default**:
- If checks fail, task cannot succeed without human override
- If checks pass, task may proceed to verifier evaluation

### 3.3 Check Configuration

Tasks define their required checks:

```yaml
task:
  checks:
    - name: pytest
      command: pytest tests/
      required: true

    - name: mypy
      command: mypy src/
      required: true

    - name: ruff
      command: ruff check src/
      required: false  # Advisory only
```

### 3.4 Check Execution

Checks run in sequence or parallel (configurable).

Each check produces:
- Exit code (0 = pass, else = fail)
- stdout/stderr output
- Duration

### 3.5 Check Results

```
CheckResult:
  name: string
  passed: boolean
  exit_code: int
  output: string
  duration_ms: int
  required: boolean
```

### 3.6 Aggregation Rule

```
all_required_passed = all(
  check.passed for check in results if check.required
)
```

If `all_required_passed` is false:
- Task cannot proceed to success
- Verifier feedback may guide retry
- Human may override

---

## 4. Verifier Agent

### 4.1 Definition

The verifier agent is an **LLM-based evaluator** that assesses task completion against criteria.

### 4.2 Authority

The verifier agent is **advisory, not authoritative**:
- Verifier PASS does not guarantee task success (checks must also pass)
- Verifier SOFT FAIL triggers retry with feedback
- Verifier HARD FAIL terminates the task (unless human overrides)

### 4.3 Verifier Inputs

The verifier receives:
- Task definition and success criteria
- Diff produced by worker
- Automated check results
- Prior attempt history (if retry)

### 4.4 Verifier Outputs

```
VerifierDecision:
  outcome: PASS | SOFT_FAIL | HARD_FAIL
  reasoning: string
  feedback: string | null  # For worker on retry
  confidence: float | null  # Optional, 0-1
```

### 4.5 Outcome Semantics

| Outcome | Meaning | Effect |
|---------|---------|--------|
| PASS | Work appears correct | Proceed to check gate |
| SOFT_FAIL | Work has fixable issues | Retry with feedback |
| HARD_FAIL | Work is fundamentally wrong | Task fails (escalate) |

### 4.6 Verifier Limitations

The verifier agent:
- May be wrong (it's an LLM)
- Cannot override automated checks
- Cannot approve violations of scope
- Should not be trusted blindly

### 4.7 Why Include a Verifier?

Despite limitations, verifier agents provide value:
- Catch semantic issues tests don't cover
- Provide natural language feedback for retry
- Evaluate against soft criteria (code style, approach)

The key is **appropriate authority**: advisory, not authoritative.

---

## 5. Human Authority

### 5.1 Definition

Human authority is the **ultimate override** in the verification hierarchy.

### 5.2 Human Actions

Humans can:
- Override failed checks (with acknowledgment)
- Override verifier decisions
- Approve tasks directly
- Reject tasks regardless of check/verifier status

### 5.3 Override Mechanics

Human override is:
- **Explicit:** Requires deliberate action
- **Attributed:** Records who overrode and why
- **Audited:** Emits HumanOverride event

```
HumanOverride:
  user: string
  task_id: string
  override_type: CHECK_OVERRIDE | VERIFIER_OVERRIDE | DIRECT_APPROVAL
  reason: string
  timestamp: datetime
```

### 5.4 Override Policy

Projects may configure override policy:

```yaml
project:
  override_policy:
    check_override_allowed: true
    check_override_requires_reason: true
    verifier_override_allowed: true
    direct_approval_allowed: false  # Only allow normal flow
```

---

## 6. Verification Flow

### 6.1 Complete Flow

```
Worker Completes
       ↓
Scope Verification
       ↓ (if scope valid)
Automated Checks Execute
       ↓
Verifier Agent Evaluates
       ↓
Decision Logic
       ↓
Outcome
```

### 6.2 Decision Logic

```
if scope_violated:
    → ATTEMPT_FAILED (ScopeError)

if not all_required_checks_passed:
    if verifier.outcome == SOFT_FAIL:
        → RETRY (with check failure + verifier feedback)
    elif verifier.outcome == HARD_FAIL:
        → TASK_FAILED
    else:
        → RETRY (with check failure info)

if verifier.outcome == HARD_FAIL:
    → TASK_FAILED

if verifier.outcome == SOFT_FAIL:
    → RETRY (with verifier feedback)

if verifier.outcome == PASS and all_required_checks_passed:
    → TASK_SUCCESS
```

### 6.3 State Transitions

| Condition | Transition |
|-----------|------------|
| Scope violation | RUNNING → FAILED |
| Required check failed, retries remain | VERIFYING → RETRY |
| Required check failed, no retries | VERIFYING → FAILED |
| Verifier HARD_FAIL | VERIFYING → FAILED |
| Verifier SOFT_FAIL, retries remain | VERIFYING → RETRY |
| Verifier SOFT_FAIL, no retries | VERIFYING → FAILED |
| Verifier PASS, checks pass | VERIFYING → SUCCESS |

---

## 7. Retry Context

### 7.1 Retry Feedback Assembly

When transitioning to RETRY, assemble feedback for the next attempt:

```
RetryContext:
  attempt_number: int
  prior_diff: string
  check_results: [CheckResult]
  verifier_feedback: string | null
  specific_failures: [string]
```

### 7.2 Feedback Sources

| Source | Content |
|--------|---------|
| Failed checks | Check name, output, exit code |
| Verifier | Natural language feedback |
| Scope (if relevant) | What was violated |

### 7.3 Feedback Delivery

Retry context is passed to the worker agent as explicit input (see `retry-context.md`).

---

## 8. Verification Events

### 8.1 Check Events

```
CheckStarted:
  check_name: string
  task_id: string
  attempt_id: string

CheckCompleted:
  check_name: string
  task_id: string
  attempt_id: string
  passed: boolean
  exit_code: int
  duration_ms: int
```

### 8.2 Verifier Events

```
VerificationStarted:
  task_id: string
  attempt_id: string

VerificationCompleted:
  task_id: string
  attempt_id: string
  outcome: PASS | SOFT_FAIL | HARD_FAIL
  reasoning: string
```

### 8.3 Override Events

```
HumanOverride:
  task_id: string
  user: string
  override_type: string
  reason: string
```

---

## 9. Configuration

### 9.1 Task-Level Verification Config

```yaml
task:
  verification:
    checks:
      - name: pytest
        command: pytest
        required: true
        timeout: 300

    verifier:
      enabled: true
      criteria: |
        - Code compiles without errors
        - New tests cover the added functionality
        - No obvious security issues

    retry:
      max_attempts: 3
      include_check_output: true
      include_verifier_feedback: true
```

### 9.2 Project-Level Defaults

```yaml
project:
  verification_defaults:
    checks:
      - name: lint
        command: make lint
        required: true

    verifier:
      enabled: true

    retry:
      max_attempts: 2
```

---

## 10. Edge Cases

### 10.1 No Checks Defined

If no checks are defined:
- Verifier outcome is the primary signal
- PASS → SUCCESS (with warning that no checks ran)
- Human review recommended

### 10.2 Verifier Disabled

If verifier is disabled:
- Check results are the only gate
- All required checks pass → SUCCESS
- Any required check fails → RETRY or FAIL

### 10.3 Checks Timeout

If a check times out:
- Treated as check failure
- Error: VERIFICATION_TIMEOUT
- May retry with extended timeout (policy-dependent)

### 10.4 Verifier Crash

If verifier agent crashes:
- Treated as SOFT_FAIL by default
- Retry includes "verifier unavailable" context
- Human may intervene

---

## 11. Invariants

The verification authority model guarantees:

- Automated checks are authoritative unless human overrides
- Verifier agents cannot approve tasks alone
- Human authority can override any automated decision
- All verification outcomes emit events
- All overrides are attributed and audited

Violating these invariants is a SystemError.

---

## 12. Summary

Verification authority in Hivemind follows a clear hierarchy:

1. **Humans:** Ultimate authority, can override anything
2. **Automated Checks:** Primary gate, deterministic, authoritative
3. **Verifier Agent:** Advisory, triggers retry, may be wrong

This hierarchy ensures that:
- Deterministic checks cannot be bypassed by LLM judgment
- Human authority is preserved at critical boundaries
- Verification is observable and auditable

> Automated checks say "this is wrong." Verifiers suggest "try this instead." Humans decide "ship it or not."

---

## 13. Implementation Notes (Phase 17)

Phase 17 implemented the structured verification check framework described above.

### 13.1 Core Types
- `CheckConfig`: name, command, required (default true), optional timeout_ms
- `CheckResult`: name, passed, exit_code, output, duration_ms, required

### 13.2 CLI Commands
- `hivemind graph add-check <graph-id> <task-id> --name <name> --command <cmd> [--required] [--timeout-ms <ms>]`: Attach checks to graph tasks (only allowed on draft graphs).
- `hivemind verify run <task-id>`: Manually trigger verification for a task in Verifying state.
- `hivemind verify results <attempt-id> [--output]`: Show per-attempt check results and optionally full output.

### 13.3 Events
- `CheckStarted`: Emitted before each check runs.
- `CheckCompleted`: Emitted after each check, including pass/fail, exit code, combined output, duration, required flag.
- `GraphTaskCheckAdded`: Emitted when a check is attached to a graph task.

### 13.4 Behavior
- Checks run in the task worktree with CARGO_TARGET_DIR set appropriately.
- Required check failures block task success (transition to Retry/Failed based on retry policy).
- Optional check failures are recorded but do not block success.
- Per-check timeout is supported; timeouts emit exit code 124 and a "timed out" message.

See the Phase 17 report for detailed validation results and coverage.
