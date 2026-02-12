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
- [x] Configure cargo-nextest for test runner

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
- [x] `TaskGraphCreated`
- [x] `TaskAddedToGraph`
- [x] `DependencyAdded`
- [x] `ScopeAssigned`
- [x] `TaskGraphValidated`
- [x] `TaskGraphLocked` (becomes immutable)

### 8.2 TaskGraph State
- [x] Define TaskGraph struct (ID, project_id, tasks, edges, scopes)
- [x] Derive from events
- [x] DAG validation (no cycles)
- [x] Mutable until locked

### 8.3 TaskGraph Commands
- [x] `hivemind graph create <project> <name>`
- [x] `hivemind graph add-task <graph> <task>` (via --from-tasks)
- [x] `hivemind graph add-dependency <graph> <from> <to>`
- [x] `hivemind graph set-scope <graph> <task> <scope>`
- [x] `hivemind graph validate <graph>`
- [x] `hivemind graph inspect <graph>`

### 8.4 Exit Criteria
- [x] Can build TaskGraphs from tasks
- [x] Dependencies form valid DAG
- [x] Cycle detection works
- [x] Graph is immutable after lock

---

## Phase 9: TaskFlow Foundation (No Execution)

**Goal:** TaskFlow is the runtime instance of a TaskGraph.

### 9.1 TaskFlow Events
- [x] `TaskFlowCreated`
- [x] `TaskFlowStarted`
- [x] `TaskFlowPaused`
- [x] `TaskFlowResumed`
- [x] `TaskFlowCompleted`
- [x] `TaskFlowAborted`

### 9.2 TaskFlow State
- [x] Define TaskFlow struct (ID, graph_id, state, created_at)
- [x] Flow states: CREATED, RUNNING, PAUSED, COMPLETED, ABORTED
- [x] Derive from events

### 9.3 TaskFlow Commands
- [x] `hivemind flow create <graph-id>`
- [x] `hivemind flow start <flow-id>` (state change only, no execution yet)
- [x] `hivemind flow pause <flow-id>`
- [x] `hivemind flow resume <flow-id>`
- [x] `hivemind flow abort <flow-id>`
- [x] `hivemind flow status <flow-id>`

### 9.4 Exit Criteria
- [x] TaskFlow lifecycle works (create → start → pause → resume → complete)
- [x] State transitions emit events
- [x] State derived from events
- [x] Flow can be paused and resumed

---

## Phase 10: Task Execution State Machine (No Runtime)

**Goal:** Task execution FSM without actual agent execution.

### 10.1 TaskExecution Events
- [x] `TaskReady`
- [x] `TaskBlocked`
- [x] `TaskExecutionStarted`
- [x] `TaskExecutionStateChanged`
- [x] `TaskExecutionSucceeded`
- [x] `TaskExecutionFailed`

### 10.2 TaskExecution States
- [x] PENDING → RUNNING → VERIFYING → SUCCESS
- [x] PENDING → RUNNING → VERIFYING → RETRY
- [x] PENDING → RUNNING → VERIFYING → FAILED
- [x] RETRY → RUNNING (bounded)
- [x] FAILED → ESCALATED (optional)

### 10.3 Scheduler (Dependency Resolution)
- [x] Task becomes READY when all upstream tasks are SUCCESS
- [x] Task is BLOCKED while dependencies are pending
- [x] Emit `TaskReady` / `TaskBlocked` events

### 10.4 Manual State Transitions (Testing)
- [x] `hivemind task retry <task-id>` (FAILED → PENDING/RETRY)
- [x] `hivemind task abort <task-id>` (any → FAILED)
- [x] Used for testing FSM without runtime

### 10.5 Exit Criteria
- [x] FSM transitions are correct
- [x] Scheduler releases tasks in dependency order
- [x] All transitions emit events
- [x] Manual simulation works for testing

---

## Phase 11: Worktree Management

**Goal:** Isolated git worktrees for task execution.

### 11.1 Worktree Operations
- [x] Create worktree from base revision
- [x] Delete worktree
- [x] List worktrees for flow

### 11.2 Worktree Lifecycle
- [x] Create on task start
- [x] Preserve on failure (for debugging)
- [x] Delete on success (configurable)

### 11.3 Worktree Commands
- [x] `hivemind worktree list <flow-id>`
- [x] `hivemind worktree inspect <task-id>`
- [x] `hivemind worktree cleanup <flow-id>` (manual cleanup)

### 11.4 Exit Criteria
- [x] Worktrees created in `.hivemind/worktrees/`
- [x] Worktrees are valid git worktrees
- [x] Worktrees isolated per task

---

## Phase 12: Baseline & Diff Computation

**Goal:** Observe what changed during execution.

### 12.1 Baseline Snapshot
- [x] Capture file list + hashes before execution
- [x] Capture git HEAD

### 12.2 Change Detection
- [x] Compare post-execution state to baseline
- [x] Identify: created, modified, deleted files
- [x] Compute unified diffs

### 12.3 Diff Events
- [x] `FileModified`
- [x] `DiffComputed`
- [x] `CheckpointCommitCreated`

### 12.4 Exit Criteria
- [x] Diffs computed accurately
- [x] Diffs attributed to task/attempt
- [x] Changes observable via events

---

## Phase 13: Runtime Adapter Interface

**Goal:** Define the contract for runtime adapters.

### 13.1 Adapter Interface
- [x] Define RuntimeAdapter interface:
  - `Initialize() -> error`
  - `Prepare(task, worktree) -> error`
  - `Execute(input) -> ExecutionReport`
  - `Terminate() -> error`

### 13.2 Execution Report
- [x] Define ExecutionReport struct:
  - exit_code, duration, stdout, stderr
  - files_created, files_modified, files_deleted
  - errors

### 13.3 Adapter Events
- [x] `RuntimeStarted`
- [x] `RuntimeOutputChunk`
- [x] `RuntimeExited`
- [x] `RuntimeTerminated`

### 13.4 Exit Criteria
- [x] Interface defined and documented
- [x] ExecutionReport captures all needed data
- [x] Events defined for all lifecycle phases

---

## Phase 14: OpenCode Adapter (First Runtime)

**Goal:** Wrap OpenCode CLI as first runtime.

### 14.1 Adapter Implementation
- [x] Implement OpenCodeAdapter
- [x] Binary detection and health check
- [x] Subprocess launch with controlled environment
- [x] Input delivery (stdin)
- [x] Output capture (stdout/stderr)
- [x] Timeout enforcement

### 14.2 Configuration
- [x] Adapter config in project/global settings
- [x] Binary path, args, timeout, env passthrough

### 14.3 Integration
- [x] Adapter selected per project
- [x] Adapter invoked by TaskFlow engine

### 14.4 Exit Criteria
- [x] OpenCode can be launched
- [x] Output captured
- [x] Timeout works
- [x] Filesystem changes observed

---

## Phase 15: Interactive Runtime Sessions (CLI)

**Goal:** Support interactive execution of external runtimes in the CLI without changing TaskFlow semantics.

> **Invariant:** Interactive mode is a transport. It must not introduce UI-only behavior or bypass scope, verification, retries, or merge governance.

### 15.1 Interactive Execution Mode
- [x] Launch runtime in an interactive session when requested
- [x] Stream output continuously while emitting `RuntimeOutputChunk` events
- [x] Forward user input lines to the runtime
- [x] Ctrl+C interrupts the runtime deterministically (not a crash)

### 15.2 Interactive Events
- [x] `RuntimeInputProvided`
- [x] `RuntimeInterrupted`

### 15.3 CLI Integration
- [x] `hivemind flow tick <flow-id> --interactive`
- [x] Interactive mode is optional; default remains non-interactive

### 15.4 Exit Criteria
- [x] Interactive runtime sessions work end-to-end in a real terminal
- [x] All interaction is observable via events
- [x] Interruptions are recorded and runtime terminates cleanly
- [x] Invariant holds: interactive mode does not change TaskFlow semantics

---

## Phase 16: Scope Enforcement (Phase 1: Detection)

**Goal:** Detect scope violations post-execution.

> **Honest:** Phase 1 is detection, not prevention.

### 16.1 Scope Verification
- [x] After execution, check all changes against scope
- [x] Filesystem scope: all modified files within allowed paths
- [x] Git scope: commits/branches only if permitted

### 16.2 Violation Handling
- [x] `ScopeViolationDetected` event
- [x] Attempt fails immediately on violation
- [x] Worktree preserved for debugging
- [x] Clear error message with violation details

### 16.3 Exit Criteria
- [x] Violations detected reliably
- [x] Violations are fatal to attempt
- [x] Violations emit observable events
- [x] Honest: prevention is Phase 2+

---

## Phase 17: Verification Framework

**Goal:** Automated checks are the primary gate.

### 17.1 Check Execution
- [x] Run configured checks (test commands)
- [x] Capture exit code, output, duration
- [x] `CheckStarted`, `CheckCompleted` events

### 17.2 Check Results
- [x] Define CheckResult struct
- [x] Required vs optional checks
- [x] Aggregation: all required must pass

### 17.3 Verification Commands
- [x] `hivemind verify run <task-id>` (manual verification)
- [x] `hivemind verify results <attempt-id>`

### 17.4 Exit Criteria
- [x] Checks run in worktree
- [x] Results captured and observable
- [x] Required check failures block success

---

## Phase 18: Verifier Agent (Advisory)

**Goal:** LLM-based verification as advisory layer.

> **Important:** Verifier is advisory, not authoritative.

**Completed:** 2025-02-11

### 18.1 Verifier Invocation
- [x] After checks, invoke verifier agent (optional toggle)
- [x] Input: task definition, diff, check results
- [x] Output: PASS / SOFT_FAIL / HARD_FAIL + feedback

### 18.2 Verifier Events
- [x] `VerificationStarted`
- [x] `VerificationCompleted`

### 18.3 Integration
- [x] Verifier decision informs retry/fail
- [x] Verifier cannot override failed checks
- [x] Verifier feedback included in retry context

### 18.4 Exit Criteria
- [x] Verifier produces structured decision
- [x] Verifier is advisory only
- [x] Feedback captured for retry

---

## Phase 19: Retry Mechanics

**Goal:** Enable intelligent retries with explicit context.

**Completed:** 2025-02-11

### 19.1 Retry Context Assembly
- [x] Gather prior attempt outcomes
- [x] Gather check results
- [x] Gather verifier feedback
- [x] Compute actionable feedback

### 19.2 Retry Context Delivery
- [x] Context is explicit input (not hidden memory)
- [x] Context visible in attempt events
- [x] Mechanical feedback prioritized over advisory

### 19.3 Retry Policy
- [x] Max attempts per task
- [x] Retry on soft fail, not on hard fail
- [x] Bounded retries (no infinite loops)

### 19.4 Exit Criteria
- [x] Retries receive explicit context
- [x] Context is observable
- [x] Retries are bounded

---

## Phase 20: Human Override

**Goal:** Humans are ultimate authority.

**Completed:** 2026-02-12

### 20.1 Override Actions
- [x] Override check failures
- [x] Override verifier decision
- [x] Direct approval/rejection
- [x] Manual retry with reset count

### 20.2 Override Events
- [x] `HumanOverride` with attribution and reason

### 20.3 Override Commands
- [x] `hivemind verify override <task-id> pass|fail --reason <text>`
- [x] `hivemind task retry <task-id> --reset-count`

### 20.4 Exit Criteria
- [x] Humans can override any automated decision
- [x] Overrides are audited
- [x] Overrides require reason

Note: admin token/login?

---

## Phase 21: Execution Commits & Branches

**Goal:** Git artifacts support observability and rollback.

### 21.1 Execution Branches
- [ ] One branch per task: `exec/<flow>/<task>`
- [ ] Created from TaskFlow base revision
- [ ] Never merged directly

### 21.2 Checkpoint Commits
- [ ] Commits created during/after execution
- [ ] Owned by task, ephemeral
- [ ] Used for diffs, rollback, retry

### 21.3 Branch Lifecycle
- [ ] Create on task start
- [ ] Reset on retry
- [ ] Archive/delete on completion

### 21.4 Exit Criteria
- [ ] Execution branches are task-isolated
- [ ] Checkpoints enable rollback
- [ ] Clean separation from integration commits

---

## Phase 22: Merge Protocol

**Goal:** Explicit, human-approved integration.

### 22.1 Merge Preparation
- [ ] Collect successful task branches
- [ ] Compute integration commits
- [ ] Test merge against target
- [ ] Report conflicts

### 22.2 Merge Commands
- [ ] `hivemind merge prepare <flow-id>`
- [ ] `hivemind merge approve <flow-id>`
- [ ] `hivemind merge execute <flow-id>`

### 22.3 Merge Events
- [ ] `MergePrepared`
- [ ] `MergeApproved`
- [ ] `MergeCompleted`

### 22.4 Exit Criteria
- [ ] Merge is explicit and human-approved
- [ ] Conflicts detected before execution
- [ ] Integration commits clean

---

## Phase 23: Single-Repo End-to-End

**Goal:** Complete workflow for single-repo projects.

> **User Story 3:** Structured TaskFlow end-to-end.

### 23.1 Integration Test
- [ ] Create project with repo
- [ ] Create tasks with scopes
- [ ] Build TaskGraph
- [ ] Create and start TaskFlow
- [ ] Execute tasks via Opencode adapter
- [ ] Verify with checks
- [ ] Retry on failure
- [ ] Merge on success

### 23.2 Exit Criteria
- [ ] Full workflow works end-to-end
- [ ] All events emitted correctly
- [ ] State derived purely from events
- [ ] Human can pause, resume, abort
- [ ] User Stories 2, 3, 4 achievable

---

## Phase 24: Parallel Execution

**Goal:** Safe parallel agent execution.

> **User Story 5:** Parallel scoped agents.

### 24.1 Scope-Based Parallelism
- [ ] Scheduler allows parallel tasks if scopes compatible
- [ ] Hard conflicts → serialize or isolate
- [ ] Soft conflicts → warn or isolate

### 24.2 Parallel Worktrees
- [ ] Multiple worktrees active simultaneously
- [ ] Each task has isolated worktree

### 24.3 Exit Criteria
- [ ] Compatible tasks run in parallel
- [ ] Conflicts handled correctly
- [ ] No corruption from parallel execution
- [ ] User Story 5, 6 achievable

---

## Phase 25: Multi-Repo Support

**Goal:** TaskFlows spanning multiple repositories.

> **User Story 7:** Multi-repo project execution.

### 25.1 Multi-Repo Worktrees
- [ ] Worktree per repo per task
- [ ] Agent receives all repo paths

### 25.2 Multi-Repo Scope
- [ ] Scope evaluated per repo
- [ ] Violation in any repo fails task

### 25.3 Multi-Repo Merge
- [ ] Atomicity at TaskFlow level (default)
- [ ] All repos merge or none merge
- [ ] Partial failure handling

### 25.4 Exit Criteria
- [ ] Tasks can modify multiple repos
- [ ] Scope enforced per repo
- [ ] Merge is atomic across repos
- [ ] User Story 7, 8 achievable

---

## Phase 26: Additional Runtime Adapters

**Goal:** Support multiple runtimes.

### 26.1 Claude Code Adapter
- [ ] Implement ClaudeCodeAdapter
- [ ] Test integration

### 26.2 Codex CLI Adapter
- [ ] Implement CodexAdapter
- [ ] Test integration

### 26.3 Gemini CLI Adapter
- [ ] Implement GeminiAdapter
- [ ] Test integration

### 26.4 Runtime Selection
- [ ] Per-project default runtime
- [ ] Per-task override
- [ ] `hivemind runtime list`
- [ ] `hivemind runtime health`

### 26.5 Exit Criteria
- [ ] Multiple runtimes work
- [ ] Runtime selection works
- [ ] Same TaskFlow semantics across runtimes

---

## Phase 27: Event Streaming & Observability

**Goal:** Real-time observability.

### 27.1 Event Stream Command
- [ ] `hivemind events stream` (real-time)
- [ ] Filters: --flow, --task, --since

### 27.2 Event Query Command
- [ ] `hivemind events list` (historical)
- [ ] `hivemind events inspect <event-id>`
- [ ] Correlation filtering
- [ ] Time range filtering

### 27.3 Exit Criteria
- [ ] Real-time event stream works
- [ ] Historical queries work
- [ ] Events are the complete record

---

## Phase 28: Automation & Scheduling

**Goal:** Triggered TaskFlows.

> **User Story 9:** Scheduled TaskFlows.

### 28.1 Automation Events
- [ ] `AutomationCreated`
- [ ] `AutomationTriggered`

### 28.2 Automation Commands
- [ ] `hivemind automation create <flow> --schedule <cron>`
- [ ] `hivemind automation list`
- [ ] `hivemind automation disable`
- [ ] `hivemind automation trigger` (manual)

### 28.3 Exit Criteria
- [ ] Scheduled triggers work
- [ ] Manual triggers work
- [ ] Automations are observable
- [ ] User Story 9 achievable

---

## Phase 29: Agent Meta-Operation

**Goal:** Agents can operate Hivemind itself.

### 29.1 CLI as API
- [ ] All commands scriptable
- [ ] JSON output reliable
- [ ] Exit codes semantic

### 29.2 Attribution
- [ ] Agent actions attributed in events
- [ ] Scope applies to meta-operations

### 29.3 Exit Criteria
- [ ] Agents can invoke CLI commands
- [ ] Actions are audited
- [ ] Meta-orchestration possible

---

## Phase 30: UI Foundation (Optional)

**Goal:** UI is a projection over CLI-accessible state.

### 30.1 Views
- [ ] Project overview
- [ ] Task list / Kanban
- [ ] TaskFlow document view
- [ ] Dependency graph
- [ ] Diff and verification views

### 30.2 Principles
- [ ] UI reads via CLI/events only
- [ ] UI does not modify state directly
- [ ] UI is secondary to CLI

### 30.3 Exit Criteria
- [ ] UI reflects CLI state accurately
- [ ] No UI-only features

---

## Phase 31: Production Hardening

**Goal:** Production-ready quality.

### 31.1 Error Handling
- [ ] All error paths tested
- [ ] Recovery hints useful
- [ ] No silent failures

### 31.2 Performance
- [ ] Event replay scales to 10k+ events
- [ ] CLI response time < 100ms for reads
- [ ] Parallel execution efficient

### 31.3 Documentation
- [ ] CLI help complete
- [ ] Architecture docs match implementation
- [ ] User guide written

### 31.4 Exit Criteria
- [ ] All user stories achievable
- [ ] All principles upheld
- [ ] Ready for real use

---

## Summary: User Story Coverage

| User Story | Phase |
|------------|-------|
| US1: Simple Todo Tracking | Phase 6 |
| US2: Manual Agent Assistance | Phase 23 |
| US3: Structured TaskFlow | Phase 23 |
| US4: Verification Failure & Retry | Phase 19 |
| US5: Parallel Scoped Agents | Phase 24 |
| US6: Scope Conflict Handling | Phase 24 |
| US7: Multi-Repo Project Execution | Phase 25 |
| US8: Shared Repo Across Projects | Phase 25 |
| US9: Automation | Phase 28 |
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
