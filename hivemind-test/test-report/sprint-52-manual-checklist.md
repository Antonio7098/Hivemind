# Sprint 52 Manual Checklist

Date: 2026-02-28
Owner: Antonio
Sprint: 52 (Host and Process Hardening Baseline)

## 52.1 Pre-main Hardening

- [x] Validate startup hardening runs before normal CLI processing (`main` pre-main gate).
- [x] Validate startup hardening failure path emits structured `startup_hardening_failed` payload and exits with code `70`.
- [x] Validate loader environment hardening clears dangerous loader keys (`LD_*`, `DYLD_*`).

## 52.2 Runtime Environment Hardening

- [x] Validate protected runtime environment construction supports inherit modes `all|core|none`.
- [x] Validate inherited sensitive keys (`*KEY*`, `*SECRET*`, `*TOKEN*`) are dropped unless explicitly overlaid.
- [x] Validate inherited reserved internal runtime keys are filtered and rebuilt from attempt context.
- [x] Validate deterministic env overlays and explicit provenance emission via `runtime_environment_prepared`.

## 52.3 Operational Hardening Verification

- [x] Run `hivemind-test/test_worktree.sh` against release binary.
- [x] Run `hivemind-test/test_execution.sh` against release binary.
- [x] Inspect canonical SQLite runtime events and filesystem observation evidence in `/tmp/hivemind-test/.hm_home/.hivemind/db.sqlite`.

## Artifacts

- `hivemind-test/test-report/2026-02-28-sprint52-test_worktree.log`
- `hivemind-test/test-report/2026-02-28-sprint52-test_execution.log`
- `hivemind-test/test-report/2026-02-28-sprint52-event-inspection.log`
- `hivemind-test/test-report/2026-02-28-sprint52-filesystem-inspection.log`
- `/tmp/hivemind-test_worktree.log`
- `/tmp/hivemind-test_execution.log`
- `hivemind-test/test-report/sprint-52-manual-report.md`
