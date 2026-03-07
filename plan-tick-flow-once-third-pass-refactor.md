## 1. Overview

Refactor `src/core/registry/flow/execution/tick/once.rs` so `Registry::tick_flow_once` remains the top-level orchestrator, but large cohesive phases move into child modules under `src/core/registry/flow/execution/tick/once/`.

### Goals / success criteria
- Keep behavior, event ordering, and error semantics unchanged.
- Reduce `tick_flow_once` to a readable phase-oriented orchestrator.
- Extract helpers that return owned data or small private structs to avoid borrow/lifetime churn.
- Do **not** broaden scope into `tick/driver.rs`, shared runtime selection logic, or event model changes.

### Scope boundaries
Included:
- Private helper extraction only.
- New child modules under `tick/once/`.
- Small private structs for multi-value returns where needed.

Excluded:
- Any semantic cleanup, deduplication with `tick/driver.rs`, or event payload changes.
- Reordering of persistence or runtime calls.

## 2. Prerequisites
- No new dependencies.
- No migrations or data changes.
- Keep module layout Rust-idiomatic for a file module with submodules:
  - keep `tick/once.rs`
  - add `tick/once/*.rs`
  - declare child modules from `once.rs`

## 3. Implementation Steps

### Step 1: Introduce child modules and phase structs
Files:
- Modify `src/core/registry/flow/execution/tick/once.rs`
- Create `src/core/registry/flow/execution/tick/once/readiness.rs`
- Create `src/core/registry/flow/execution/tick/once/worktree.rs`
- Create `src/core/registry/flow/execution/tick/once/attempt.rs`
- Create `src/core/registry/flow/execution/tick/once/runtime.rs`

Key details:
- Add `mod readiness; mod worktree; mod attempt; mod runtime;` to `once.rs`.
- Add small private structs in the child modules instead of returning tuples everywhere.
- Prefer owned fields like `Uuid`, `String`, `GraphTask`, `ProjectRuntimeConfig`, `Vec<(String, WorktreeStatus)>`.

Testing:
- No behavior change expected; later run targeted integration tests around `flow tick`.

### Step 2: Extract pending/readiness scheduling phase
Move together from `once.rs`:
- Lines 30-123: pending scan + `TaskBlocked` / `TaskReady` emission
- Lines 125-141: runnable task selection
- Optionally lines 143-164: no-task completion branch

Recommended helpers in `once/readiness.rs`:
- `fn process_pending_task_transitions(&self, flow: &TaskFlow, graph: &TaskGraph, origin: &'static str) -> Result<()>`
- `fn select_task_to_run(state: &AppState, flow: &TaskFlow, preferred_task: Option<Uuid>) -> Option<Uuid>`
- `fn maybe_complete_finished_flow(&self, flow: &TaskFlow, flow_id: &str, origin: &'static str) -> Result<Option<TaskFlow>>`

Why this boundary works:
- Uses only `&TaskFlow`, `&TaskGraph`, `&AppState`; no returned borrows.
- Preserves the current pattern of: emit transitions -> reload flow -> choose task.
- `maybe_complete_finished_flow` is optional but still conservative because it only owns event append + dependent autostart + reload.

### Step 3: Extract launch-prerequisite and worktree phase
Move together from `once.rs`:
- Lines 167-202: runtime selection, worktree ensure/inspect, exec lookup, clean-retry reset
- Lines 204-273: dependency-branch merge propagation

Recommended helpers in `once/worktree.rs`:
- `fn prepare_launch_prerequisites(&self, state: &AppState, flow: &TaskFlow, task_id: Uuid, origin: &'static str) -> Result<LaunchPrereqs>`
- `fn merge_dependency_branch_heads(&self, flow: &TaskFlow, graph: &TaskGraph, task_id: Uuid, repo_worktrees: &[(String, WorktreeStatus)], origin: &'static str) -> Result<()>`

Recommended `LaunchPrereqs` fields:
- `runtime: ProjectRuntimeConfig`
- `runtime_selection_source: RuntimeSelectionSource`
- `worktree_status: WorktreeStatus`
- `repo_worktrees: Vec<(String, WorktreeStatus)>`
- `next_attempt_number: u32`

Borrow/lifetime note:
- Do **not** return `&TaskExecution`; compute `next_attempt_number` inside the helper and return the number.
- Let the helper perform clean-retry reset internally so `tick_flow_once` does not need to hold `exec` borrows.

### Step 4: Extract attempt/context assembly phase
Move together from `once.rs`:
- Lines 275-520: start attempt, build retry context, assemble attempt context, append all context-window/context-delivery events
- Lines 522-532: build `ExecutionInput` and formatted prompt

Recommended helper in `once/attempt.rs`:
- `fn start_attempt_and_build_input(&self, state: &AppState, flow: &TaskFlow, graph: &TaskGraph, task_id: Uuid, runtime: &ProjectRuntimeConfig, repo_worktrees: &[(String, WorktreeStatus)], next_attempt_number: u32, origin: &'static str) -> Result<AttemptLaunch>`

Recommended `AttemptLaunch` fields:
- `attempt_id: Uuid`
- `attempt_corr: CorrelationIds`
- `task_scope: Option<Scope>`
- `max_attempts: u32`
- `input: ExecutionInput`
- `runtime_prompt: String`

Why this boundary works:
- This phase is already cohesive around “attempt creation + input materialization”.
- Returning `task_scope` avoids keeping a borrow to `graph.tasks[task_id]` alive into runtime env preparation.
- All event ordering inside lines 316-520 should stay identical within one helper.

### Step 5: Extract runtime prep and execution phase
Move together from `once.rs`:
- Lines 535-642: mutate runtime env for the attempt
- Lines 728-995: initialize/prepare/execute adapter, stream output events, append native/fs/exited events
- Lines 996-1072: post-run failure handling and success transition

Recommended helpers in `once/runtime.rs`:
- `fn populate_runtime_env_for_attempt(&self, state: &AppState, flow: &TaskFlow, task_id: Uuid, attempt_id: Uuid, task_scope: Option<&Scope>, runtime: &mut ProjectRuntimeConfig, worktree_status: &WorktreeStatus, repo_worktrees: &[(String, WorktreeStatus)], origin: &'static str) -> Result<()>`
- `fn initialize_prepare_and_execute_adapter(&self, adapter: &mut dyn RuntimeAdapter, input: ExecutionInput, interactive: bool, task_id: Uuid, attempt_id: Uuid, attempt_corr: &CorrelationIds, worktree_status: &WorktreeStatus, origin: &'static str) -> std::result::Result<RuntimeRunOutcome, RuntimeError>`
- `fn finalize_attempt_and_transition(&self, state: &AppState, flow: &TaskFlow, flow_id: &str, task_id: Uuid, attempt_id: Uuid, runtime_for_adapter: &ProjectRuntimeConfig, next_attempt_number: u32, max_attempts: u32, report: &ExecutionReport, origin: &'static str) -> Result<TaskFlow>`

Recommended `RuntimeRunOutcome` fields:
- `report: ExecutionReport`
- `terminated_reason: Option<String>`

Conservative detail:
- Keep `prepare_runtime_environment(...)` error handling in `tick_flow_once` unless extraction can preserve the current `handle_runtime_failure(...); return self.get_flow(flow_id);` behavior exactly.
- Likewise keep `build_runtime_adapter(...)` at the orchestrator level if that keeps failure mapping simpler.

## 4. File Changes Summary

### Create
- `src/core/registry/flow/execution/tick/once/readiness.rs`
- `src/core/registry/flow/execution/tick/once/worktree.rs`
- `src/core/registry/flow/execution/tick/once/attempt.rs`
- `src/core/registry/flow/execution/tick/once/runtime.rs`

### Modify
- `src/core/registry/flow/execution/tick/once.rs`

### Delete
- None

## 5. Testing Strategy
- Re-run existing `flow tick` integration coverage in `tests/integration.rs`, especially paths asserting `runtime_started`, `runtime_output_chunk`, `runtime_exited` / `runtime_terminated`, checkpoint completion, and scope/runtime failure handling.
- Add focused unit tests only if helper extraction introduces pure helper logic that is easy to test in isolation (for example dependency-blocked reason formatting or preferred-task selection).
- Manual sanity check: compare emitted event order before/after for one successful tick and one retry/failure tick.

## 6. Rollback Plan
- Revert the new child modules and restore the original body of `tick_flow_once`.
- No data rollback required because this is source-only refactoring.

## 7. Estimated Effort
- Effort: 2-4 hours
- Complexity: Medium
- Main risk: accidentally changing event ordering or when fresh flow/state snapshots are reloaded.

## Refactor constraints to preserve exactly
- Keep verifying-task short-circuit at lines 24-28 before any readiness promotion.
- Keep blocked events emitted before ready events.
- Keep `self.get_flow(flow_id)?` reload after readiness/event emission.
- Keep clean-retry worktree reset before dependency-branch merge.
- Keep `start_task_execution` before retry/context events.
- Keep `RuntimeEnvironmentPrepared` before `RuntimeStarted`.
- Keep `RuntimeTerminated` (if any) before `RuntimeExited`.
- Keep nonzero-exit handling before `complete_task_execution`.
- End success path with `self.process_verifying_task(flow_id, task_id)` exactly as today.

