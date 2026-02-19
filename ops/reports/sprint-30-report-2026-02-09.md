# Sprint 30: Production Hardening

## 1. Sprint Metadata

- **Sprint**: 30
- **Title**: Production Hardening
- **Branch**: `sprint/30-production-hardening`
- **Date**: 2026-02-17
- **Owner**: Antonio

## 2. Objectives

- Make runtime failures first-class, observable events.
- Centralize retry/fallback behavior for transient runtime failures.
- Handle early runtime termination/checkpoint incompletion safely without losing attempt context.
- Validate behavior with both simulated and real OpenCode runtime scenarios in `../hivemind-test`.

## 3. Delivered Changes

- Added runtime failure observability events:
  - `runtime_error_classified`
  - `runtime_recovery_scheduled`
- Centralized runtime failure handling in the registry for initialize/prepare/execute/nonzero-exit/checkpoint-gating paths.
- Added fallback recovery scheduling with backoff and worker runtime adapter switch when applicable.
- Restricted automatic retries to transient/runtime transport failures (rate limiting/timeouts), preventing broad unintended retries.
- Preserved stdout/stderr on failed execution reports for downstream classification.
- Added stderr-aware detection for wrapped-runtime auth/rate-limit failures even when subprocess exit code is `0`.
- Preserved checkpoint-incomplete attempts as running so operators/agents can still complete checkpoints against the same attempt.

## 4. Automated Validation

- `cargo test --lib core::registry::tests::tick_flow_` ✅
- `cargo test --test integration cli_checkpoint_complete_unblocks_attempt_and_emits_lifecycle_events -- --nocapture` ✅
- `make validate` ✅ (fmt, clippy `-D warnings`, tests, docs)

## 5. Manual Validation (`../hivemind-test`)

Executed in `/home/antonio/programming/Hivemind/hivemind-test/sprint30-manual-CcjxGB`.

### 5.1 Simulated rate-limit failure path

- Runtime: `/usr/bin/env sh -c "echo 'HTTP 429 Too Many Requests' 1>&2; exit 1"`
- Observed:
  - `runtime_output_chunk` captured stderr `HTTP 429 Too Many Requests`.
  - `runtime_error_classified` with `category=rate_limit`, `rate_limited=true`, `retryable=true`.
  - `runtime_recovery_scheduled` emitted with fallback strategy.
  - `task_retry_requested` emitted.

### 5.2 Simulated auth failure path

- Runtime: `/usr/bin/env sh -c "echo '401 Unauthorized: model access denied' 1>&2; exit 1"`
- Observed:
  - stderr captured in `runtime_output_chunk`.
  - `runtime_error_classified` with non-rate-limit category.
  - no `runtime_recovery_scheduled` (expected for non-transient auth-style failure).

### 5.3 Real OpenCode free-model execution

- Model: `opencode/glm-5-free`
- Command: `opencode run --model opencode/glm-5-free --print-logs ...`
- Observed:
  - runtime launched and completed checkpoint lifecycle events.
  - checkpoint completion/commit events emitted.

### 5.4 Real OpenCode auth-error probe

- Probe command: `OPENAI_API_KEY=invalid_key opencode run --model openai/o1-pro --print-logs ...`
- Observed stderr contained explicit OpenAI auth failure text:
  - `Incorrect API key provided: invalid_key`
  - provider-level error payloads in stderr logs.

### 5.5 Free-model rate-limit exhaustion attempt

- Ran 12 repeated direct free-model probes with `opencode/glm-5-free`.
- Results:
  - Attempts 1–11 exited successfully.
  - Attempt 12 timed out (`exit_code=124`) before completion.
  - No definitive 429/rate-limit string observed during this run.

## 6. Follow-up Notes

- Real wrapped OpenCode runs can emit long stderr log streams and may not terminate quickly under certain prompt/tooling combinations; hard timeout and stderr classification paths remain important operational guards.
- Production hardening regression coverage was extended for both transient and auth-related failure parsing.
