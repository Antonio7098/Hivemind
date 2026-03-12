# Sprint 65 Manual Report

Date: 2026-03-12
Status: Complete

## Manual Smoke Summary

- Executed a real repository workflow smoke run under `/tmp/hm_sprint65_manual_tvdGYG`
- Verified a three-step flat workflow (`step-a -> step-b -> step-c`) through the real CLI and runtime bridge
- Verified separate pause/resume/abort lifecycle behavior on a second workflow run
- Verified failure and recovery on a third workflow run using a fail-once runtime and the bridged synthetic task retry path
- Verified workflow event inspection includes workflow-native events and bridged task/attempt/checkpoint events with workflow correlation

## Observed Results

- Three-step workflow:
  - initial status showed only the root step `ready`
  - after tick 1, `step-a` was `succeeded` while downstream steps remained blocked
  - after tick 2, the dependent middle step completed and the final step remained blocked until release
  - after tick 3, the workflow run reached `completed`
- Lifecycle run:
  - `workflow pause` changed the run to `paused`
  - `workflow resume` returned it to `running`
  - `workflow abort --reason stop-now` moved it to `aborted`
  - event inspection contained `workflow_run_paused`, `workflow_run_resumed`, and `workflow_run_aborted`
- Retry run:
  - the first tick used a runtime command that exited non-zero on purpose
  - the workflow step moved to `failed`
  - `task retry <synthetic-task-id> --mode continue` followed by another `workflow tick` completed the run successfully
  - event inspection contained `task_retried`, `task_execution_succeeded`, and `workflow_run_completed`
- Attribution:
  - workflow-run event filtering returned workflow-native and bridged execution events together
  - bridged events carried `workflow_id`, `workflow_run_id`, `step_id`, `step_run_id`, and attempt ids where applicable

## Current Limitation Confirmed

- Sprint 65 execution still rejects non-`task` workflow step kinds during `workflow tick`
- Retry is operational, but the operator-facing retry action still goes through the synthetic task bridge rather than a dedicated `workflow retry` command

## Validation Commands

```bash
cargo test workflow_tick_bridges_task_steps_into_synthetic_flow_execution --quiet
cargo test workflow_tick_rejects_unsupported_step_kinds --quiet
cargo test workflow_endpoints_tick_pause_resume_and_abort --quiet
cargo test workflow_endpoints_reject_invalid_step_transition --quiet
cargo test workflow_endpoints_reject_unknown_step_dependency --quiet
cargo test workflow_add_step_deduplicates_dependencies --quiet
cargo test workflow_bridge_correlation_for_flow_id_event_includes_workflow_metadata --quiet
cargo test cli_workflow_tick_executes_flat_task_steps --quiet
cargo test cli_workflow_tick_rejects_unsupported_step_kinds --quiet
cargo test workflow_ --quiet
```
