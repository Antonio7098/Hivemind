## 1. Overview

Refactor `src/core/registry/flow/merge/prepare/run.rs` so `Registry::merge_prepare` remains the top-level orchestrator while cohesive inner phases move into child modules under `src/core/registry/flow/merge/prepare/run/`.

Goals:
- Reduce `merge_prepare` size without changing behavior.
- Keep event ordering, git command behavior, and conflict accumulation identical.
- Minimize borrow/lifetime friction by returning owned results and passing immutable refs or owned collections.

Out of scope:
- Cross-module deduplication with `merge/execute/local.rs`.
- Changing git command semantics, event payloads, error codes, or merge policy.
- Reworking existing shared helpers outside `merge/prepare/run/`.

## 2. Prerequisites

- No new dependencies.
- No migrations or data changes.
- Confirm the new nested module layout compiles with:
  - `src/core/registry/flow/merge/prepare.rs` -> `mod run;`
  - `src/core/registry/flow/merge/prepare/run.rs` -> `mod common; mod primary; mod checks; mod secondary;`
- Reuse existing helpers already defined elsewhere:
  - `Self::worktree_managers_for_flow`
  - `Self::git_ref_exists`
  - `Self::resolve_git_ref`
  - `Self::run_check_command`
  - `self.emit_merge_conflict`
  - `self.emit_integration_lock_acquired`

## 3. Implementation Steps

### Step 1: Keep orchestration in `run.rs`
- Keep inline in `merge_prepare`:
  - lines 10-53: load flow, validate state, constitution gate, idempotent prepared fast-path, lock, freeze/reload.
  - lines 55-88: load `graph`, allocate managers, validate project repo presence, split primary vs secondary manager.
  - lines 761-785: append `MergePrepared` event and reload final `MergeState`.
- Add module declarations in `run.rs` for `common`, `primary`, `checks`, and `secondary`.
- Add one local precomputed value after graph lookup:
  - `let successful_task_ids = Self::successful_task_ids_in_topological_order(&flow, graph);`
- Testing: no behavior change yet; compile after module wiring.

### Step 2: Extract small shared helpers into `run/common.rs`
- Move/derive shared, non-eventful logic here.
- Proposed helpers:
  1. `fn successful_task_ids_in_topological_order(flow: &TaskFlow, graph: &TaskGraph) -> Vec<Uuid>`
     - Pulls repeated filtering now duplicated in lines 186-193, 412-419, and 613-620.
  2. `fn resolve_prepare_target_branch(manager: &WorktreeManager, requested: Option<&str>, origin: &'static str) -> Result<String>`
     - Move lines 90-128.
     - Preserve `main` preference, current branch fallback, and detached-HEAD error/hint.
  3. `fn recreate_prepare_worktree(manager: &WorktreeManager, flow_id: Uuid, merge_branch: &str, base_ref: &str, origin: &'static str) -> Result<PathBuf>`
     - Move the common worktree remove/create/add logic from lines 130-184 and 563-603.
     - Return owned `PathBuf` to avoid borrowed path lifetimes.
- Testing: compile and ensure signatures are usable from both `primary.rs` and `secondary.rs`.

### Step 3: Extract primary-repo integration into `run/primary.rs`
- Create an owned outcome struct:
  - `struct PrimaryPrepareOutcome { prepared_target_branch: String, conflicts: Vec<String>, integrated_tasks: Vec<(Uuid, Option<String>)> }`
- Main helper:
  - `fn prepare_primary_repo(&self, flow: &TaskFlow, graph: &TaskGraph, successful_task_ids: &[Uuid], manager: &WorktreeManager, requested_target_branch: Option<&str>, origin: &'static str) -> Result<PrimaryPrepareOutcome>`
- Move together:
  - lines 90-556 overall, but delegate lines 401-515 to `checks.rs`.
- Internal helpers in `primary.rs`:
  1. `fn integrate_primary_repo_tasks(&self, flow: &TaskFlow, graph: &TaskGraph, successful_task_ids: &[Uuid], merge_path: &Path, merge_branch: &str, target_branch: &str, origin: &'static str) -> Result<(Vec<String>, Vec<(Uuid, Option<String>)>)>`
     - Move lines 186-399.
     - Keep dependency validation, sandbox branch flow, merge/abort/checkout behavior, commit message shape, and `TaskIntegratedIntoFlow` SHA capture logic together.
  2. `fn publish_primary_prepare_branch(&self, flow: &TaskFlow, manager: &WorktreeManager, merge_branch: &str, integrated_tasks: &[(Uuid, Option<String>)], origin: &'static str) -> Result<()>`
     - Move lines 517-556.
     - Keep branch update and `TaskIntegratedIntoFlow` event emission in one place so publication remains atomic from the orchestrator’s perspective.
- Testing: target compile and focused merge tests.

### Step 4: Extract aggregated check execution into `run/checks.rs`
- Proposed helpers:
  1. `fn collect_unique_merge_checks(graph: &TaskGraph, successful_task_ids: &[Uuid]) -> Vec<crate::core::verification::CheckConfig>`
     - Move lines 411-435.
  2. `fn run_aggregated_merge_checks(&self, flow: &TaskFlow, graph: &TaskGraph, successful_task_ids: &[Uuid], merge_path: &Path, origin: &'static str) -> Result<Vec<String>>`
     - Move lines 401-515.
     - Return owned `Vec<String>` of conflicts instead of mutating caller-owned state.
- Keep event emission inside this helper because it owns the check loop and conflict generation.
- Preserve details exactly:
  - check start/completion events
  - log file write under `cargo-target/<flow>/_integration_prepare/checks`
  - required-check failure format
  - optional first-10-lines snippet behavior
- Testing: existing merge-prepare happy-path plus a required-check failure scenario.

### Step 5: Extract secondary-repo fanout into `run/secondary.rs`
- Main helper:
  - `fn prepare_secondary_repositories(&self, flow: &TaskFlow, successful_task_ids: &[Uuid], managers: Vec<(String, WorktreeManager)>, prepared_target_branch: &str, origin: &'static str) -> Result<Vec<String>>`
- Optional inner helper for clarity:
  - `fn prepare_secondary_repository(&self, flow: &TaskFlow, repo_name: &str, manager: &WorktreeManager, successful_task_ids: &[Uuid], prepared_target_branch: &str, origin: &'static str) -> Result<Vec<String>>`
- Move together:
  - lines 561-759.
- Keep this conservative:
  - do not add dependency-drift validation here unless already present.
  - preserve repo-level continue/break behavior exactly.
  - preserve weaker error handling on final `branch -f` (currently ignored).
- Testing: compile, plus multi-repo merge prepare path if existing coverage is available.

### Step 6: Reassemble `merge_prepare` as a small phase driver
- Expected high-level shape in `run.rs`:
  1. preflight + freeze
  2. resolve graph/managers
  3. compute `successful_task_ids`
  4. `let primary = self.prepare_primary_repo(...)?;`
  5. `let mut conflicts = primary.conflicts;`
  6. if no conflicts, `conflicts.extend(self.prepare_secondary_repositories(...)?);`
  7. finalize `MergePrepared` event and reload state
- Testing: run the narrowest relevant test set first, then broader merge coverage only if needed.

## 4. File Changes Summary

Create:
- `src/core/registry/flow/merge/prepare/run/common.rs`
- `src/core/registry/flow/merge/prepare/run/primary.rs`
- `src/core/registry/flow/merge/prepare/run/checks.rs`
- `src/core/registry/flow/merge/prepare/run/secondary.rs`

Modify:
- `src/core/registry/flow/merge/prepare/run.rs`

Delete:
- None.

## 5. Testing Strategy

- Unit/compile-level:
  - `cargo test --test integration cli_merge_prepare_blocked_emits_error_event`
  - run the merge-prepare integration test already covering non-completed flow behavior near `tests/integration.rs:2958-2972`
- End-to-end/script-level if needed:
  - `hivemind-test/test_merge.sh`
- Manual spot checks:
  - prepared fast-path still returns existing prepared state
  - detached `HEAD` still errors with the same hint
  - required merge checks still emit start/completed events and capture log snippets
  - multi-repo secondary preparation still stops per repo/task exactly as before

## 6. Rollback Plan

- Revert the new `run/` child modules and restore the previous single-file body of `merge_prepare`.
- Because there are no schema or data changes, rollback is code-only.
- If behavior diverges, first inline `prepare_secondary_repositories`, then `run_aggregated_merge_checks`, then `prepare_primary_repo` to isolate which extraction changed semantics.

## 7. Estimated Effort

- Effort: 2-4 hours including compile/test iteration.
- Complexity: Medium.
- Main risk: accidental behavior drift from changing when values are owned vs borrowed, especially around conflict accumulation and event emission order.
- Mitigation: return owned outcome structs/vectors, avoid returning references into `AppState`, and keep git command blocks intact when extracting.

