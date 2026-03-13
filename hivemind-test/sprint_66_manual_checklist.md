# Sprint 66 Manual Checklist

Date: 2026-03-13
Status: Executed

## Scope

- Typed workflow context with explicit initialization inputs, revisions, and snapshot hashes
- Deterministic step input resolution and workflow-derived attempt-context manifest injection
- Append-only workflow output bag with reducer-driven join fan-in
- Join-step native execution on top of the Sprint 65 synthetic flow bridge
- Explicit failure behavior for reducer conflicts and schema mismatches
- Real-runtime prompt delivery through `opencode` with workflow step context and attempt-manifest attribution

## Checklist

- [x] Create a project and attach a real git repository fixture
- [x] Configure a working runtime adapter for the project
- [x] Create a workflow with two parallel `task` branches and one downstream `join` step
- [x] Create a workflow run with typed workflow context initialization input
- [x] Start and tick the workflow until the join step completes
- [x] Confirm `workflow status` shows a context snapshot revision increment and a `joined_summary` context key
- [x] Confirm `workflow status` shows append-only bag entries for both branch outputs and the join output
- [x] Confirm `events list --workflow-run` includes `workflow_context_initialized`, `workflow_context_snapshot_captured`, `workflow_step_inputs_resolved`, `workflow_output_appended`, and `attempt_context_assembled`
- [x] Confirm the attempt-context manifest includes the new `workflow` section with workflow/step hashes
- [x] Validate duplicate single-producer expectation fails explicitly through the real CLI
- [x] Validate schema mismatch between expected and produced output payloads fails explicitly through the real CLI
- [x] Confirm a live `opencode/nemotron-3-super-free` runtime receives the Sprint 66 workflow step context in its prompt
- [x] Confirm a live runtime writes expected branch outputs in isolated worktrees
- [x] Confirm live external runtimes can drive checkpoint completion through orchestration-level `ACT:tool:checkpoint_complete` handling without manual recovery
- [x] Confirm synthetic workflow join fan-in still completes natively even when branch steps use external runtimes
- [x] Confirm the shared no-progress watchdog fails silent external runtimes loudly instead of letting attempts hang indefinitely
- [x] Publish the Sprint 66 manual report artifact under `hivemind-test`

## Evidence Summary

- Manual smoke root: `/tmp/hm_sprint66_manual_EiOQnU`
- Positive run id: `74231b30-21b7-4c6d-8fb0-dcce655358e4`
- Positive workflow completed with `joined_summary = "alpha\nbeta"` and three output-bag entries
- Attempt manifest captured `workflow_run_id`, `step_id`, `step_run_id`, `context_snapshot_hash`, `step_input_snapshot_hash`, and `output_bag_hash`
- Negative reducer case failed with `workflow_output_reduce_failed`
- Negative schema case failed with `workflow_output_schema_mismatch`
- Post-fix real runtime root: `/tmp/hm_sprint66_verify6_mOmSO2`
- Post-fix real runtime run id: `3f24f2af-9178-4e71-9645-47710fc9abde`
- Real runtime branch attempts emitted filesystem observations, wrote `branch-a.txt` / `branch-b.txt`, and auto-completed checkpoints through shared external-runtime directive handling
- The post-fix real run advanced both branch outputs into the workflow output bag with deterministic fan-in ordering (`alpha` then `beta`)
- Additional watchdog evidence: `/tmp/hm_sprint66_verify7_AYWVtS` recorded a retryable `no_observable_progress_timeout` on one branch instead of allowing an unbounded hang

## Notes

- The CLI smoke used the locally built binary at `/tmp/hivemind-target/debug/hivemind` to avoid repeated `cargo run` recompilation during manual validation.
- The positive manual run preserved reducer order from declared `producer_step_ids` (`alpha` then `beta`) rather than completion order.
- The remaining variability in the final live rerun is model behavior on remote attempts, not the Sprint 66 workflow data plane or checkpoint bridge.
