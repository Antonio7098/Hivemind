---
title: Scope Enforcement
description: Scope enforcement mechanisms
order: 7
---

# Hivemind — Scope Enforcement

> **Principle 2:** Fail fast, fail loud, fail early.
> **Principle 3:** Reliability over cleverness.

This document specifies **how scope is enforced** at each phase of Hivemind's evolution. It is honest about what enforcement means in Phase 1 (wrapper runtimes) versus later phases.

Scope is law. But law requires enforcement mechanisms. This document defines those mechanisms.

---

## 1. Enforcement Philosophy

### 1.1 Prevention vs Detection

Scope enforcement has two modes:

- **Prevention:** Block violations before they occur
- **Detection:** Identify violations after they occur

These are not equivalent. Prevention is stronger but requires deeper system integration.

### 1.2 Honest Assessment

In Phase 1 (wrapper runtimes), Hivemind **cannot prevent** all scope violations because:
- Wrapper runtimes execute as subprocesses
- Subprocess filesystem access cannot be intercepted without OS-level sandboxing
- OS-level sandboxing (seccomp, landlock) is platform-specific and complex

Therefore, Phase 1 scope enforcement is primarily **detection-based**.

This is acceptable because:
- Detection still satisfies Principle 2 (fail fast via post-hoc check)
- Detection is reliable and deterministic
- Detected violations are fatal and observable

What is **not** acceptable:
- Claiming prevention when only detection exists
- Allowing violations to pass silently
- Treating detection failures as non-fatal

---

## 2. Enforcement Phases

### Phase 1: Detection-Based (Wrapper Runtimes)

Scope violations are detected after execution completes.

**Mechanisms:**
- Git diff inspection
- Filesystem snapshot comparison
- Worktree isolation

**Timing:**
- After worker agent completes
- Before verification begins

**Consequence:**
- Detected violation → attempt fails immediately
- ScopeViolationDetected event emitted
- No retry without human acknowledgment

---

### Phase 2: Hybrid (Interception + Detection)

Scope violations are partially intercepted during execution.

**Additional Mechanisms:**
- Tool call interception (if runtime supports it)
- Command allowlisting
- Real-time filesystem monitoring (inotify/fswatch)

**Timing:**
- During execution (where possible)
- After execution (for completeness)

---

### Phase 3: Prevention-Based (Native Runtime)

Scope violations are prevented before they occur.

**Mechanisms:**
- All tool calls routed through Hivemind
- Filesystem access mediated by Hivemind
- No direct subprocess execution

**Timing:**
- Before any operation executes

---

## 3. Phase 1 Enforcement Mechanics

This section details exactly how enforcement works with wrapper runtimes.

### 3.1 Pre-Execution Setup

Before launching a worker agent:

1. **Worktree Creation**
   - Create isolated worktree for the task
   - Worktree is the execution sandbox

2. **Baseline Snapshot**
   - Record filesystem state (file list + hashes) within worktree
   - Record HEAD commit

3. **Environment Restriction**
   - Set working directory to worktree
   - Restrict environment variables (remove sensitive values)
   - Set resource limits where possible (ulimit)

### 3.2 Execution Monitoring

During execution:

1. **Process Monitoring**
   - Track subprocess lifecycle
   - Enforce timeout limits
   - Capture stdout/stderr

2. **Optional: Filesystem Watching**
   - inotify/fswatch on worktree directory
   - Log file access patterns (advisory, not blocking)
   - Detect writes outside worktree (if possible)

Filesystem watching in Phase 1 is **best-effort observability**, not enforcement.

### 3.3 Post-Execution Verification

After worker agent exits:

1. **Compute Changes**
   - Compare filesystem state to baseline
   - Generate list of modified/created/deleted files

2. **Scope Check**
   - For each changed file:
     - Is path within allowed write scope? → OK
     - Is path within denied scope? → VIOLATION
     - Is path outside all declared scopes? → VIOLATION

3. **Git Verification**
   - Inspect uncommitted changes
   - Inspect any commits created
   - Verify all changes fall within scope

4. **Emit Result**
   - All changes within scope → ScopeValidated event
   - Any violation → ScopeViolationDetected event

### 3.4 Violation Handling

On ScopeViolationDetected:

1. **Attempt Fails Immediately**
   - No verification phase
   - No retry without policy check

2. **Event Emitted**
   ```
   ScopeViolationDetected:
     task_id: ...
     attempt_id: ...
     violation_type: WRITE | READ | EXECUTE
     violating_path: "/path/to/file"
     allowed_paths: [...]
     denied_paths: [...]
   ```

3. **Worktree Preserved**
   - Do not clean up
   - Evidence for debugging

4. **Error Raised**
   - Category: ScopeError
   - Code: SCOPE_VIOLATION_WRITE (or READ, EXECUTE)
   - Recoverable: false

---

## 4. Worktree Isolation

Worktrees are the primary isolation mechanism in Phase 1.

### 4.1 Isolation Rules

| Scope Compatibility | Worktree Strategy |
|---------------------|-------------------|
| Compatible (disjoint writes) | May share worktree |
| Soft Conflict | Separate worktrees recommended |
| Hard Conflict | Separate worktrees required |

### 4.2 Worktree Lifecycle

1. **Creation**
   - `git worktree add` from TaskFlow base revision
   - Unique path per task: `<workdir>/.hivemind/worktrees/<taskflow>/<task>`

2. **Ownership**
   - Worktree is owned by exactly one task at a time
   - Ownership transferred on task completion

3. **Cleanup**
   - On task success: archive or delete (configurable)
   - On task failure: preserve for debugging
   - On TaskFlow completion: cleanup all worktrees

### 4.3 Worktree Limitations

Worktrees isolate **git-tracked content** but not:
- Global state (databases, caches)
- Network resources
- Shared dependencies

These require additional enforcement mechanisms (execution scope).

---

## 5. Execution Scope Enforcement

Execution scope restricts what commands agents may run.

### 5.1 Phase 1: Advisory Only

In wrapper mode, Hivemind cannot intercept arbitrary subprocess execution.

**What we can do:**
- Document allowed/denied commands in scope
- Detect violations via output parsing (heuristic)
- Fail on detection of forbidden patterns

**What we cannot do:**
- Prevent a runtime from running `rm -rf /`
- Intercept arbitrary shell commands

**Mitigation:**
- Run wrapper processes with restricted permissions
- Use containerization where available
- Accept that Phase 1 execution scope is advisory

### 5.2 Phase 2+: Enforcement

With tool interception:
- Route all tool calls through Hivemind
- Check each tool call against execution scope
- Block violations before execution

---

## 6. Repository Scope Enforcement

Repository scope controls which repos an agent may access.

### 6.1 Enforcement Mechanism

- Each attached repo gets a task-scoped worktree (`exec/<flow>/<task>`)
- Runtime receives all repo worktree paths (context + `HIVEMIND_ALL_WORKTREES` + per-repo env vars)
- Post-execution verification inspects each repo worktree (`git status --porcelain`) and enforces repository access mode
- Read-only mode is enforced via post-hoc write detection in that repo worktree

### 6.2 Multi-Repo Tasks

For tasks spanning multiple repositories:
- Each repo gets its own execution branch
- Scope is evaluated per-repo
- Violations in any repo fail the entire attempt

---

## 7. Git Scope Enforcement

Git scope controls git operations.

### 7.1 Phase 1 Enforcement

Post-hoc verification:
- Inspect `git log` for unexpected commits
- Inspect `git branch` for unexpected branches
- Inspect `git remote` for push attempts

### 7.2 Violation Detection

| Permission | Violation Indicator |
|------------|---------------------|
| May commit | Commits exist when forbidden |
| May branch | New branches exist when forbidden |
| Read-only | Any modification to git state |

---

## 8. Scope Verification Integration

Scope verification happens **before** agent verification.

### 8.1 Verification Order

```
Worker Completes
    ↓
Scope Verification (this document)
    ↓
[If scope valid] Agent Verification
    ↓
[If scope violation] Attempt Fails (skip agent verification)
```

### 8.2 Rationale

- Scope violations are deterministic facts
- No point running verification on violated scope
- Fail fast, fail loud

---

## 9. Scope Violation Events

All scope decisions emit events.

### 9.1 Success Event

```
ScopeValidated:
  task_id: string
  attempt_id: string
  files_checked: int
  timestamp: datetime
```

### 9.2 Violation Event

```
ScopeViolationDetected:
  task_id: string
  attempt_id: string
  violation_type: WRITE | READ | EXECUTE | GIT
  violating_path: string
  scope_definition: Scope
  timestamp: datetime
```

### 9.3 Conflict Event

```
ScopeConflictDetected:
  flow_id: uuid
  task_id: uuid
  conflicting_task_id: uuid
  severity: soft_conflict | hard_conflict
  action: warn_parallel | serialized
  reason: string
```

### 9.4 Scheduling Deferred Event

```
TaskSchedulingDeferred:
  flow_id: uuid
  task_id: uuid
  reason: string
```

---

## 10. Limitations and Honesty

### 10.1 What Phase 1 Cannot Do

- Prevent filesystem access outside worktree (can only detect)
- Prevent arbitrary command execution (can only advise)
- Enforce network restrictions
- Prevent memory-based side effects

### 10.2 Why This Is Acceptable

Per Principle 2: "Errors surface immediately"

Detection-based enforcement satisfies this:
- Violations are detected before verification
- Violations fail the attempt immediately
- Violations are fully observable

Per Principle 3: "Reliability over cleverness"

Detection is **reliable**:
- Filesystem diffs are deterministic
- Git state is inspectable
- No heuristics or guessing

### 10.3 What Is Not Acceptable

- Claiming prevention when only detection exists
- Allowing detected violations to be ignored
- Retrying without human acknowledgment after scope violation

---

## 11. Future Enforcement

### 11.1 OS-Level Sandboxing (Phase 2)

Potential mechanisms:
- Linux: seccomp-bpf, landlock
- macOS: sandbox-exec
- Containers: Docker, Podman

These provide true prevention but require platform-specific implementation.

### 11.2 Native Runtime (Phase 3)

With a native runtime:
- All file operations mediated by Hivemind
- All tool calls routed through Hivemind
- True prevention becomes possible

---

## 12. Invariants

Scope enforcement guarantees:

- Every attempt undergoes scope verification
- Scope verification completes before agent verification
- Scope violations are fatal to the attempt
- Scope violations emit observable events
- Worktrees provide isolation for git-tracked content

Violating these invariants is a SystemError.

---

## 13. Summary

Scope enforcement in Hivemind is:

- **Phase 1:** Detection-based, post-execution, reliable
- **Phase 2:** Hybrid interception + detection
- **Phase 3:** Prevention-based, mediated execution

The architecture is honest about Phase 1 limitations while ensuring that detected violations are always fatal and observable.

> Scope is law. Detection is enforcement. Silence is forbidden.
