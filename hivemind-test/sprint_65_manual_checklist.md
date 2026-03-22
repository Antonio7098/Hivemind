# Sprint 65 Manual Checklist

Date: 2026-03-12
Status: Executed

## Scope

- Flat workflow execution bridge on top of the existing attempt/worktree/verification machinery
- Workflow CLI/API lifecycle surface: `create`, `list`, `inspect`, `start`, `tick`, `pause`, `resume`, `abort`
- Workflow-aware event attribution and correlation for synthetic flow/task/attempt execution

## Checklist

- [x] Create a project and attach a real repository
- [x] Configure a working runtime adapter for the project
- [x] Create a flat workflow with at least three `task` steps
- [x] Add at least one dependency edge and confirm downstream steps do not run early
- [x] Create a workflow run and start it through the CLI
- [x] Tick the workflow until all steps complete
- [x] Confirm attempt/worktree/verification activity is visible through event inspection
- [x] Pause a running workflow and verify status reflects `paused`
- [x] Resume the paused workflow and verify execution continues
- [x] Exercise a retry path and confirm failed workflow execution can be retried to completion through the bridged task retry path
- [x] Abort a running workflow and verify the workflow and synthetic flow both become terminal
- [x] Confirm unsupported step kinds fail with a clear Sprint 65 limitation error
- [x] Confirm workflow-run event filters return workflow-native and bridged execution events

## Evidence Summary

- Manual smoke root: `/tmp/hm_sprint65_manual_tvdGYG`
- Successful three-step workflow run completed after three `workflow tick --max-parallel 1` invocations
- Lifecycle validation confirmed `paused`, `running`, and `aborted` states on a separate workflow run
- Event inspection showed `workflow_run_*`, `workflow_step_state_changed`, `attempt_started`, checkpoint, and bridged task/flow events carrying workflow correlation ids
- Retry validation used a runtime that fails once, then recovers after `task retry <synthetic-task-id> --mode continue` plus a follow-up `workflow tick`
- Unsupported nested workflow step kinds remain rejected for Sprint 65 execution

## Notes

- Automated coverage for the unsupported-kind, dependency, invalid-transition, and correlation cases was extended on 2026-03-12.
- Workflow-native retry remains bridged through the synthetic task retry path; there is still no dedicated `workflow retry` command in Sprint 65.
