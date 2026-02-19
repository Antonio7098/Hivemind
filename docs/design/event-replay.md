---
title: Event Replay
description: Event replay semantics
order: 3
---

# Hivemind — Event Replay Semantics

> **Principle 1:** Observability is truth. All state is derived from events.
> **Principle 15:** No magic. Everything has a trail.

This document clarifies what **event replay** means in Hivemind, what it can and cannot reconstruct, and how the distinction between orchestration state and execution artifacts is maintained.

Event replay is powerful. It is also bounded. This document defines those bounds honestly.

---

## 1. The Replay Promise

### 1.1 What Events Enable

Events are the single source of truth. From events, Hivemind can:
- Reconstruct orchestration state (TaskFlow position, attempt history)
- Understand what happened (sequence of actions and decisions)
- Audit execution (who did what, when)
- Debug failures (trace back to cause)

### 1.2 What Events Cannot Do

Events cannot:
- Recreate files that no longer exist
- Reconstruct git commits that were deleted
- Re-execute runtime behavior
- Restore external system state

Exception for governance artifacts:
- Governance files can be restored when a bounded governance recovery snapshot exists.
- The snapshot is still subordinate to event authority (restore only applies revision-compatible entries).

---

## 2. Two Classes of State

### 2.1 Orchestration State

**Orchestration state** is the logical state of TaskFlows and tasks.

Examples:
- TaskFlow status (RUNNING, PAUSED, COMPLETED)
- Task execution state (PENDING, RUNNING, SUCCESS, FAILED)
- Attempt count and outcomes
- Verification decisions
- Dependency satisfaction

**Property:** Orchestration state is **fully reconstructable** from events.

### 2.2 Execution Artifacts

**Execution artifacts** are materialized side effects of agent work.

Examples:
- Modified files
- Created commits
- Generated diffs
- Runtime output

**Property:** Execution artifacts are **referenced** by events but **not contained** in them.

---

## 3. The Precise Replay Guarantee

### 3.1 Formal Statement

> Replaying events in causal order deterministically reconstructs **orchestration state**.
>
> Replaying events **references** execution artifacts, which must be preserved separately.

### 3.2 What This Means

**Can Reconstruct:**
- "Task X is in state SUCCESS"
- "Attempt 2 was rejected by the verifier"
- "TaskFlow was paused at 14:32:00"
- "Human approved merge at 15:00:00"

**Cannot Reconstruct:**
- The actual bytes of modified files
- The actual git commit objects
- The actual runtime output

### 3.3 Why This Distinction Matters

Understanding this distinction prevents:
- False expectations about recovery capabilities
- Confusion when artifacts are missing
- Over-reliance on events for backup

---

## 4. Event Categories by Reconstruction

### 4.1 Pure State Events (Fully Reconstructable)

These events contain all information needed to reconstruct state:

```
TaskFlowStarted
TaskFlowPaused
TaskFlowResumed
TaskFlowCompleted
TaskFlowAborted

TaskReady
TaskBlocked
TaskExecutionStarted
TaskExecutionStateChanged
TaskExecutionSucceeded
TaskExecutionFailed

AttemptStarted
AttemptCompleted

VerificationStarted
VerificationCompleted

HumanPausedTaskFlow
HumanResumedTaskFlow
HumanOverride
```

Replaying these events reconstructs:
- Current state of all tasks
- History of all state transitions
- Timing of all transitions
- Human intervention points

### 4.2 Reference Events (Point to Artifacts)

These events reference external artifacts:

```
CheckpointCommitCreated:
  commit_sha: "abc123"  ← Reference to git object

FileModified:
  path: "src/main.py"
  hash: "def456"  ← Reference to file content

DiffComputed:
  diff_id: "diff-789"  ← Reference to stored diff
```

Replaying these events tells you:
- A commit was created with SHA abc123
- File src/main.py was modified
- A diff was computed

It does NOT give you:
- The commit contents
- The file contents
- The diff contents

### 4.3 Observational Events (Context Only)

These events provide context but don't affect state:

```
RuntimeOutputChunk:
  content: "Running tests..."  ← Captured output

RuntimeStarted:
  prompt: "Task: ..."
  flags: ["--model", "..."]

ModelRequestPrepared:
  request:
    digest: "sha256:..."
    blob_path: "~/.hivemind/blobs/sha256/..../....blob"

ModelResponseReceived:
  response:
    digest: "sha256:..."
    blob_path: "~/.hivemind/blobs/sha256/..../....blob"
```

These are useful for debugging but not for state reconstruction.

---

## 5. Artifact Preservation

### 5.1 Preservation Strategies

Since artifacts are not in events, they must be preserved separately:

**Git Artifacts:**
- Execution branches
- Checkpoint commits
- Worktrees

Preserved by: Git reflog, explicit branch retention policy

**File Artifacts:**
- Diffs
- Snapshots
- Output logs
- Native model/tool payload blobs (`~/.hivemind/blobs/sha256/...`)

Preserved by: Artifact storage (filesystem or object store)

**Governance Artifacts (bounded):**
- Constitution/documents/templates/skills/prompts/notepads/graph snapshot files
- Preserved by: governance recovery snapshots under `~/.hivemind/projects/<project-id>/recovery/snapshots/`

### 5.2 Artifact Lifecycle

```
Artifact Created
    ↓
Event References Artifact
    ↓
TaskFlow Completes
    ↓
Retention Policy Applies
    ↓
Artifact Archived or Deleted
```

### 5.3 Artifact Retention Policy

```yaml
project:
  artifact_retention:
    execution_branches:
      on_success: delete_after_merge
      on_failure: retain_7_days
    diffs:
      retain: 30_days
    output_logs:
      retain: 7_days
    native_blobs:
      retain: 30_days
```

---

## 6. Replay Operations

### 6.1 Full State Reconstruction

Reconstruct current orchestration state from events:

```
replay(events) → OrchestrationState
```

Input: Ordered list of events
Output: Current TaskFlow state, task states, attempt history

Use cases:
- Recovery after crash
- State verification
- Debugging

### 6.2 Point-in-Time Reconstruction

Reconstruct state at a specific moment:

```
replay(events, until=timestamp) → OrchestrationState
```

Use cases:
- "What was the state at 14:30?"
- Historical debugging

### 6.3 Projection

Derive a specific view from events:

```
project(events, projection="task_timeline") → TaskTimeline
```

Use cases:
- UI views
- Reports
- Metrics

---

## 7. Replay Guarantees

### 7.1 Determinism

Given the same events in the same order, replay produces identical state.

**Requirement:** Events must be replayed in causal order.

**Causal Order:** Events from the same TaskFlow are ordered by sequence number. Events across TaskFlows are independent.

### 7.2 Idempotence

Replaying the same events multiple times produces the same result.

**Requirement:** Replay does not emit new events or cause side effects.

### 7.3 Completeness

All orchestration state transitions are captured in events.

**Requirement:** No state change occurs without an event.

**Enforcement:** State mutation only happens through event application.

---

## 8. Failure Scenarios

### 8.1 Missing Events

If events are lost:
- Orchestration state may be incorrect
- This is a **system failure**
- Mitigation: Event log durability, replication

### 8.2 Missing Artifacts

If artifacts are lost but events exist:
- Orchestration state is correct
- Artifact content is unavailable
- Events show "commit abc123 was created" but commit may not exist

**Handling:**
- Detect missing artifact references
- Report as degraded state
- Do not fail replay

### 8.3 Corrupt Events

If events are corrupt:
- Replay fails at corrupt event
- Report corruption with event ID
- Manual intervention required

---

## 9. Implementation Notes

### 9.1 Event Storage

Events are stored with:
- Sequence number (per TaskFlow)
- Timestamp
- Correlation IDs
- Event type
- Payload

### 9.2 Replay Algorithm

```
function replay(events):
    state = initial_state()
    for event in sorted(events, by=sequence):
        state = apply(state, event)
    return state

function apply(state, event):
    match event.type:
        TaskExecutionStateChanged:
            state.tasks[event.task_id].state = event.new_state
        VerificationCompleted:
            state.tasks[event.task_id].verification = event.outcome
        # ... etc
    return state
```

### 9.3 Artifact Resolution

When events reference artifacts:
```
function resolve_artifact(event):
    match event.type:
        CheckpointCommitCreated:
            return git.get_commit(event.commit_sha)  # May fail
        DiffComputed:
            return artifact_store.get(event.diff_id)  # May fail
```

Artifact resolution is separate from state reconstruction.

---

## 10. Observability

### 10.1 Replay Events

Replay itself is observable:

```
ReplayStarted:
  taskflow_id: string
  event_count: int

ReplayCompleted:
  taskflow_id: string
  events_applied: int
  artifacts_missing: int
  duration_ms: int
```

### 10.2 Consistency Checks

Periodic consistency checks:
- Compare derived state to stored state
- Report discrepancies
- Alert on drift

---

## 11. Invariants

The event replay model guarantees:

- Orchestration state is fully derivable from events
- Replay is deterministic given ordered events
- Replay is idempotent
- Missing artifacts do not break state reconstruction
- All state transitions emit events

Violating these invariants is a SystemError.

---

## 12. Summary

Event replay in Hivemind:

**Does:**
- Reconstruct orchestration state (TaskFlow position, task states, history)
- Provide audit trail (who, what, when)
- Enable point-in-time queries
- Support crash recovery

**Does Not:**
- Recreate file contents
- Restore git commits
- Re-execute runtime behavior

**The Distinction:**
- Events capture **what happened**
- Artifacts capture **what was produced**
- Events reference artifacts; they do not contain them

> The event log tells the story. The artifacts are the evidence.

This model is honest about its capabilities while providing strong guarantees for orchestration state.
