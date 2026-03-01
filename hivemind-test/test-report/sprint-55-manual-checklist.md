# Sprint 55 Manual Checklist

Date: 2026-02-28
Owner: Antonio
Sprint: 55 (Unified Exec Sessions And Runtime Cancellation Safety)

## 55.1 Unified Exec Contract

- [x] Validate native `exec_command` contract spawns persistent session and returns stable `session_id`.
- [x] Validate native `write_stdin` contract resumes existing session and returns bounded trailing output.
- [x] Validate session IDs remain collision-safe under repeated session allocation.

## 55.2 Process Lifecycle Hardening

- [x] Validate process-group isolation and termination path for session cleanup.
- [x] Validate trailing-output grace handling (`capture_ms` / `wait_ms`) returns deterministic output snapshots.
- [x] Validate deterministic output chunking bounds and truncation metadata (`stdout_truncated`, `stderr_truncated`, `*_truncated_bytes`).

## 55.3 Capacity And Cleanup Controls

- [x] Validate open-session cap and deterministic pruning strategy (prefer exited/LRU outside protected recent set).
- [x] Validate warning projection when session counts approach configured cap.
- [x] Validate runtime cleanup path on adapter terminate (`cleanup_exec_sessions`).

## 55.4 Operational Verification

- [x] Run `hivemind-test/test_worktree.sh` against fresh-clone release binary.
- [x] Run `hivemind-test/test_execution.sh` against fresh-clone release binary.
- [x] Inspect canonical SQLite runtime/filesystem event evidence under `/tmp/hivemind-test/.hm_home/.hivemind/db.sqlite`.

## Artifacts

- `hivemind-test/test-report/2026-02-28-sprint55-manual-bin.log`
- `hivemind-test/test-report/2026-02-28-sprint55-test_worktree.log`
- `hivemind-test/test-report/2026-02-28-sprint55-test_execution.log`
- `hivemind-test/test-report/2026-02-28-sprint55-event-inspection.log`
- `hivemind-test/test-report/2026-02-28-sprint55-filesystem-inspection.log`
- `hivemind-test/test-report/sprint-55-manual-report.md`
