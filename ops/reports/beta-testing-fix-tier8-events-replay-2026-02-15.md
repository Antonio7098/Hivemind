# Tier 8 Fix Note

Date: 2026-02-15  
Branch: `fix/tier6-9-beta-stabilization`

## Fixed Findings
- EVT-004-F1: flow abort now transitions active task executions from RUNNING/VERIFYING to FAILED.
- EVT-004-F2: `task_execution_state_changed` now includes optional `attempt_id` for lineage correlation.
- EVT observability coverage strengthened with abort/merge/cleanup event assertions in integration tests.

## Validation
- `tests/integration.rs::cli_abort_flow_transitions_running_tasks_to_failed`
- `tests/integration.rs::cli_merge_prepare_blocked_emits_error_event`
- `cargo test --all-features -- --test-threads=1`
