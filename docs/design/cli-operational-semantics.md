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

### 1.3 Global runtime environment

- `HIVEMIND_DATA_DIR` controls where local orchestration state is stored.
  - Default: `~/.hivemind`
  - Contents include `events.jsonl` plus generated artifacts/worktrees.
- For reproducible tests and isolated runs, set `HIVEMIND_DATA_DIR` explicitly.

### 1.4 version

**Synopsis:**
```
hivemind version
```

**Preconditions:** None

**Effects:** None (read-only)

**Events:** None

**Output:**
- `table`: `hivemind <version>`
- `json|yaml`: standard success wrapper containing `{ "name": "hivemind", "version": "..." }`

**Failures:** None

**Idempotence:** Idempotent.

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
- `project_exists`: Project with name exists
- `invalid_project_name`: Name is empty

**Idempotence:** Not idempotent. Second call fails.

---

### 2.2 project list

**Synopsis:**
```
hivemind [-f json|table|yaml] project list
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
- `project_not_found`: Project doesn't exist
- `repo_path_not_found`: Path does not exist
- `invalid_repository_path`: Empty path
- `not_a_git_repo`: Path is not a git repo
- `repo_already_attached`: Repo path already attached
- `repo_name_already_attached`: Repo name already used

**Idempotence:** Not idempotent. Second call fails.

---

### 2.4 project runtime-set

**Synopsis:**
```
hivemind project runtime-set <project> [--adapter <name>] [--binary-path <path>] [--model <model>] [--arg <arg>...] [--env KEY=VALUE...] [--timeout-ms <ms>]
```

**Preconditions:**
- Project `<project>` exists

**Effects:**
- Project runtime configuration is set (or replaced)
- Adapter selection becomes explicit for TaskFlow execution
- Runtime changes during active flows are allowed and apply to future task executions/ticks.
  Running attempts are not retroactively mutated.

**Events:**
```
ProjectRuntimeConfigured:
  project_id: <project_id>
  adapter_name: <name>
  binary_path: <path>
  model: <model> | null
  args: [<args...>]
  env: { <key>: <value>, ... }
  timeout_ms: <ms>
```

**Failures:**
- `project_not_found`
- `invalid_env`: invalid env formatting

**Idempotence:** Idempotent if config is unchanged. Otherwise emits a new configuration event.

---

### 2.5 project inspect

**Synopsis:**
```
hivemind [-f json|table|yaml] project inspect <project>
```

**Preconditions:**
- Project exists

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `project_not_found`

**Idempotence:** Idempotent.

---

### 2.6 project update

**Synopsis:**
```
hivemind project update <project> [--name <name>] [--description <text>]
```

**Preconditions:**
- Project exists

**Effects:**
- Project metadata updated

**Events:**
```
ProjectUpdated:
  project_id: <project_id>
  name: <name> | null
  description: <text> | null
```

**Failures:**
- `project_not_found`
- `invalid_project_name`: Name is empty
- `project_name_conflict`: Name already taken

**Idempotence:** Idempotent if no changes. Otherwise emits a new update event.
No `ProjectUpdated` event is emitted when both name and description are unchanged.

---

### 2.7 project detach-repo

**Synopsis:**
```
hivemind project detach-repo <project> <repo-name>
```

**Preconditions:**
- Project exists
- Repository with `<repo-name>` is attached

**Effects:**
- Repository reference removed from project

**Events:**
```
RepositoryDetached:
  project_id: <project_id>
  name: <repo-name>
```

**Failures:**
- `project_not_found`
- `repo_not_found`

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
- `project_not_found`: Project doesn't exist
- `invalid_task_title`: Title is empty
- `invalid_scope`: Scope definition is malformed

**Idempotence:** Not idempotent. Creates new task each time.

---

### 3.2 task list

**Synopsis:**
```
hivemind [-f json|table|yaml] task list <project> [--state <state>]
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
- `task_not_found`: Task doesn't exist
- `task_in_active_flow`: Task is part of running TaskFlow

**Idempotence:** Idempotent. Closing closed task is no-op.

---

### 3.4 task inspect

**Synopsis:**
```
hivemind [-f json|table|yaml] task inspect <task-id>
```

**Preconditions:**
- Task ID is a valid UUID
- Task exists

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `invalid_task_id`: Task ID is not a valid UUID
- `task_not_found`: Task doesn't exist

**Idempotence:** Idempotent.

---

### 3.5 task update

**Synopsis:**
```
hivemind task update <task-id> [--title <text>] [--description <text>]
```

**Preconditions:**
- Task exists

**Effects:**
- Task metadata updated

**Events:**
```
TaskUpdated:
  task_id: <task-id>
  title: <text> | null
  description: <text> | null
```

**Failures:**
- `invalid_task_id`: Task ID is not a valid UUID
- `task_not_found`: Task doesn't exist
- `invalid_task_title`: Title is empty

**Idempotence:** Idempotent if no changes. Otherwise emits a new update event.

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
- `<to-task>` must complete before `<from-task>` can start

**Events:**
```
DependencyAdded:
  graph_id: <graph-id>
  from_task: <from-task>
  to_task: <to-task>
```

**Failures:**
- `graph_not_found`
- `graph_immutable`: Graph already used in a flow; error message may include locking flow ID
- `task_not_in_graph`
- `cycle_detected`: Dependency would create cycle

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

### 4.4 graph list

**Synopsis:**
```
hivemind [-f json|table|yaml] graph list [--project <project-id-or-name>]
```

**Preconditions:**
- If `--project` is supplied, project exists

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `project_not_found`: Provided project filter does not resolve

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
hivemind flow status <flow-id>
```

**Preconditions:**
- Flow exists

**Effects:** None (read-only)

**Output:**
- Flow state
- Task states summary
- Progress metrics
- Current activity
- In `json|yaml`, successful responses follow `{ success: true, data: <flow> }`

**Events:** None

**Failures:**
- `FLOW_NOT_FOUND`

**Idempotence:** Idempotent.

---

### 5.7 flow list

**Synopsis:**
```
hivemind [-f json|table|yaml] flow list [--project <project-id-or-name>]
```

**Preconditions:**
- If `--project` is supplied, project exists

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `project_not_found`: Provided project filter does not resolve

**Idempotence:** Idempotent.

---

### 5.8 flow tick

**Synopsis:**
```
hivemind flow tick <flow-id> [--interactive]
```

`--interactive` is introduced in **Phase 15: Interactive Runtime Sessions (CLI)**.

**Preconditions:**
- Flow exists
- Flow state is RUNNING
- Project has runtime configured

**Effects:**
- Transitions any dependency-satisfied `PENDING` tasks to `READY`
- Executes a single `READY` task attempt using the configured runtime adapter
- Emits runtime lifecycle events correlated by attempt ID
- Optionally emits projected runtime observational events from stdout/stderr parsing

If `--interactive` is provided and the selected adapter supports interactive execution:

- The CLI runs the attempt in an interactive session
- Runtime output is streamed continuously
- User input is forwarded to the runtime
- Ctrl+C interrupts the runtime deterministically and records the interruption

**Events:**
```
TaskReady:
  flow_id: <flow-id>
  task_id: <task-id>

TaskExecutionStateChanged:
  task_id: <task-id>
  from: READY
  to: RUNNING

RuntimeStarted:
  attempt_id: <attempt-id>
  task_id: <task-id>

RuntimeOutputChunk:
  attempt_id: <attempt-id>
  stream: stdout|stderr
  content: <text>

RuntimeCommandObserved:
  attempt_id: <attempt-id>
  stream: stdout|stderr
  command: <text>

RuntimeToolCallObserved:
  attempt_id: <attempt-id>
  stream: stdout|stderr
  tool_name: <text>
  details: <text>

RuntimeTodoSnapshotUpdated:
  attempt_id: <attempt-id>
  stream: stdout|stderr
  items: [<todo-lines...>]

RuntimeNarrativeOutputObserved:
  attempt_id: <attempt-id>
  stream: stdout|stderr
  content: <text>

RuntimeInputProvided:
  attempt_id: <attempt-id>
  content: <text>

RuntimeInterrupted:
  attempt_id: <attempt-id>

RuntimeFilesystemObserved:
  attempt_id: <attempt-id>
  files_created: [<paths...>]
  files_modified: [<paths...>]
  files_deleted: [<paths...>]

RuntimeExited:
  attempt_id: <attempt-id>
  exit_code: <code>

TaskExecutionStateChanged:
  task_id: <task-id>
  from: RUNNING
  to: VERIFYING
```

**Projection invariant:**
- Runtime projection events are observational telemetry only
- They must never drive scheduling, verification outcomes, retry policy, or merge decisions
- Projection failure must not fail task execution

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_RUNNING`
- `RUNTIME_NOT_CONFIGURED`
- `WORKTREE_NOT_FOUND`
- `UNSUPPORTED_RUNTIME`
- `INTERACTIVE_MODE_UNSUPPORTED`: `--interactive` was provided but the adapter does not support interactive execution

**Idempotence:** Not idempotent. Each tick may schedule and/or execute work.

---

## 6. Task Execution Commands

### 6.1 task start

**Synopsis:**
```
hivemind task start <task-id>
```

**Preconditions:**
- Task exists and is part of a TaskFlow
- Flow is RUNNING
- Task execution state is READY or RETRY

Note: the task worktree is created/ensured as part of this command.

**Effects:**
- Task execution state transitions to RUNNING
- A new attempt is created for the task
- A filesystem baseline is captured for that attempt
- Baseline artifact is persisted under `.hivemind/artifacts/baselines/`

**Events:**
```
TaskExecutionStateChanged:
  task_id: <task-id>
  from: READY|RETRY
  to: RUNNING

AttemptStarted:
  task_id: <task-id>
  attempt_id: <generated>

BaselineCaptured:
  task_id: <task-id>
  attempt_id: <attempt-id>
  baseline_id: <generated>
  git_head: <sha>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`: Task is not part of any TaskFlow
- `FLOW_NOT_RUNNING`: Flow is not in RUNNING state
- `TASK_NOT_READY`: Task is not in READY or RETRY state
- `WORKTREE_NOT_FOUND`: Worktree not found for task
- `BASELINE_CAPTURE_FAILED`: Baseline capture failed

**Idempotence:** Not idempotent.

---

### 6.2 task complete

**Synopsis:**
```
hivemind task complete <task-id>
```

**Preconditions:**
- Task exists and is part of a TaskFlow
- Task execution state is RUNNING
- The current attempt has an associated baseline
- All active checkpoints for the attempt are completed

**Effects:**
- Task execution state transitions to VERIFYING
- Changes are detected and unified diffs are computed against the baseline
- Diff artifact is persisted under `.hivemind/artifacts/diffs/`
- A best-effort checkpoint commit may be created in the task worktree

**Events:**
```
TaskExecutionStateChanged:
  task_id: <task-id>
  from: RUNNING
  to: VERIFYING

FileModified:
  task_id: <task-id>
  attempt_id: <attempt-id>
  path: <path>
  change_type: CREATED|MODIFIED|DELETED

DiffComputed:
  task_id: <task-id>
  attempt_id: <attempt-id>
  diff_id: <generated>
  baseline_id: <baseline-id>

CheckpointCommitCreated:
  task_id: <task-id>
  attempt_id: <attempt-id>
  commit_sha: <sha>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`
- `TASK_NOT_RUNNING`: Task is not in RUNNING state
- `checkpoints_incomplete`: One or more active checkpoints are not yet completed
- `BASELINE_NOT_FOUND`: Baseline artifact missing for attempt
- `DIFF_COMPUTE_FAILED`: Diff computation failed

**Idempotence:** Not idempotent.

---

### 6.3 task retry

**Synopsis:**
```
hivemind task retry <task-id> [--reset-count] [--mode clean|continue]
```

**Preconditions:**
- Task exists and is in a TaskFlow
- Task state is FAILED or RETRY
- Retry limit not exceeded (unless --reset-count)

**Effects:**
- Task state changes to PENDING (if dependencies met) or appropriate state
- New attempt will be scheduled
- With --reset-count: retry counter reset to 0
- With --mode clean (default): execution worktree is reset to the flow base revision
- With --mode continue: execution worktree is preserved (no reset)

**Events:**
```
TaskRetryRequested:
  task_id: <task-id>
  reset_count: <boolean>
  retry_mode: clean|continue
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`: Task is not part of any TaskFlow
- `TASK_NOT_RETRIABLE`: Task is not in retriable state
- `RETRY_LIMIT_EXCEEDED`: And --reset-count not specified
- `git_checkout_failed`: Clean retry failed to reset the execution branch/worktree
- `git_clean_failed`: Clean retry failed to clean untracked files in the execution worktree

**Idempotence:** Not idempotent. Each call queues a retry.

---

### 6.4 task abort

**Synopsis:**
```
hivemind task abort <task-id> [--reason <text>]
```

**Preconditions:**
- Task exists and is in a TaskFlow
- Task is not already SUCCESS

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

### 6.5 attempt inspect

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

**Notes:**
- The CLI primarily expects `<attempt-id>`. For backwards compatibility, `<task-id>` may be accepted and will return the latest attempt information for that task within its flow.
- With `--diff`, the CLI prints the stored unified diff artifact (if one has been computed).

**Events:** None

**Failures:**
- `ATTEMPT_NOT_FOUND`

**Idempotence:** Idempotent.

---

### 6.6 worktree list

**Synopsis:**
```
hivemind worktree list <flow-id>
```

**Preconditions:**
- Flow exists
- Flow belongs to a project with exactly one attached repository

**Effects:** None (read-only)

**Output:**
- Worktree status list for each task in the flow

**Events:** None

**Failures:**
- `FLOW_NOT_FOUND`
- `PROJECT_HAS_NO_REPO`: No repository attached
- `MULTIPLE_REPOS_UNSUPPORTED`: More than one repository attached

**Idempotence:** Idempotent.

---

### 6.7 worktree inspect

**Synopsis:**
```
hivemind worktree inspect <task-id>
```

**Preconditions:**
- Task exists and is part of a flow
- Flow belongs to a project with exactly one attached repository

**Effects:** None (read-only)

**Output:**
- Worktree status for the task, including:
  - expected worktree path
  - whether the path is a valid git worktree
  - current branch and HEAD (if available)

**Events:** None

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`
- `PROJECT_HAS_NO_REPO`: No repository attached
- `MULTIPLE_REPOS_UNSUPPORTED`: More than one repository attached

**Idempotence:** Idempotent.

---

### 6.8 worktree cleanup

**Synopsis:**
```
hivemind worktree cleanup <flow-id>
```

**Preconditions:**
- Flow exists
- Flow belongs to a project with exactly one attached repository

**Effects:**
- Removes all task worktrees for the flow under `.hivemind/worktrees/<flow-id>/...`

**Events:** None

**Failures:**
- `FLOW_NOT_FOUND`
- `PROJECT_HAS_NO_REPO`: No repository attached
- `MULTIPLE_REPOS_UNSUPPORTED`: More than one repository attached
- `GIT_WORKTREE_FAILED`: Worktree remove fails

**Idempotence:** Idempotent (no-op if worktrees are absent).

---

## 7. Verification Commands

### 7.1 verify override

**Synopsis:**
```
hivemind verify override <task-id> <pass|fail> --reason <text>
```

**Preconditions:**
- Task exists and is part of a TaskFlow
- Task execution state is `verifying`, `retry`, `failed`, or `escalated`
- `--reason` is non-empty
- User has override authority

**Effects:**
- Verification outcome overridden
- Task transitions based on override (SUCCESS or FAILED)
- Override recorded with attribution
- If the override causes all tasks in the flow to be `success`, the flow may complete

**Events:**
```
HumanOverride:
  task_id: <task-id>
  override_type: VERIFICATION_OVERRIDE
  decision: <pass|fail>
  reason: <text>
  user: <current-user>
```

**Attribution:**
- The CLI/registry will attribute the override to `HIVEMIND_USER` if set, otherwise `USER`.

**Failures:**
- `invalid_task_id`
- `task_not_in_flow`: Task is not part of any flow
- `flow_not_active`: Flow is completed or aborted
- `invalid_decision`: Decision must be `pass` or `fail`
- `invalid_reason`: Reason must be non-empty
- `task_not_overridable`: Task is not in an overridable state

**Idempotence:** Not idempotent. Only valid while the task is in an overridable execution state.

---

## 8. Merge Commands

### 8.1 merge prepare

**Synopsis:**
```
hivemind merge prepare <flow-id> [--target <branch>]
```

**Preconditions:**
- Flow exists
- Flow state is COMPLETED (success) or already FROZEN_FOR_MERGE
- No pending merge preparation

**Effects:**
- Flow is frozen for merge (`FlowFrozenForMerge`) if not already frozen
- Per-flow integration lock acquired (`FlowIntegrationLockAcquired`)
- Integration sandbox created/refreshed (`integration/<flow-id>/prepare` + `_integration_prepare` worktree)
- Per-task integration sandboxes created (`integration/<flow-id>/<task-id>`) and promoted into the flow prepare branch
- Drift checks enforce dependency ancestry before task integration
- Integration branch `flow/<flow-id>` rewritten from prepared sandbox result
- Conflict check performed (no auto-resolution)
- Graph-level integration checks run inside the sandbox (required checks gate success)

**Git Artifacts (deterministic):**
- Flow-scoped integration sandbox branch: `integration/<flow-id>/prepare`
- Flow-scoped integration sandbox worktree: `<data-dir>/<flow-id>/_integration_prepare`
- Task-scoped integration branches: `integration/<flow-id>/<task-id>`
- Flow integration branch refreshed: `flow/<flow-id>`
- Successful task execution branches consumed as inputs: `exec/<flow-id>/<task-id>`

**Events:**
```
FlowFrozenForMerge:
  flow_id: <flow-id>

FlowIntegrationLockAcquired:
  flow_id: <flow-id>
  operation: merge_prepare

TaskIntegratedIntoFlow:
  flow_id: <flow-id>
  task_id: <task-id>
  commit_sha: <integration-commit>

MergeConflictDetected:
  flow_id: <flow-id>
  task_id: <task-id|null>
  details: <conflict-detail>

MergeCheckStarted:
  flow_id: <flow-id>
  check_name: <name>
  required: <bool>

MergeCheckCompleted:
  flow_id: <flow-id>
  check_name: <name>
  passed: <bool>
  exit_code: <int>
  output: <captured stdout/stderr>
  duration_ms: <uint>
  required: <bool>

MergePrepared:
  flow_id: <flow-id>
  repos: [<repo-statuses>]
  conflicts: [<conflicts>]
  target_branch: <branch>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_COMPLETED`: Flow hasn't completed successfully
- `MERGE_ALREADY_PREPARED`: Preparation exists without conflicts
- `PROJECT_HAS_NO_REPO`
- `MULTIPLE_REPOS_UNSUPPORTED`
- `DETACHED_HEAD`: Target cannot be inferred (default prefers `main` when present)
- `UNRESOLVED_CONFLICTS`: Conflicts or required check failures

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
- `MERGE_ALREADY_APPROVED`: Approval is idempotent but enforced server-side

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
- Flow is frozen for merge

**Effects:**
- Per-flow integration lock acquired (`FlowIntegrationLockAcquired`)
- Target branch fast-forward applied from prepared integration branch `integration/<flow>/prepare`
- Sandbox worktrees/branches cleaned up (`_integration_prepare`, `integration/<flow>/prepare`, `integration/<flow>/<task>`, `_merge`)
- Execution branches cleaned up (per project policy)
- Flow marked as merged

**Git Artifacts (deterministic):**
- The prepared integration branch `integration/<flow-id>/prepare` is merged into the selected target branch.

**Events:**
```
FlowIntegrationLockAcquired:
  flow_id: <flow-id>
  operation: merge_execute

MergeCompleted:
  flow_id: <flow-id>
  commits: [<commit-refs>]
```

**Failures:**
- `FLOW_NOT_FOUND`
- `MERGE_NOT_PREPARED`
- `MERGE_NOT_APPROVED`
- `FLOW_NOT_FROZEN_FOR_MERGE`
- `MERGE_CONFLICT`: Conflict occurred during final fast-forward
- `PUSH_FAILED`: Could not update target

**Idempotence:** Not idempotent. Cannot merge twice.

---

## 9. Event Commands

### 9.1 events stream

**Synopsis:**
```
hivemind events stream [--flow <flow-id>] [--task <task-id>] [--project <project-id>] [--graph <graph-id>] [--limit <n>]
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
hivemind events replay <flow-id> [--verify]
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
-f, --format json    # Machine-readable JSON
-f, --format table   # Human-readable output
-f, --format yaml    # YAML output
```

Default: table

For machine-readable success payloads (`json|yaml`), commands emit:
```json
{
  "success": true,
  "data": { "...": "..." }
}
```

Some ID-returning commands may return a minimal object in `data` (for example, a single `flow_id` or `graph_id`).

### 10.2 Error Output Format

Errors are always structured:
```json
{
  "success": false,
  "error": {
    "category": "user",
    "code": "task_not_found",
    "message": "Task abc123 not found",
    "origin": "registry:get_task",
    "hint": "Try again"
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
