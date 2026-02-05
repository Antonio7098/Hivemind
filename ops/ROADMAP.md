# Hivemind Master Roadmap

> **Principle 11:** Build incrementally, prove foundations first.
> Complexity is earned. Start minimal. Prove invariants. Expand deliberately.

This roadmap builds Hivemind from absolute fundamentals. Each phase must be complete and proven before moving to the next. No shortcuts.

---

## How to Use This Roadmap

- [ ] marks incomplete items
- [x] marks complete items
- Each phase has **exit criteria** that must pass before proceeding
- If something breaks in a later phase, fix it in the earliest affected phase

---

## Phase 0: Project Bootstrap

**Goal:** Establish the development foundation.

### 0.1 Repository Setup
- [x] Initialize Rust workspace (`cargo init --name hivemind`)
- [x] Create directory structure:
  ```
  src/
    main.rs           # CLI entrypoint
    lib.rs            # Library root
    cli/              # CLI commands
    core/             # Core domain (events, state, errors)
    adapters/         # Runtime adapters
    storage/          # Event storage
  tests/              # Integration tests
  ```
- [x] Set up Makefile (build, test, lint, fmt)
- [x] Configure clippy (strict: `-D warnings`)
- [x] Configure rustfmt
- [ ] Configure cargo-nextest for test runner

### 0.2 CI Foundation
- [x] GitHub Actions for build
- [x] GitHub Actions for tests
- [x] GitHub Actions for lint
- [x] All CI must pass before any merge

### 0.3 Exit Criteria
- [x] `make build` produces binary
- [x] `make test` runs (even with zero tests)
- [x] `make lint` passes
- [x] CI runs on every PR

---

## Phase 1: Event Foundation

**Goal:** Establish the single source of truth. All state will derive from events.

> **Principle 1:** Observability is truth. If it's not observable, it didn't happen.

### 1.1 Event Core Types
- [x] Define `Event` interface
- [x] Define `EventID` (unique, ordered)
- [x] Define `Timestamp` handling (monotonic within flow)
- [x] Define `CorrelationIDs` (project, flow, task, attempt)
- [x] Define event serialization format (JSON initially)

### 1.2 Event Store Interface
- [x] Define `EventStore` interface:
  - `Append(event) -> EventID`
  - `Read(filter) -> []Event`
  - `Stream(filter) -> chan Event`
- [x] Implement file-based EventStore (append-only file per flow)
- [x] Implement in-memory EventStore (for testing)

### 1.3 Event Replay
- [x] Implement `Replay(events) -> State` function
- [x] Prove: replay is deterministic (same events → same state)
- [x] Prove: replay is idempotent (replay twice → same result)

### 1.4 Core Event Types (Minimal Set)
- [x] `ProjectCreated`
- [x] `ProjectUpdated`
- [x] `TaskCreated`
- [x] `TaskUpdated`
- [x] `TaskClosed`

### 1.5 Exit Criteria
- [x] Events can be appended and read
- [x] Events survive process restart (file store)
- [x] Replay produces identical state from identical events
- [x] Tests prove determinism and idempotency
- [x] No state exists outside events

---

## Phase 2: Error Model

**Goal:** Establish structured error handling before any complex logic.

> **Principle 4:** Errors must be classifiable, attributable, and actionable.

### 2.1 Error Types
- [x] Define `HivemindError` struct:
  - Category (System, Runtime, Agent, Scope, Verification, Git, User, Policy)
  - Code (unique within category)
  - Message (human-readable)
  - Origin (component:identifier)
  - Recoverable (bool)
  - RecoveryHint (optional)
  - Context (map)
- [x] Implement error constructors for each category

### 2.2 Error Events
- [ ] Define `ErrorOccurred` event
- [x] Errors emit events (no silent failures)

### 2.3 Exit Criteria
- [x] All error categories defined
- [x] All errors produce structured output
- [x] Errors include actionable recovery hints
- [x] Tests verify error classification

---

## Phase 3: CLI Skeleton

**Goal:** CLI-first is non-negotiable. Build the CLI shell before features.

> **Principle 7:** If it cannot be done via CLI, it is not a real feature.

### 3.1 CLI Framework
- [x] Choose CLI framework (clap recommended)
- [x] Implement root command with version
- [x] Implement global flags (--format, --verbose)
- [x] Implement structured output (JSON, table via comfy-table or similar)

### 3.2 Output Contract
- [x] JSON output for all commands when `--format json`
- [x] Structured error output (code, message, details)
- [x] Exit codes per spec (0=success, 1=error, 2=not found, etc.)

### 3.3 Stub Commands
- [x] `hivemind version`
- [x] `hivemind project` (subcommand group)
- [x] `hivemind task` (subcommand group)
- [x] `hivemind events` (subcommand group)

### 3.4 Exit Criteria
- [x] CLI parses commands correctly
- [x] `--format json` produces valid JSON
- [x] Errors produce structured output
- [x] Exit codes follow spec
- [x] `hivemind --help` is useful

---

## Phase 4: Project Registry

**Goal:** Projects are the top-level organizational unit.

### 4.1 Project State
- [x] Define Project struct (ID, name, description, created_at)
- [x] Derive project state from events via replay

### 4.2 Project Commands
- [x] `hivemind project create <name>` → emits `ProjectCreated`
- [x] `hivemind project list` → reads from derived state
- [x] `hivemind project inspect <id>` → shows project details
- [x] `hivemind project update <id>` → emits `ProjectUpdated`

### 4.3 Project Registry Storage
- [x] Define registry location (`~/.hivemind/` or configurable)
- [x] Event log per project
- [ ] Registry index (project ID → event log path)

### 4.4 Exit Criteria
- [x] Can create, list, inspect projects
- [x] Projects survive restart (events persisted)
- [x] State is derived purely from events
- [x] CLI output matches spec

---

## Phase 5: Repository Attachment

**Goal:** Projects reference repositories (git).

### 5.1 Repository Events
- [x] `RepositoryAttachedToProject`
- [x] `RepositoryDetachedFromProject`

### 5.2 Repository Commands
- [x] `hivemind project attach-repo <project> <path>` → validates git repo, emits event
- [x] `hivemind project detach-repo <project> <repo-name>`
- [x] Show attached repos in `project inspect`

### 5.3 Repository Validation
- [x] Verify path is a git repository
- [x] Verify repo is accessible
- [x] Store repo path and access mode (ro/rw)

### 5.4 Exit Criteria
- [x] Can attach/detach repos to projects
- [x] Invalid paths rejected with clear error
- [x] Repo state derived from events
- [x] `project inspect` shows attached repos

---

## Phase 6: Task Management (Lightweight)

**Goal:** Tasks exist as units of intent, independent of execution.

> **User Story 1:** Simple todo tracking without automation.

### 6.1 Task Events
- [x] `TaskCreated`
- [x] `TaskUpdated`
- [x] `TaskClosed`

### 6.2 Task State
- [x] Define Task struct (ID, project_id, title, description, state, created_at)
- [x] Task states: OPEN, CLOSED
- [x] Derive task state from events

### 6.3 Task Commands
- [x] `hivemind task create <project> <title>`
- [x] `hivemind task list <project>` (with filters: --state)
- [x] `hivemind task inspect <task-id>`
- [x] `hivemind task update <task-id>`
- [x] `hivemind task close <task-id>`

### 6.4 Exit Criteria
- [x] Tasks work as simple todo list
- [x] Tasks survive restart
- [x] State derived from events
- [x] User Story 1 is achievable

---

## Phase 7: Scope Model (Definitions Only)

**Goal:** Define scope contracts before enforcement.

### 7.1 Scope Types
- [x] Define FilesystemScope (paths, read/write/deny)
- [x] Define RepositoryScope (repo, access mode)
- [x] Define ExecutionScope (allowed commands)
- [x] Define GitScope (may commit, may branch)

### 7.2 Scope Declaration
- [x] Scope attached to tasks (optional at this phase)
- [x] Scope serialization (YAML/JSON)
- [x] Scope validation (well-formed)

### 7.3 Scope Compatibility
- [x] Implement compatibility check between two scopes
- [x] Compatible / Soft Conflict / Hard Conflict classification
- [x] Unit tests for compatibility rules

### 7.4 Exit Criteria
- [x] Scopes can be defined and validated
- [x] Compatibility can be computed
- [x] No enforcement yet (definitions only)

---

## Phase 8: TaskGraph (Static Planning)

**Goal:** TaskGraphs represent immutable intent (DAG of tasks).

### 8.1 TaskGraph Events
- [ ] `TaskGraphCreated`
- [ ] `TaskAddedToGraph`
- [ ] `DependencyAdded`
- [ ] `ScopeAssigned`
- [ ] `TaskGraphValidated`
- [ ] `TaskGraphLocked` (becomes immutable)

### 8.2 TaskGraph State
- [ ] Define TaskGraph struct (ID, project_id, tasks, edges, scopes)
- [ ] Derive from events
- [ ] DAG validation (no cycles)
- [ ] Mutable until locked

### 8.3 TaskGraph Commands
- [ ] `hivemind graph create <project> <name>`
- [ ] `hivemind graph add-task <graph> <task>`
- [ ] `hivemind graph add-dependency <graph> <from> <to>`
- [ ] `hivemind graph set-scope <graph> <task> <scope>`
- [ ] `hivemind graph validate <graph>`
- [ ] `hivemind graph inspect <graph>`

### 8.4 Exit Criteria
- [ ] Can build TaskGraphs from tasks
- [ ] Dependencies form valid DAG
- [ ] Cycle detection works
- [ ] Graph is immutable after lock

---

## Phase 9: TaskFlow Foundation (No Execution)

**Goal:** TaskFlow is the runtime instance of a TaskGraph.

### 9.1 TaskFlow Events
- [ ] `TaskFlowCreated`
- [ ] `TaskFlowStarted`
- [ ] `TaskFlowPaused`
- [ ] `TaskFlowResumed`
- [ ] `TaskFlowCompleted`
- [ ] `TaskFlowAborted`

### 9.2 TaskFlow State
- [ ] Define TaskFlow struct (ID, graph_id, state, created_at)
- [ ] Flow states: CREATED, RUNNING, PAUSED, COMPLETED, ABORTED
- [ ] Derive from events

### 9.3 TaskFlow Commands
- [ ] `hivemind flow create <graph-id>`
- [ ] `hivemind flow start <flow-id>` (state change only, no execution yet)
- [ ] `hivemind flow pause <flow-id>`
- [ ] `hivemind flow resume <flow-id>`
- [ ] `hivemind flow abort <flow-id>`
- [ ] `hivemind flow status <flow-id>`

### 9.4 Exit Criteria
- [ ] TaskFlow lifecycle works (create → start → pause → resume → complete)
- [ ] State transitions emit events
- [ ] State derived from events
- [ ] Flow can be paused and resumed

---

## Phase 10: Task Execution State Machine (No Runtime)

**Goal:** Task execution FSM without actual agent execution.

### 10.1 TaskExecution Events
- [ ] `TaskReady`
- [ ] `TaskBlocked`
- [ ] `TaskExecutionStarted`
- [ ] `TaskExecutionStateChanged`
- [ ] `TaskExecutionSucceeded`
- [ ] `TaskExecutionFailed`

### 10.2 TaskExecution States
- [ ] PENDING → RUNNING → VERIFYING → SUCCESS
- [ ] PENDING → RUNNING → VERIFYING → RETRY
- [ ] PENDING → RUNNING → VERIFYING → FAILED
- [ ] RETRY → RUNNING (bounded)
- [ ] FAILED → ESCALATED (optional)

### 10.3 Scheduler (Dependency Resolution)
- [ ] Task becomes READY when all upstream tasks are SUCCESS
- [ ] Task is BLOCKED while dependencies are pending
- [ ] Emit `TaskReady` / `TaskBlocked` events

### 10.4 Manual State Transitions (Testing)
- [ ] `hivemind task-exec start <task>` (simulate PENDING → RUNNING)
- [ ] `hivemind task-exec complete <task>` (simulate → SUCCESS)
- [ ] `hivemind task-exec fail <task>` (simulate → FAILED)
- [ ] Used for testing FSM without runtime

### 10.5 Exit Criteria
- [ ] FSM transitions are correct
- [ ] Scheduler releases tasks in dependency order
- [ ] All transitions emit events
- [ ] Manual simulation works for testing

---

## Phase 11: Worktree Management

**Goal:** Isolated git worktrees for task execution.

### 11.1 Worktree Operations
- [ ] Create worktree from base revision
- [ ] Delete worktree
- [ ] List worktrees for flow

### 11.2 Worktree Lifecycle
- [ ] Create on task start
- [ ] Preserve on failure (for debugging)
- [ ] Delete on success (configurable)

### 11.3 Worktree Commands
- [ ] `hivemind worktree list <flow-id>`
- [ ] `hivemind worktree inspect <task-id>`
- [ ] `hivemind worktree cleanup <flow-id>` (manual cleanup)

### 11.4 Exit Criteria
- [ ] Worktrees created in `.hivemind/worktrees/`
- [ ] Worktrees are valid git worktrees
- [ ] Worktrees isolated per task

---

## Phase 12: Baseline & Diff Computation

**Goal:** Observe what changed during execution.

### 12.1 Baseline Snapshot
- [ ] Capture file list + hashes before execution
- [ ] Capture git HEAD

### 12.2 Change Detection
- [ ] Compare post-execution state to baseline
- [ ] Identify: created, modified, deleted files
- [ ] Compute unified diffs

### 12.3 Diff Events
- [ ] `FileModified`
- [ ] `DiffComputed`
- [ ] `CheckpointCommitCreated`

### 12.4 Exit Criteria
- [ ] Diffs computed accurately
- [ ] Diffs attributed to task/attempt
- [ ] Changes observable via events

---

## Phase 13: Runtime Adapter Interface

**Goal:** Define the contract for runtime adapters.

### 13.1 Adapter Interface
- [ ] Define RuntimeAdapter interface:
  - `Initialize() -> error`
  - `Prepare(task, worktree) -> error`
  - `Execute(input) -> ExecutionReport`
  - `Terminate() -> error`

### 13.2 Execution Report
- [ ] Define ExecutionReport struct:
  - exit_code, duration, stdout, stderr
  - files_created, files_modified, files_deleted
  - errors

### 13.3 Adapter Events
- [ ] `RuntimeStarted`
- [ ] `RuntimeOutputChunk`
- [ ] `RuntimeExited`
- [ ] `RuntimeTerminated`

### 13.4 Exit Criteria
- [ ] Interface defined and documented
- [ ] ExecutionReport captures all needed data
- [ ] Events defined for all lifecycle phases

---

## Phase 14: OpenCode Adapter (First Runtime)

**Goal:** Wrap OpenCode CLI as first runtime.

### 14.1 Adapter Implementation
- [ ] Implement OpenCodeAdapter
- [ ] Binary detection and health check
- [ ] Subprocess launch with controlled environment
- [ ] Input delivery (stdin)
- [ ] Output capture (stdout/stderr)
- [ ] Timeout enforcement

### 14.2 Configuration
- [ ] Adapter config in project/global settings
- [ ] Binary path, args, timeout, env passthrough

### 14.3 Integration
- [ ] Adapter selected per project
- [ ] Adapter invoked by TaskFlow engine

### 14.4 Exit Criteria
- [ ] OpenCode can be launched
- [ ] Output captured
- [ ] Timeout works
- [ ] Filesystem changes observed

---

## Phase 15: Scope Enforcement (Phase 1: Detection)

**Goal:** Detect scope violations post-execution.

> **Honest:** Phase 1 is detection, not prevention.

### 15.1 Scope Verification
- [ ] After execution, check all changes against scope
- [ ] Filesystem scope: all modified files within allowed paths
- [ ] Git scope: commits/branches only if permitted

### 15.2 Violation Handling
- [ ] `ScopeViolationDetected` event
- [ ] Attempt fails immediately on violation
- [ ] Worktree preserved for debugging
- [ ] Clear error message with violation details

### 15.3 Exit Criteria
- [ ] Violations detected reliably
- [ ] Violations are fatal to attempt
- [ ] Violations emit observable events
- [ ] Honest: prevention is Phase 2+

---

## Phase 16: Verification Framework

**Goal:** Automated checks are the primary gate.

### 16.1 Check Execution
- [ ] Run configured checks (test commands)
- [ ] Capture exit code, output, duration
- [ ] `CheckStarted`, `CheckCompleted` events

### 16.2 Check Results
- [ ] Define CheckResult struct
- [ ] Required vs optional checks
- [ ] Aggregation: all required must pass

### 16.3 Verification Commands
- [ ] `hivemind verify run <task-id>` (manual verification)
- [ ] `hivemind verify results <attempt-id>`

### 16.4 Exit Criteria
- [ ] Checks run in worktree
- [ ] Results captured and observable
- [ ] Required check failures block success

---

## Phase 17: Verifier Agent (Advisory)

**Goal:** LLM-based verification as advisory layer.

> **Important:** Verifier is advisory, not authoritative.

### 17.1 Verifier Invocation
- [ ] After checks, invoke verifier agent
- [ ] Input: task definition, diff, check results
- [ ] Output: PASS / SOFT_FAIL / HARD_FAIL + feedback

### 17.2 Verifier Events
- [ ] `VerificationStarted`
- [ ] `VerificationCompleted`

### 17.3 Integration
- [ ] Verifier decision informs retry/fail
- [ ] Verifier cannot override failed checks
- [ ] Verifier feedback included in retry context

### 17.4 Exit Criteria
- [ ] Verifier produces structured decision
- [ ] Verifier is advisory only
- [ ] Feedback captured for retry

---

## Phase 18: Retry Mechanics

**Goal:** Enable intelligent retries with explicit context.

### 18.1 Retry Context Assembly
- [ ] Gather prior attempt outcomes
- [ ] Gather check results
- [ ] Gather verifier feedback
- [ ] Compute actionable feedback

### 18.2 Retry Context Delivery
- [ ] Context is explicit input (not hidden memory)
- [ ] Context visible in attempt events
- [ ] Mechanical feedback prioritized over advisory

### 18.3 Retry Policy
- [ ] Max attempts per task
- [ ] Retry on soft fail, not on hard fail
- [ ] Bounded retries (no infinite loops)

### 18.4 Exit Criteria
- [ ] Retries receive explicit context
- [ ] Context is observable
- [ ] Retries are bounded

---

## Phase 19: Human Override

**Goal:** Humans are ultimate authority.

### 19.1 Override Actions
- [ ] Override check failures
- [ ] Override verifier decision
- [ ] Direct approval/rejection
- [ ] Manual retry with reset count

### 19.2 Override Events
- [ ] `HumanOverride` with attribution and reason

### 19.3 Override Commands
- [ ] `hivemind verify override <task-id> pass|fail --reason <text>`
- [ ] `hivemind task retry <task-id> --reset-count`

### 19.4 Exit Criteria
- [ ] Humans can override any automated decision
- [ ] Overrides are audited
- [ ] Overrides require reason

---

## Phase 20: Execution Commits & Branches

**Goal:** Git artifacts support observability and rollback.

### 20.1 Execution Branches
- [ ] One branch per task: `exec/<flow>/<task>`
- [ ] Created from TaskFlow base revision
- [ ] Never merged directly

### 20.2 Checkpoint Commits
- [ ] Commits created during/after execution
- [ ] Owned by task, ephemeral
- [ ] Used for diffs, rollback, retry

### 20.3 Branch Lifecycle
- [ ] Create on task start
- [ ] Reset on retry
- [ ] Archive/delete on completion

### 20.4 Exit Criteria
- [ ] Execution branches are task-isolated
- [ ] Checkpoints enable rollback
- [ ] Clean separation from integration commits

---

## Phase 21: Merge Protocol

**Goal:** Explicit, human-approved integration.

### 21.1 Merge Preparation
- [ ] Collect successful task branches
- [ ] Compute integration commits
- [ ] Test merge against target
- [ ] Report conflicts

### 21.2 Merge Commands
- [ ] `hivemind merge prepare <flow-id>`
- [ ] `hivemind merge approve <flow-id>`
- [ ] `hivemind merge execute <flow-id>`

### 21.3 Merge Events
- [ ] `MergePrepared`
- [ ] `MergeApproved`
- [ ] `MergeCompleted`

### 21.4 Exit Criteria
- [ ] Merge is explicit and human-approved
- [ ] Conflicts detected before execution
- [ ] Integration commits clean

---

## Phase 22: Single-Repo End-to-End

**Goal:** Complete workflow for single-repo projects.

> **User Story 3:** Structured TaskFlow end-to-end.

### 22.1 Integration Test
- [ ] Create project with repo
- [ ] Create tasks with scopes
- [ ] Build TaskGraph
- [ ] Create and start TaskFlow
- [ ] Execute tasks via Claude Code adapter
- [ ] Verify with checks
- [ ] Retry on failure
- [ ] Merge on success

### 22.2 Exit Criteria
- [ ] Full workflow works end-to-end
- [ ] All events emitted correctly
- [ ] State derived purely from events
- [ ] Human can pause, resume, abort
- [ ] User Stories 2, 3, 4 achievable

---

## Phase 23: Parallel Execution

**Goal:** Safe parallel agent execution.

> **User Story 5:** Parallel scoped agents.

### 23.1 Scope-Based Parallelism
- [ ] Scheduler allows parallel tasks if scopes compatible
- [ ] Hard conflicts → serialize or isolate
- [ ] Soft conflicts → warn or isolate

### 23.2 Parallel Worktrees
- [ ] Multiple worktrees active simultaneously
- [ ] Each task has isolated worktree

### 23.3 Exit Criteria
- [ ] Compatible tasks run in parallel
- [ ] Conflicts handled correctly
- [ ] No corruption from parallel execution
- [ ] User Story 5, 6 achievable

---

## Phase 24: Multi-Repo Support

**Goal:** TaskFlows spanning multiple repositories.

> **User Story 7:** Multi-repo project execution.

### 24.1 Multi-Repo Worktrees
- [ ] Worktree per repo per task
- [ ] Agent receives all repo paths

### 24.2 Multi-Repo Scope
- [ ] Scope evaluated per repo
- [ ] Violation in any repo fails task

### 24.3 Multi-Repo Merge
- [ ] Atomicity at TaskFlow level (default)
- [ ] All repos merge or none merge
- [ ] Partial failure handling

### 24.4 Exit Criteria
- [ ] Tasks can modify multiple repos
- [ ] Scope enforced per repo
- [ ] Merge is atomic across repos
- [ ] User Story 7, 8 achievable

---

## Phase 25: Additional Runtime Adapters

**Goal:** Support multiple runtimes.

### 25.1 Claude Code Adapter
- [ ] Implement ClaudeCodeAdapter
- [ ] Test integration

### 25.2 Codex CLI Adapter
- [ ] Implement CodexAdapter
- [ ] Test integration

### 25.3 Gemini CLI Adapter
- [ ] Implement GeminiAdapter
- [ ] Test integration

### 25.4 Runtime Selection
- [ ] Per-project default runtime
- [ ] Per-task override
- [ ] `hivemind runtime list`
- [ ] `hivemind runtime health`

### 25.5 Exit Criteria
- [ ] Multiple runtimes work
- [ ] Runtime selection works
- [ ] Same TaskFlow semantics across runtimes

---

## Phase 26: Event Streaming & Observability

**Goal:** Real-time observability.

### 26.1 Event Stream Command
- [ ] `hivemind events stream` (real-time)
- [ ] Filters: --flow, --task, --since

### 26.2 Event Query Command
- [ ] `hivemind events query` (historical)
- [ ] Correlation filtering
- [ ] Time range filtering

### 26.3 Exit Criteria
- [ ] Real-time event stream works
- [ ] Historical queries work
- [ ] Events are the complete record

---

## Phase 27: Automation & Scheduling

**Goal:** Triggered TaskFlows.

> **User Story 9:** Scheduled TaskFlows.

### 27.1 Automation Events
- [ ] `AutomationCreated`
- [ ] `AutomationTriggered`

### 27.2 Automation Commands
- [ ] `hivemind automation create <flow> --schedule <cron>`
- [ ] `hivemind automation list`
- [ ] `hivemind automation disable`
- [ ] `hivemind automation trigger` (manual)

### 27.3 Exit Criteria
- [ ] Scheduled triggers work
- [ ] Manual triggers work
- [ ] Automations are observable
- [ ] User Story 9 achievable

---

## Phase 28: Agent Meta-Operation

**Goal:** Agents can operate Hivemind itself.

### 28.1 CLI as API
- [ ] All commands scriptable
- [ ] JSON output reliable
- [ ] Exit codes semantic

### 28.2 Attribution
- [ ] Agent actions attributed in events
- [ ] Scope applies to meta-operations

### 28.3 Exit Criteria
- [ ] Agents can invoke CLI commands
- [ ] Actions are audited
- [ ] Meta-orchestration possible

---

## Phase 29: UI Foundation (Optional)

**Goal:** UI is a projection over CLI-accessible state.

### 29.1 Views
- [ ] Project overview
- [ ] Task list / Kanban
- [ ] TaskFlow document view
- [ ] Dependency graph
- [ ] Diff and verification views

### 29.2 Principles
- [ ] UI reads via CLI/events only
- [ ] UI does not modify state directly
- [ ] UI is secondary to CLI

### 29.3 Exit Criteria
- [ ] UI reflects CLI state accurately
- [ ] No UI-only features

---

## Phase 30: Production Hardening

**Goal:** Production-ready quality.

### 30.1 Error Handling
- [ ] All error paths tested
- [ ] Recovery hints useful
- [ ] No silent failures

### 30.2 Performance
- [ ] Event replay scales to 10k+ events
- [ ] CLI response time < 100ms for reads
- [ ] Parallel execution efficient

### 30.3 Documentation
- [ ] CLI help complete
- [ ] Architecture docs match implementation
- [ ] User guide written

### 30.4 Exit Criteria
- [ ] All user stories achievable
- [ ] All principles upheld
- [ ] Ready for real use

---

## Summary: User Story Coverage

| User Story | Phase |
|------------|-------|
| US1: Simple Todo Tracking | Phase 6 |
| US2: Manual Agent Assistance | Phase 22 |
| US3: Structured TaskFlow | Phase 22 |
| US4: Verification Failure & Retry | Phase 18 |
| US5: Parallel Scoped Agents | Phase 23 |
| US6: Scope Conflict Handling | Phase 23 |
| US7: Multi-Repo Project Execution | Phase 24 |
| US8: Shared Repo Across Projects | Phase 24 |
| US9: Automation | Phase 27 |
| US10: Pause & Resume | Phase 9 |

---

## Principle Checkpoints

After each phase, verify:

1. **Observability:** All state derived from events?
2. **Fail Fast:** Errors surface immediately?
3. **Reliability:** Deterministic, no cleverness?
4. **Errors:** Structured, attributable, actionable?
5. **Structure:** Explicit boundaries and FSMs?
6. **SOLID:** Single responsibility, clear interfaces?
7. **CLI-First:** Feature works via CLI?
8. **Absolute Observability:** No silent side effects?
9. **Automated Checks:** Verification gates progress?
10. **Failures First-Class:** Failures preserved and inspectable?
11. **Incremental:** Foundations proven before expansion?
12. **Modularity:** Replaceable components?
13. **Control:** Power users can drop down a level?
14. **Human Authority:** Humans decide at boundaries?
15. **No Magic:** Everything has a reason and trail?

---

**This roadmap is the contract. Follow it.**
