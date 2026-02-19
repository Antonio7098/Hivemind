# Beta Testing Fix Report: Tier 12-16 Compatibility and Observability (2026-02-19)

## Scope
- Continue post-tier-11 fixes through remaining tier-12+ beta findings with live script revalidation.
- Focus on product defects and high-value compatibility gaps still impacting operator/beta harness workflows.

## Sources Reviewed
- `24-hour-hivemind/runs/tier_12_constitution_and_graph_snapshot_contracts_sprint_36_37/tier_12_constitution_and_graph_snapshot_contracts_sprint_36_37-FINAL-REPORT.md`
- `24-hour-hivemind/runs/tier_13_constitution_enforcement_gates_sprint_38/tier_13_constitution_enforcement_gates_sprint_38-FINAL-REPORT.md`
- `24-hour-hivemind/runs/tier_13_constitution_enforcement_gates_sprint_38/ENF-005/ENF-005-FINAL-REPORT.md`
- `24-hour-hivemind/runs/tier_15_governance_replay_snapshots_and_repair_sprint_40/RCV-004/RCV-004-FINAL-REPORT.md`
- `24-hour-hivemind/runs/tier_15_governance_replay_snapshots_and_repair_sprint_40/RCV-004/tests/additional_error_tests.sh`
- `24-hour-hivemind/runs/tier_16_governance_operability_and_production_hardening_sprint_41/OPS-001/OPS-001-FINAL-REPORT.md`

## Findings Addressed
1. Legacy task command arity broke beta harnesses
- Symptom: `task start|complete|retry <project> <task-id>` failed at CLI parse layer.
- Fix:
  - Added backward-compatible positional support for legacy two-arg forms.
  - Added validation to ensure provided task belongs to provided project (`task_project_mismatch` on mismatch).
  - Added hidden legacy `task complete --success/--message` compatibility handling; `--success false` maps to explicit task close behavior.
- Files:
  - `src/cli/commands.rs`
  - `src/main.rs`

2. Retry event naming mismatch with ENF-005 evidence (`task_retried`)
- Symptom: report/harness expected `task_retried`, runtime emitted `task_retry_requested`.
- Fix:
  - Retry event now serializes as `task_retried`.
  - Backward deserialization alias preserved for historical logs (`task_retry_requested`).
  - CLI table label aligned to `task_retried`.
- Files:
  - `src/core/events.rs`
  - `src/main.rs`

3. Missing `events list --error-type` filter used by RCV-004 additional tests
- Symptom: `--error-type` rejected as unexpected argument.
- Fix:
  - Added `--error-type` to `events list` and `events stream`.
  - Implemented `EventFilter.error_type` matching for `error_occurred` category.
- Files:
  - `src/cli/commands.rs`
  - `src/main.rs`
  - `src/storage/event_store.rs`

## Regression Tests Added
- `tests/integration.rs`
  - `cli_task_commands_accept_legacy_project_task_arity`
  - `cli_events_list_supports_error_type_filter`
- `src/core/events.rs`
  - `task_retry_payload_uses_task_retried_type_and_accepts_legacy_alias`
- `src/storage/event_store.rs`
  - `in_memory_store_filters_error_events_by_error_type`

## Validation Executed
- `cargo fmt --all`
- `cargo test -q cli_task_commands_accept_legacy_project_task_arity -- --nocapture`
- `cargo test -q cli_events_list_supports_error_type_filter -- --nocapture`
- `cargo test -q in_memory_store_filters_error_events_by_error_type -- --nocapture`
- `cargo test -q task_retry_payload_uses_task_retried_type_and_accepts_legacy_alias -- --nocapture`
- `cargo test -q merge_prepare_approve_and_execute_enforce_constitution_hard_gates -- --nocapture`
- `cargo test -q cli_governance_replay_verify_and_diagnose_detect_missing_artifact_files -- --nocapture`

## Beta Script Reruns
1. `ENF-005/tests/focused-anti-magic-test.sh`
- Product assertions now pass (`PASS: 9`, `FAIL: 0`) when run in isolation.
- Harness footer still prints false failure because script arithmetic parses `0\n0` (script bug).

2. `ENF-005/tests/anti-magic-test-v2.sh`
- Legacy parse failures resolved (no more `unexpected argument` for project+task arity).
- Remaining FAILs are harness/model mismatch:
  - Script has invalid jq boolean checks (object output compared to literal `found`/`valid`).
  - Script expects non-flow tasks to emit `task_retried`/`task_completed` lifecycle semantics that are not valid for `task_not_in_flow` paths.
- Evidence from latest rerun:
  - `events-test1-after-create.json` contains the just-created task event and correlation (`MATCH_COUNT: 1`, matching `LAST_TASK`).
  - Script still reports `TaskCreated event not found` because its jq command appends object output before the sentinel string and the shell equality check fails.

3. `RCV-004/tests/additional_error_tests.sh`
- `--error-type user` path now succeeds and returns filtered `error_occurred` events.
- Script still exits early by design at Test A4 due `set -e` + intentional failing command (harness behavior).

## Notes
- Tier-12 merge constitution hard-gate behavior is already covered and passing by existing regression (`merge_prepare_approve_and_execute_enforce_constitution_hard_gates`), so no additional code change was required in this pass.
- Binary path synchronization is required for script correctness in this environment:
  - build output: `/home/antonio/.cargo/shared-target/release/hivemind`
  - scripts use: `~/.cargo/bin/hivemind`
