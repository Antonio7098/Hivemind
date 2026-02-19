# Sprint 19 Report: Retry Mechanics

**Date:** 2025-02-11  
**Status:** Complete  
**Sprint:** 19 – Retry Mechanics

---

## 1. Objectives vs Outcomes

### 1.1 Goals (from ROADMAP.md)
- [x] Enable intelligent retries with explicit context
- [x] Assemble retry context from prior attempt outcomes, check results, and verifier feedback
- [x] Deliver retry context as explicit input (not hidden memory)
- [x] Enforce retry policy: max attempts, bounded retries, retry only on soft failures

### 1.2 Exit Criteria Met
- [x] Retries receive explicit context
- [x] Context is observable via events
- [x] Retries are bounded (max_attempts enforced)

---

## 2. Implementation Summary

### 2.1 Core Changes
- **Events:** Added `RetryContextAssembled` event payload with full retry context metadata.
- **Registry (`src/core/registry.rs`):**
  - Implemented `build_retry_context` to assemble context from events/artifacts.
  - Modified `tick_flow` to schedule tasks in `Retry` state and emit `RetryContextAssembled`.
  - Pass retry context and prior attempts into `ExecutionInput` for runtime adapters.
  - Enforced retry policy (max attempts, bounded retries, soft-fail-only).
- **State (`src/core/state.rs`):** Recognize `RetryContextAssembled` event (no mutation needed).
- **CLI/Server UI:** Added `RetryContextAssembled` to event labeling and UI mappings.
- **Attempt inspection:** Added `--context` flag to `attempt inspect` to view retry context.

### 2.2 Retry Context Structure
The assembled context includes:
- Current attempt number and max attempts
- Prior attempt IDs and summaries (change counts, failure reasons)
- Required/optional check failures with names and outputs
- Runtime exit code and termination reason
- Human-readable context string for the runtime adapter

### 2.3 Delivery to Runtime
- For retries, `ExecutionInput.context` contains the assembled context string.
- `ExecutionInput.prior_attempts` includes `AttemptSummary` items for each prior attempt.
- Context is visible in `AttemptStarted` events and in `attempt inspect --context`.

---

## 3. Validation Results

### 3.1 Automated Tests
- All unit and integration tests pass (`cargo test --all-features`).
- Integration test `cli_verify_run_and_results_capture_check_outcomes` still exits non-zero on verification failure, preserving CLI semantics.

### 3.2 Manual End-to-End Test
A manual test was performed using the real runtime adapter path:
- **Attempt 1:** Runtime exits 0, required check fails, task transitions `Verifying -> Retry`, CLI exits non-zero.
- **Attempt 2:** Task transitions `Retry -> Running`, `RetryContextAssembled` emitted, runtime receives explicit retry context, check passes, task transitions to `Success`, flow completes.
- **Evidence:** Captured runtime prompt shows retry context; `events.jsonl` contains `retry_context_assembled` event before `runtime_started` for attempt 2.

---

## 4. Observability & Discoverability

### 4.1 CLI Discoverability
- `hivemind task retry <task-id> [--reset-count]` documented and implemented.
- `hivemind attempt inspect <attempt-id> --context` shows retry context.
- `hivemind events stream/list` include `RetryContextAssembled` events.

### 4.2 Documentation
- Design spec `docs/design/retry-context.md` matches implementation.
- CLI operational semantics document `docs/design/cli-operational-semantics.md` includes retry commands.
- No inconsistencies found between docs and implementation.

---

## 5. Sprint 18 Note
Sprint 18 (Verifier Agent) remains unimplemented; its checklist items in `ops/ROADMAP.md` are still unchecked. Sprint 19 does not depend on Sprint 18.

---

## 6. Quality Gates

- [x] Code follows project principles (observability, no magic, explicitness).
- [x] All retry actions and context are observable via events.
- [x] Retry context is explicitly passed; no hidden memory.
- [x] Retry policy is enforced and bounded.
- [x] CLI semantics preserved (exit codes on verification failure).
- [x] Documentation updated and consistent.

---

## 7. Artifacts

- **Event:** `RetryContextAssembled` (observable, contains full context).
- **CLI flag:** `attempt inspect --context` (user-visible context).
- **Runtime input:** `ExecutionInput.context` and `.prior_attempts` (explicit delivery).
- **Test evidence:** Manual test log and captured prompt (available on request).

---

**Sprint 19 is complete and ready for merge.**
