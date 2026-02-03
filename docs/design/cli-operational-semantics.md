# Hivemind — CLI Operational Semantics

> **Principle 7:** CLI-first is non-negotiable.
> **Principle 15:** No magic. Everything has a reason.

This document specifies the **operational semantics** of Hivemind CLI commands: not just what you can do, but what happens when you do it. Each command has defined preconditions, effects, and failure modes.

The CLI is the authoritative interface. These semantics are the contract.

---

## 1. Semantic Structure

### 1.1 Command Semantics Format

Each command is specified with:
- **Synopsis:** Command signature
- **Preconditions:** What must be true before execution
- **Effects:** What changes when command succeeds
- **Events:** What events are emitted
- **Failures:** What can go wrong and how it's reported
- **Idempotence:** Behavior on repeated execution

### 1.2 Notation

```
[optional]
<required>
...       # repeatable
|         # alternatives
```

---

## 2. Project Commands

### 2.1 project create

**Synopsis:**
```
hivemind project create <name> [--description <text>]
```

**Preconditions:**
- No project with `<name>` exists in registry

**Effects:**
- New project created with unique ID
- Project added to registry
- Project directory structure created (if applicable)

**Events:**
```
ProjectCreated:
  project_id: <generated>
  name: <name>
  description: <text>
```

**Failures:**
- `PROJECT_ALREADY_EXISTS`: Project with name exists
- `INVALID_PROJECT_NAME`: Name contains invalid characters

**Idempotence:** Not idempotent. Second call fails.

---

### 2.2 project list

**Synopsis:**
```
hivemind project list [--format json|table]
```

**Preconditions:** None

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `REGISTRY_READ_ERROR`: Cannot read project registry

**Idempotence:** Idempotent. Always returns current state.

---

### 2.3 project attach-repo

**Synopsis:**
```
hivemind project attach-repo <project> <repo-path> [--name <name>] [--access ro|rw]
```

**Preconditions:**
- Project `<project>` exists
- `<repo-path>` is a valid git repository
- No repo with same name already attached

**Effects:**
- Repository reference added to project
- Access mode recorded (default: rw)

**Events:**
```
RepositoryAttachedToProject:
  project_id: <project_id>
  repo_name: <name>
  repo_path: <repo-path>
  access_mode: <ro|rw>
```

**Failures:**
- `PROJECT_NOT_FOUND`: Project doesn't exist
- `INVALID_REPOSITORY`: Path is not a git repo
- `REPO_ALREADY_ATTACHED`: Repo name already used

**Idempotence:** Not idempotent. Second call fails.

---

## 3. Task Commands

### 3.1 task create

**Synopsis:**
```
hivemind task create <project> <title> [--description <text>] [--scope <scope-def>]
```

**Preconditions:**
- Project exists

**Effects:**
- Task created with unique ID
- Task added to project's task list
- Task state: OPEN (not in any TaskFlow)

**Events:**
```
TaskCreated:
  task_id: <generated>
  project_id: <project_id>
  title: <title>
```

**Failures:**
- `PROJECT_NOT_FOUND`: Project doesn't exist
- `INVALID_SCOPE`: Scope definition is malformed

**Idempotence:** Not idempotent. Creates new task each time.

---

### 3.2 task list

**Synopsis:**
```
hivemind task list <project> [--state open|closed|all] [--format json|table]
```

**Preconditions:**
- Project exists

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `PROJECT_NOT_FOUND`: Project doesn't exist

**Idempotence:** Idempotent.

---

### 3.3 task close

**Synopsis:**
```
hivemind task close <task-id> [--reason <text>]
```

**Preconditions:**
- Task exists
- Task is not in an active TaskFlow

**Effects:**
- Task state changes to CLOSED
- Task removed from active consideration

**Events:**
```
TaskClosed:
  task_id: <task-id>
  reason: <text>
```

**Failures:**
- `TASK_NOT_FOUND`: Task doesn't exist
- `TASK_IN_ACTIVE_FLOW`: Task is part of running TaskFlow

**Idempotence:** Idempotent. Closing closed task is no-op.

---

## 4. TaskGraph Commands

### 4.1 graph create

**Synopsis:**
```
hivemind graph create <project> <name> [--from-tasks <task-ids...>]
```

**Preconditions:**
- Project exists
- All specified tasks exist and are open

**Effects:**
- TaskGraph created with unique ID
- Tasks added as nodes (if specified)
- Graph is mutable until used

**Events:**
```
TaskGraphCreated:
  graph_id: <generated>
  project_id: <project_id>
  name: <name>
```

**Failures:**
- `PROJECT_NOT_FOUND`
- `TASK_NOT_FOUND`: One or more tasks don't exist

**Idempotence:** Not idempotent. Creates new graph.

---

### 4.2 graph add-dependency

**Synopsis:**
```
hivemind graph add-dependency <graph-id> <from-task> <to-task>
```

**Preconditions:**
- Graph exists and is mutable
- Both tasks are in the graph
- Adding dependency doesn't create cycle

**Effects:**
- Dependency edge added to graph
- `<from-task>` must complete before `<to-task>` can start

**Events:**
```
DependencyAdded:
  graph_id: <graph-id>
  from_task: <from-task>
  to_task: <to-task>
```

**Failures:**
- `GRAPH_NOT_FOUND`
- `GRAPH_IMMUTABLE`: Graph already used in TaskFlow
- `TASK_NOT_IN_GRAPH`
- `CYCLE_DETECTED`: Dependency would create cycle

**Idempotence:** Idempotent. Adding existing dependency is no-op.

---

### 4.3 graph validate

**Synopsis:**
```
hivemind graph validate <graph-id>
```

**Preconditions:**
- Graph exists

**Effects:** None (validation only)

**Output:**
- Validation result (valid/invalid)
- List of issues if invalid

**Events:** None

**Failures:**
- `GRAPH_NOT_FOUND`

**Idempotence:** Idempotent.

---

## 5. TaskFlow Commands

### 5.1 flow create

**Synopsis:**
```
hivemind flow create <graph-id> [--name <name>]
```

**Preconditions:**
- Graph exists and is valid
- Graph is not already used by another active flow

**Effects:**
- TaskFlow created with unique ID
- Graph becomes immutable
- Flow state: CREATED (not started)
- All tasks in flow: PENDING

**Events:**
```
TaskFlowCreated:
  flow_id: <generated>
  graph_id: <graph-id>
  name: <name>
```

**Failures:**
- `GRAPH_NOT_FOUND`
- `GRAPH_INVALID`: Graph fails validation
- `GRAPH_IN_USE`: Graph already used by active flow

**Idempotence:** Not idempotent. Creates new flow.

---

### 5.2 flow start

**Synopsis:**
```
hivemind flow start <flow-id>
```

**Preconditions:**
- Flow exists
- Flow state is CREATED or PAUSED

**Effects:**
- Flow state changes to RUNNING
- Scheduler begins releasing ready tasks
- Tasks with no dependencies become READY

**Events:**
```
TaskFlowStarted:
  flow_id: <flow-id>
  timestamp: <now>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_ALREADY_RUNNING`: Flow is already running
- `FLOW_COMPLETED`: Flow has already completed
- `FLOW_ABORTED`: Flow was aborted

**Idempotence:** Idempotent if flow is PAUSED. Not idempotent if CREATED.

---

### 5.3 flow pause

**Synopsis:**
```
hivemind flow pause <flow-id> [--wait]
```

**Preconditions:**
- Flow exists
- Flow state is RUNNING

**Effects:**
- Flow state changes to PAUSED
- No new tasks scheduled
- Running tasks continue to completion (unless --wait)
- With --wait: blocks until running tasks complete

**Events:**
```
TaskFlowPaused:
  flow_id: <flow-id>
  running_tasks: [<task-ids>]
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_RUNNING`: Flow is not in RUNNING state

**Idempotence:** Idempotent. Pausing paused flow is no-op.

---

### 5.4 flow resume

**Synopsis:**
```
hivemind flow resume <flow-id>
```

**Preconditions:**
- Flow exists
- Flow state is PAUSED

**Effects:**
- Flow state changes to RUNNING
- Scheduler resumes releasing ready tasks

**Events:**
```
TaskFlowResumed:
  flow_id: <flow-id>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_PAUSED`: Flow is not paused

**Idempotence:** Not idempotent on non-paused flow.

---

### 5.5 flow abort

**Synopsis:**
```
hivemind flow abort <flow-id> [--force] [--reason <text>]
```

**Preconditions:**
- Flow exists
- Flow is not already COMPLETED or ABORTED

**Effects:**
- Flow state changes to ABORTED
- Running tasks are terminated (with --force)
- Or running tasks complete, then abort (without --force)
- No further tasks scheduled
- Artifacts preserved

**Events:**
```
TaskFlowAborted:
  flow_id: <flow-id>
  reason: <text>
  forced: <boolean>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_ALREADY_TERMINAL`: Flow is completed or aborted

**Idempotence:** Idempotent. Aborting aborted flow is no-op.

---

### 5.6 flow status

**Synopsis:**
```
hivemind flow status <flow-id> [--format json|table|detail]
```

**Preconditions:**
- Flow exists

**Effects:** None (read-only)

**Output:**
- Flow state
- Task states summary
- Progress metrics
- Current activity

**Events:** None

**Failures:**
- `FLOW_NOT_FOUND`

**Idempotence:** Idempotent.

---

## 6. Task Execution Commands

### 6.1 task retry

**Synopsis:**
```
hivemind task retry <task-id> [--reset-count]
```

**Preconditions:**
- Task exists and is in a TaskFlow
- Task state is FAILED or RETRY
- Retry limit not exceeded (unless --reset-count)

**Effects:**
- Task state changes to PENDING (if dependencies met) or appropriate state
- New attempt will be scheduled
- With --reset-count: retry counter reset to 0

**Events:**
```
TaskRetryRequested:
  task_id: <task-id>
  reset_count: <boolean>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`: Task is not part of any TaskFlow
- `TASK_NOT_RETRIABLE`: Task is not in retriable state
- `RETRY_LIMIT_EXCEEDED`: And --reset-count not specified

**Idempotence:** Not idempotent. Each call queues a retry.

---

### 6.2 task abort

**Synopsis:**
```
hivemind task abort <task-id> [--reason <text>]
```

**Preconditions:**
- Task exists and is in a TaskFlow
- Task is not already SUCCESS or FAILED

**Effects:**
- Task state changes to FAILED
- Running attempt terminated
- Downstream tasks remain PENDING (blocked)

**Events:**
```
TaskAborted:
  task_id: <task-id>
  reason: <text>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`
- `TASK_ALREADY_TERMINAL`: Task is success or failed

**Idempotence:** Idempotent. Aborting failed task is no-op.

---

### 6.3 attempt inspect

**Synopsis:**
```
hivemind attempt inspect <attempt-id> [--context] [--diff] [--output]
```

**Preconditions:**
- Attempt exists

**Effects:** None (read-only)

**Output:**
- Attempt metadata
- With --context: retry context that was provided
- With --diff: changes made
- With --output: runtime output

**Events:** None

**Failures:**
- `ATTEMPT_NOT_FOUND`

**Idempotence:** Idempotent.

---

## 7. Verification Commands

### 7.1 verify override

**Synopsis:**
```
hivemind verify override <task-id> <pass|fail> --reason <text>
```

**Preconditions:**
- Task exists and is in VERIFYING state
- User has override authority

**Effects:**
- Verification outcome overridden
- Task transitions based on override (SUCCESS or FAILED)
- Override recorded with attribution

**Events:**
```
HumanOverride:
  task_id: <task-id>
  override_type: VERIFICATION_OVERRIDE
  decision: <pass|fail>
  reason: <text>
  user: <current-user>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_VERIFYING`: Task is not in verification state
- `OVERRIDE_NOT_PERMITTED`: Policy forbids override

**Idempotence:** Not idempotent. Only valid during VERIFYING state.

---

## 8. Merge Commands

### 8.1 merge prepare

**Synopsis:**
```
hivemind merge prepare <flow-id> [--target <branch>]
```

**Preconditions:**
- Flow exists
- Flow state is COMPLETED (success)
- No pending merge preparation

**Effects:**
- Integration commits computed
- Conflict check performed
- Merge preview generated

**Events:**
```
MergePrepared:
  flow_id: <flow-id>
  repos: [<repo-statuses>]
  conflicts: [<conflicts>]
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_COMPLETED`: Flow hasn't completed successfully
- `MERGE_ALREADY_PREPARED`: Preparation exists

**Idempotence:** Idempotent if no conflicts. Re-preparation refreshes.

---

### 8.2 merge approve

**Synopsis:**
```
hivemind merge approve <flow-id>
```

**Preconditions:**
- Flow exists
- Merge is prepared
- No unresolved conflicts

**Effects:**
- Merge marked as approved
- Ready for execution

**Events:**
```
MergeApproved:
  flow_id: <flow-id>
  user: <current-user>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `MERGE_NOT_PREPARED`: No merge preparation
- `UNRESOLVED_CONFLICTS`: Conflicts exist

**Idempotence:** Idempotent. Approving approved merge is no-op.

---

### 8.3 merge execute

**Synopsis:**
```
hivemind merge execute <flow-id>
```

**Preconditions:**
- Flow exists
- Merge is approved

**Effects:**
- Integration commits pushed to target branches
- Execution branches cleaned up (per policy)
- Flow marked as merged

**Events:**
```
MergeCompleted:
  flow_id: <flow-id>
  commits: [<commit-refs>]
```

**Failures:**
- `FLOW_NOT_FOUND`
- `MERGE_NOT_APPROVED`: Merge not approved
- `MERGE_CONFLICT`: Conflict occurred during merge
- `PUSH_FAILED`: Could not push to remote

**Idempotence:** Not idempotent. Cannot merge twice.

---

## 9. Event Commands

### 9.1 events stream

**Synopsis:**
```
hivemind events stream [--flow <flow-id>] [--task <task-id>] [--since <timestamp>]
```

**Preconditions:** None

**Effects:** None (read-only, streaming)

**Output:**
- Real-time event stream
- Filtered by specified criteria

**Events:** None (reads events, doesn't create)

**Failures:**
- `INVALID_FILTER`: Filter criteria invalid

**Idempotence:** N/A (streaming command)

---

### 9.2 events replay

**Synopsis:**
```
hivemind events replay <flow-id> [--until <timestamp>] [--verify]
```

**Preconditions:**
- Flow exists (or existed)

**Effects:**
- With --verify: compares replayed state to stored state
- Without --verify: outputs reconstructed state

**Output:**
- Reconstructed orchestration state
- Discrepancies (with --verify)

**Events:** None

**Failures:**
- `FLOW_NOT_FOUND`
- `EVENT_CORRUPTION`: Events are corrupt
- `STATE_MISMATCH`: Replayed state differs (with --verify)

**Idempotence:** Idempotent.

---

## 10. Output Contracts

### 10.1 Standard Output Format

All commands support:
```
--format json    # Machine-readable JSON
--format table   # Human-readable table
--format yaml    # YAML output
```

Default: table for TTY, json for pipes

### 10.2 Error Output Format

Errors are always structured:
```json
{
  "error": {
    "code": "TASK_NOT_FOUND",
    "message": "Task abc123 not found",
    "details": {}
  }
}
```

### 10.3 Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Command error (invalid args, precondition failed) |
| 2 | Resource not found |
| 3 | Conflict or invalid state |
| 4 | Permission denied |
| 10+ | System errors |

---

## 11. Invariants

CLI operational semantics guarantee:

- Every command has defined preconditions
- Every command has defined effects
- Effects only occur if preconditions met
- Failures are reported with structured errors
- Idempotence behavior is documented
- Events are emitted for state-changing operations

Violating these invariants is a SystemError.

---

## 12. Summary

This document defines **what happens** when CLI commands execute:

- **Preconditions:** What must be true
- **Effects:** What changes
- **Events:** What is recorded
- **Failures:** What can go wrong

The CLI is not just an interface. It is a contract.

> If the CLI says it happened, it happened. If it says it failed, it failed.
> There is no ambiguity.
