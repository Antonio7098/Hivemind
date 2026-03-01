# Sprint 57 Manual Checklist

Date: 2026-03-01
Owner: Antonio
Sprint: 57 (Durable Runtime State, Secure Secrets, And Readiness Gates)

## 57.1 Runtime State Durability

- [x] Validate dedicated native runtime state DB initializes with WAL mode and busy timeout.
- [x] Validate runtime state DB applies versioned migrations deterministically.
- [x] Validate lease/heartbeat ownership for native background jobs (`native_runtime_log_ingestor`, `native_runtime_log_retention`).
- [x] Validate batched async runtime log ingestion persists records and survives restart/reopen.
- [x] Validate retention cleanup job is lease-governed and does not break runtime execution.

## 57.2 Secret Handling Hardening

- [x] Validate native secrets manager writes encrypted local store (no plaintext secret persistence).
- [x] Validate key material is loaded from keyring-backed path (or explicit env override) and reused across restarts.
- [x] Validate secret store writes use atomic write/replace semantics.
- [x] Validate temporary plaintext buffers are wiped in write/decrypt paths.

## 57.3 Component Readiness Gates

- [x] Validate tokenized readiness transitions are emitted for runtime async dependencies.
- [x] Validate startup sequencing blocks execution until readiness gate is satisfied.
- [x] Validate readiness transitions are projected into runtime event stream.

## 57.4 Operational Verification

- [x] Run `hivemind-test/test_worktree.sh` against fresh-clone release binary.
- [x] Run `hivemind-test/test_execution.sh` against fresh-clone release binary.
- [x] Inspect canonical SQLite runtime event evidence under `/tmp/hivemind-test/.hm_home/.hivemind/db.sqlite`.

## Artifacts

- `hivemind-test/test-report/2026-03-01-sprint57-manual-bin.log`
- `hivemind-test/test-report/2026-03-01-sprint57-test_worktree.log`
- `hivemind-test/test-report/2026-03-01-sprint57-test_execution.log`
- `hivemind-test/test-report/2026-03-01-sprint57-event-inspection.log`
- `hivemind-test/test-report/2026-03-01-sprint57-filesystem-inspection.log`
- `hivemind-test/test-report/sprint-57-manual-report.md`
