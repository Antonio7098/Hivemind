# Sprint 56 Manual Checklist

Date: 2026-02-28
Owner: Antonio
Sprint: 56 (Transport Resilience, Retry, And Fallback)

## 56.1 Retry Policy Framework

- [x] Validate bounded provider retry policy fields are honored (`max_attempts`, `base_delay`, retry toggles for 429/5xx/transport).
- [x] Validate exponential backoff with deterministic jitter is applied on retryable transport failures.
- [x] Validate retry metadata is captured in native invocation transport telemetry.

## 56.2 Streaming Robustness

- [x] Validate streaming idle-timeout classification (`native_stream_idle_timeout`) is explicit and attributable.
- [x] Validate incomplete/failed terminal stream classifications are explicit (`native_stream_terminal_incomplete`, `native_stream_terminal_failed`).
- [x] Validate retry delay and retryable classification are visible through runtime events (`runtime_error_classified`, `runtime_recovery_scheduled`).

## 56.3 Fallback Transport Strategy

- [x] Validate configured fallback transport endpoint activates when primary transport becomes unhealthy.
- [x] Validate fallback activation state persists for the invocation session and is exported in transport telemetry.
- [x] Validate fallback projection is visible in runtime/report surfaces.

## 56.4 Operational Verification

- [x] Run `hivemind-test/test_worktree.sh` against fresh-clone release binary.
- [x] Run `hivemind-test/test_execution.sh` against fresh-clone release binary.
- [x] Inspect canonical SQLite runtime event evidence under `/tmp/hivemind-test/.hm_home/.hivemind/db.sqlite`.

## Artifacts

- `hivemind-test/test-report/2026-02-28-sprint56-manual-bin.log`
- `hivemind-test/test-report/2026-02-28-sprint56-test_worktree.log`
- `hivemind-test/test-report/2026-02-28-sprint56-test_execution.log`
- `hivemind-test/test-report/2026-02-28-sprint56-event-inspection.log`
- `hivemind-test/test-report/2026-02-28-sprint56-filesystem-inspection.log`
- `hivemind-test/test-report/sprint-56-manual-report.md`
