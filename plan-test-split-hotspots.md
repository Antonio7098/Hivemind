## 1. Overview
- Split the remaining oversized test hotspots conservatively, without changing test behavior.
- Primary targets: `tests/integration.rs` (4351 lines) and `src/server/tests.rs` (589 lines).
- Secondary but relevant targets: `src/native/tests.rs` (1325 lines) and `src/native/tool_engine/tests.rs` (1566 lines).
- Success criteria:
  - smaller capability-based files
  - shared helpers preserved in existing root/helper files
  - each move validated with the narrowest possible `cargo test` command
- Out of scope:
  - test logic rewrites
  - helper dedupe across unrelated modules
  - package/dependency changes

## 2. Prerequisites
- No new dependencies, migrations, or config changes.
- Keep existing helper entry points in place first:
  - `tests/support.rs`
  - `src/server/tests.rs`
  - `src/native/tests.rs`
  - `src/native/tool_engine/tests.rs`
- Preserve current `#[cfg(test)] mod tests;` wiring in `src/server.rs`, `src/native/mod.rs`, and `src/native/tool_engine.rs`.

## 3. Implementation Steps
### Step 1: Split `tests/integration.rs` by CLI capability using top-level files in `tests/`
- Use separate top-level integration files, matching the existing `tests/worktree_flow.rs` + `tests/support.rs` pattern.
- Create `mod support; use support::*;` in each new file.
- Recommended end-state groupings:
  - `tests/governance_cli.rs`
    - governance lifecycle/init
    - sprint35 artifacts/templates
    - sprint36 constitution lifecycle
    - constitution check
    - governance diagnose/stale snapshot
    - governance concurrent artifact ops
    - governance snapshot restore/repair
    - governance replay/restore/diagnose flows
  - `tests/events_cli.rs`
    - error-type filtering
    - events stream filters
    - events replay/verify
    - events mirror recover/verify
  - `tests/runtime_attempts_cli.rs`
    - scope violation + tmp/worktree guards
    - attempt inspect / attempt diff / attempt context
    - runtime config + flow tick
    - checkpoint lifecycle
    - worktree cleanup / flow restart / attempt list / abort flow
  - `tests/verification_merge_cli.rs`
    - verify run/results
    - verify override audit trail
    - merge lifecycle
    - merge prepare blocked
  - `tests/query_output_cli.rs`
    - graph query bounds
    - graph/flow list project filter
    - YAML output
    - not-found exit codes
    - legacy task arity if no better cluster emerges
- Low-risk first move: extract `tests/governance_cli.rs` first; it is the largest coherent slice and already depends only on `tests/support.rs`.
- Second move: extract `tests/runtime_attempts_cli.rs`, anchored by the 491-line `cli_attempt_inspect_context_returns_manifest_and_retry_linkage` hotspot.
- Testing: run each new target directly, e.g. `cargo test --test governance_cli`, then `cargo test --test runtime_attempts_cli`.

### Step 2: Convert `src/server/tests.rs` into a shared helper root plus child modules
- Keep `src/server/tests.rs` as the root helper file; do not rename it yet.
- Add child modules under `src/server/tests/`.
- Keep shared helpers in root:
  - `test_registry`
  - `json_value`
  - `api_request`
  - `native_blob_ref`
  - `seed_runtime_projection_attempt`
- Recommended groupings:
  - `src/server/tests/api_core.rs`
    - version/state/not-found
    - project create/delete
  - `src/server/tests/chat.rs`
    - `api_chat_invoke_ok_with_mock_provider`
    - `api_chat_sessions_create_send_and_inspect_round_trip`
  - `src/server/tests/runtime_stream.rs`
    - runtime stream empty
    - projected runtime items
    - detail levels
  - `src/server/tests/worktrees.rs`
    - restore-turn confirmation guard
- Low-risk first move: extract `chat.rs` and `runtime_stream.rs`; they already form clean clusters around existing shared helpers.
- Testing: run targeted filters after each move, e.g. `cargo test api_chat_` and `cargo test api_runtime_stream_`.

### Step 3: If continuing, split `src/native/tests.rs` by runtime capability
- Keep `src/native/tests.rs` as the root helper file initially.
- Preserve root-local helpers first:
  - `native_input`
  - `allow_all_scope`
  - `test_tool_context`
  - `RecordingModelClient`
- Recommended groupings:
  - `src/native/tests/agent_loop.rs`
    - state transitions, relaxed parsing, malformed output recovery
  - `src/native/tests/budget_history.rs`
    - budget compaction, stabilization, recorded budget pressure
  - `src/native/tests/checkpoints.rs`
    - all checkpoint-repair and auto-completion cases
  - `src/native/tests/prompt_history.rs`
    - prompt assembly determinism, turn-item normalization, replayed history
  - `src/native/tests/tool_mode.rs`
    - planner-mode mutation denial, tool action parsing
- Low-risk first move: `checkpoints.rs`; it is contiguous, repetitive, and can later absorb a small local helper for fake `HIVEMIND_BIN` setup.
- Do not try to dedupe helpers with `src/native/tool_engine/tests.rs` in the same change.

### Step 4: If continuing, split `src/native/tool_engine/tests.rs` by tool capability
- Keep `src/native/tool_engine/tests.rs` as the root helper file first.
- Preserve root helpers first:
  - `lock_exec_session_tests`
  - git/snapshot helpers
  - `allow_all_scope`
  - `test_tool_context*`
- Recommended groupings:
  - `src/native/tool_engine/tests/validation.rs`
  - `src/native/tool_engine/tests/git_tools.rs`
  - `src/native/tool_engine/tests/run_command.rs`
  - `src/native/tool_engine/tests/approval_sandbox.rs`
  - `src/native/tool_engine/tests/network_policy.rs`
  - `src/native/tool_engine/tests/graph_query.rs`
  - `src/native/tool_engine/tests/exec_sessions.rs`
  - `src/native/tool_engine/tests/perf.rs`
- Low-risk first moves:
  - `network_policy.rs` (tight, contiguous policy cluster)
  - `exec_sessions.rs` (already guarded and capability-specific)
- Keep `write_snapshot_artifact` in root until the graph-query move is stable.

## 4. File Changes Summary
- Create later:
  - `plan-test-split-hotspots.md`
  - new `tests/*.rs` files listed above
  - new `src/server/tests/*.rs` files listed above
  - optionally new `src/native/tests/*.rs` and `src/native/tool_engine/tests/*.rs` files
- Modify later:
  - `tests/integration.rs`
  - `src/server/tests.rs`
  - optionally `src/native/tests.rs`
  - optionally `src/native/tool_engine/tests.rs`
- Delete later:
  - none in the conservative phase

## 5. Testing Strategy
- After each extraction, run only the moved scope first.
- Then run the owning hotspot file’s remaining tests.
- Suggested order:
  - `cargo test --test governance_cli`
  - `cargo test --test runtime_attempts_cli`
  - `cargo test api_chat_`
  - `cargo test api_runtime_stream_`
  - then a broader `cargo test` only after the staged moves are stable.

## 6. Rollback Plan
- Revert newly created test files.
- Move tests back into the original hotspot file if module wiring proves awkward.
- No data rollback required.

## 7. Estimated Effort
- `tests/integration.rs`: medium, ~2-4 hours across 2-3 PR-sized moves.
- `src/server/tests.rs`: low, ~30-60 minutes.
- `src/native/tests.rs`: medium, ~1-2 hours if included.
- `src/native/tool_engine/tests.rs`: medium, ~1.5-3 hours if included.
- Overall complexity: medium, with low behavioral risk if helpers remain rooted during the first pass.

