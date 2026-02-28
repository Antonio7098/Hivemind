# Sprint 54 Manual Report

Date: 2026-02-28
Owner: Antonio
Sprint: 54 (Managed Network Policy And Host Approvals)
Status: PASS

## Scope

Manual validation for Sprint 54 runtime hardening additions:

- managed network proxy runtime surfaces (HTTP listener, optional SOCKS5 listener, local admin endpoint)
- safe bind-default clamping behavior for non-loopback addresses
- host/domain policy engine controls (deny-wins allow/deny rules, private/local blocking, limited-mode method restrictions)
- host/protocol network approval lifecycle (immediate and deferred) with attributable policy outcomes
- deferred denial watcher termination path for running command sessions
- fresh-clone `hivemind-test` smoke checks and canonical event/filesystem observability confirmation

## Commands and Outcomes

1. Fresh-clone smoke scripts (`hivemind-test`)
   - Commands:
     - `HIVEMIND=<release-binary> ./hivemind-test/test_worktree.sh`
     - `HIVEMIND=<release-binary> ./hivemind-test/test_execution.sh`
   - Outcome:
     - both scripts completed successfully
     - worktree lifecycle and runtime execution paths remained healthy
     - runtime/filesystem observability remained explicit (`runtime_environment_prepared`, `runtime_started`, `runtime_filesystem_observed`, `runtime_exited`)

2. Canonical SQLite runtime event inspection
   - Command:
     - `sqlite3 -json /tmp/hivemind-test/.hm_home/.hivemind/db.sqlite "SELECT sequence, event_json FROM events ORDER BY sequence DESC LIMIT 400;" | rg 'runtime_|task_execution_state_changed|merge_|policy_tags|tool_call_failed'`
   - Outcome:
     - runtime lifecycle and task execution transition events are present and attributable
     - runtime error classification/checkpoint gating behavior remains explicit and recoverable

3. Filesystem observation inspection
   - Command:
     - `sqlite3 -json /tmp/hivemind-test/.hm_home/.hivemind/db.sqlite "SELECT event_json FROM events ORDER BY sequence DESC LIMIT 400;" | rg 'runtime_filesystem_observed|tool_call_'`
   - Outcome:
     - filesystem side effects remained observable and attributable

## Artifacts

- `hivemind-test/test-report/2026-02-28-sprint54-manual-bin.log`
- `hivemind-test/test-report/2026-02-28-sprint54-test_worktree.log`
- `hivemind-test/test-report/2026-02-28-sprint54-test_execution.log`
- `hivemind-test/test-report/2026-02-28-sprint54-event-inspection.log`
- `hivemind-test/test-report/2026-02-28-sprint54-filesystem-inspection.log`

## Notes

- Manual smoke flow was executed from a fresh temp clone with working-tree sync and release build, then driven using explicit binary override recorded in `2026-02-28-sprint54-manual-bin.log`.
- `hivemind-test` smoke scripts exercise wrapper-runtime surfaces by design; Sprint 54 native managed-network controls were additionally validated through strict Rust unit/integration tests and policy-tag assertions.
