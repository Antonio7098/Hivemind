# Sprint 66 Manual Report

Date: 2026-03-13
Status: Complete

## Manual Smoke Summary

- Executed a real CLI workflow smoke run under `/tmp/hm_sprint66_manual_EiOQnU`
- Verified a parallel-branch workflow with join fan-in and explicit workflow context patching
- Verified workflow-derived attempt manifest content and hash attribution through `events list --workflow-run`
- Verified explicit failure behavior for duplicate single-producer reduction and workflow output schema mismatch
- Executed post-fix real runtime smokes under `/tmp/hm_sprint66_verify6_mOmSO2` and `/tmp/hm_sprint66_verify7_AYWVtS` using `opencode/nemotron-3-super-free`
- Verified Sprint 66 workflow step context, attempt-manifest hashes, and append-only bag semantics were exercised against a live remote runtime after the external-runtime checkpoint bridge fix
- Verified orchestration-level handling of `ACT:tool:checkpoint_complete` for non-native runtimes and fail-fast watchdog behavior for silent remote attempts

## Observed Results

- Positive join workflow:
  - two `task` branches appended typed `branch` outputs into the workflow output bag
  - a downstream `join` step reduced those entries with `concat_text_newline`
  - the workflow run reached `completed`
  - `workflow status` showed context revision `2` with `joined_summary = "alpha\nbeta"`
  - `workflow status` showed three bag entries: two branch outputs plus one join-produced `summary`
- Workflow event surface:
  - `events list --workflow-run 74231b30-21b7-4c6d-8fb0-dcce655358e4` contained `workflow_context_initialized`
  - the same stream contained `workflow_context_snapshot_captured`, `workflow_step_inputs_resolved`, `workflow_output_appended`, and `workflow_run_completed`
  - the same stream contained `attempt_context_assembled`, proving workflow data reached the attempt manifest path
- Attempt manifest:
  - manifest schema version was `attempt_context.v3`
  - manifest included a `workflow` section with `workflow_run_id`, `step_id`, `step_run_id`, `context_snapshot_hash`, `step_input_snapshot_hash`, and `output_bag_hash`
  - the observed manifest for the positive run referenced the branch-step attempt context before the join completed, which is correct for attempt launch timing on the Sprint 65 bridge
- Negative reducer validation:
  - second tick on workflow run `de0f1213-2103-4117-bcfd-5718c183a442` failed with `workflow_output_reduce_failed`
  - the CLI error was: `single reducer expected exactly one output entry for 'branch', found 2`
- Negative schema validation:
  - second tick on workflow run `cdd7efbf-2693-4a90-adfd-1b520bb9a1d4` failed with `workflow_output_schema_mismatch`
  - the CLI error was: `Workflow output 'branch' expected schema 'application/json' but step 'b79af42c-b591-474c-ab4e-2bdcee852b63' produced 'text/plain'`
- Real runtime manifest injection:
  - workflow run `3f24f2af-9178-4e71-9645-47710fc9abde` launched real `opencode` attempts against `opencode/nemotron-3-super-free`
  - `events list --workflow-run 3f24f2af-9178-4e71-9645-47710fc9abde` showed `attempt_context_assembled` and `context_window_snapshot_created`
  - the delivered prompt included `Workflow Step Context` with `workflow_run_id`, `step_id`, `step_run_id`, `context_snapshot_hash`, `step_input_snapshot_hash`, and `output_bag_hash`
  - the real runtime created `branch-a.txt` and `branch-b.txt` with the expected contents in their isolated worktrees
- Real runtime checkpoint bridge after fix:
  - branch attempt `89382fb5-c646-49fd-b926-7f9a9c5ef3c2` emitted `ACT:tool:checkpoint_complete:{...}` for `branch-b.txt`
  - branch attempt `96612d87-86a8-4b8f-922a-e90b475a84db` emitted `ACT:tool:checkpoint_complete:{...}` for `branch-a.txt`
  - both checkpoints were completed automatically by the shared external-runtime directive bridge with no manual `hivemind checkpoint complete` recovery
  - the workflow output bag appended both branch outputs after live execution, and the reducer input order matched declared `producer_step_ids` (`alpha` then `beta`)
- Shared watchdog behavior:
  - in run `fe508ca5-dbfe-428f-b96f-4dd060e54bb6`, the first branch-a live attempt timed out with `no_observable_progress_timeout` and was retried instead of hanging indefinitely
  - this confirmed the shared watchdog applies above any single runtime adapter surface
- Remaining live variability:
  - run `3f24f2af-9178-4e71-9645-47710fc9abde` proved the branch execution bridge and output-bag integration end-to-end on a live runtime
  - run `fe508ca5-dbfe-428f-b96f-4dd060e54bb6` showed the watchdog recovery path under a real remote stall
  - final live completion still depends on model cooperation on remote attempts, but the Sprint 66 workflow data plane and the external-runtime bridge behavior were both verified directly

## Validation Commands

```bash
CARGO_TARGET_DIR=/tmp/hivemind-target cargo check
CARGO_TARGET_DIR=/tmp/hivemind-target cargo test workflow_ --quiet
CARGO_TARGET_DIR=/tmp/hivemind-target cargo test --test integration workflow_tick

env HOME=/tmp/hm_sprint66_manual_EiOQnU/home /tmp/hivemind-target/debug/hivemind -f json workflow status 74231b30-21b7-4c6d-8fb0-dcce655358e4
env HOME=/tmp/hm_sprint66_manual_EiOQnU/home /tmp/hivemind-target/debug/hivemind -f json events list --workflow-run 74231b30-21b7-4c6d-8fb0-dcce655358e4
env HOME=/tmp/hm_sprint66_manual_EiOQnU/home /tmp/hivemind-target/debug/hivemind -f json workflow tick de0f1213-2103-4117-bcfd-5718c183a442 --max-parallel 2
env HOME=/tmp/hm_sprint66_manual_EiOQnU/home /tmp/hivemind-target/debug/hivemind -f json workflow tick cdd7efbf-2693-4a90-adfd-1b520bb9a1d4 --max-parallel 1

env HOME=/tmp/hm_sprint66_real_dfBXpY/home /tmp/hivemind-target/debug/hivemind -f json workflow tick f22491fc-a99b-4b8e-b4f6-887958ff6a47 --max-parallel 2
env HOME=/tmp/hm_sprint66_real_dfBXpY/home /tmp/hivemind-target/debug/hivemind -f json events list --workflow-run f22491fc-a99b-4b8e-b4f6-887958ff6a47 --limit 200
env HOME=/tmp/hm_sprint66_real_dfBXpY/home /tmp/hivemind-target/debug/hivemind -f json attempt inspect 94dcb1a5-3618-4f4e-8f12-0bb0368fc9e4
env HOME=/tmp/hm_sprint66_real_dfBXpY/home /tmp/hivemind-target/debug/hivemind -f json attempt inspect 1108fe41-618c-48af-9286-33d9b1645a21
env HOME=/tmp/hm_sprint66_real_dfBXpY/home /tmp/hivemind-target/debug/hivemind checkpoint complete --attempt-id 94dcb1a5-3618-4f4e-8f12-0bb0368fc9e4 --id checkpoint-1 --summary 'manual completion after real runtime smoke'
env HOME=/tmp/hm_sprint66_real_dfBXpY/home /tmp/hivemind-target/debug/hivemind checkpoint complete --attempt-id 1108fe41-618c-48af-9286-33d9b1645a21 --id checkpoint-1 --summary 'manual completion after ACT line did not invoke checkpoint bridge'
env HOME=/tmp/hm_sprint66_real_dfBXpY/home /tmp/hivemind-target/debug/hivemind -f json flow status 1b5e357b-274c-4db6-bc1d-a4642ec4e87f

env HOME=/tmp/hm_sprint66_verify6_mOmSO2/home /tmp/hivemind-target/debug/hivemind -f json workflow tick 3f24f2af-9178-4e71-9645-47710fc9abde --max-parallel 2
env HOME=/tmp/hm_sprint66_verify6_mOmSO2/home /tmp/hivemind-target/debug/hivemind -f json workflow status 3f24f2af-9178-4e71-9645-47710fc9abde
env HOME=/tmp/hm_sprint66_verify6_mOmSO2/home /tmp/hivemind-target/debug/hivemind -f json events list --workflow-run 3f24f2af-9178-4e71-9645-47710fc9abde --limit 320

env HOME=/tmp/hm_sprint66_verify7_AYWVtS/home /tmp/hivemind-target/debug/hivemind -f json workflow status fe508ca5-dbfe-428f-b96f-4dd060e54bb6
env HOME=/tmp/hm_sprint66_verify7_AYWVtS/home /tmp/hivemind-target/debug/hivemind -f json events list --workflow-run fe508ca5-dbfe-428f-b96f-4dd060e54bb6 --limit 320
```

## Evidence Files

- `/tmp/hm_sprint66_manual_EiOQnU/status.json`
- `/tmp/hm_sprint66_manual_EiOQnU/events.json`
- `/tmp/hm_sprint66_manual_EiOQnU/duplicate_single_producer_tick.txt`
- `/tmp/hm_sprint66_manual_EiOQnU/schema_mismatch_tick.txt`
- `/tmp/hm_sprint66_manual_EiOQnU/workflow_create.json`
- `/tmp/hm_sprint66_manual_EiOQnU/run_create.json`
- `/tmp/hm_sprint66_real_dfBXpY/status_final.json`
- `/tmp/hm_sprint66_real_dfBXpY/status_after_checkpoint2.json`
- `/tmp/hm_sprint66_real_dfBXpY/events_final.json`
- `/tmp/hm_sprint66_real_dfBXpY/checkpoint1.txt`
- `/tmp/hm_sprint66_real_dfBXpY/checkpoint2.txt`
- `/tmp/hm_sprint66_real_dfBXpY/flow_status_after_tick1.json`
- `/tmp/hm_sprint66_verify6_mOmSO2/tick1.json`
- `/tmp/hm_sprint66_verify6_mOmSO2/status.json`
- `/tmp/hm_sprint66_verify7_AYWVtS/home/.hivemind`
