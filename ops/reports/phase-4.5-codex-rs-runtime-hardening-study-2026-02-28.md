# Phase 4.5 Codex-rs Runtime Hardening Study

## Metadata

- Date: 2026-02-28
- Scope: Runtime hardening, robustness, and native runtime capability patterns in `@codex-main/codex-rs`
- Target: Define what to port into Hivemind native runtime during Phase 4.5

## Executive Summary

`codex-rs` has a materially deeper runtime hardening stack than current Hivemind native runtime in five areas:

1. Host/process hardening before runtime start.
2. Multi-layer sandbox + approval orchestration with explicit escalation semantics.
3. Managed network enforcement (proxy policy + per-host interactive approvals).
4. Robust long-lived command execution primitives (persistent process sessions, process-group cleanup, cancellation, bounded output streaming).
5. Transport/state resilience (retry policies, idle timeouts, fallback transport, durable runtime state + secure secrets handling).

Hivemind already has strong event contracts, typed tool schemas, scope-aware gating, and deterministic native turn tracing, but still lacks most of the platform/runtime hardening layers above.

## Source Inventory (Codex-rs)

### 1) Process And Startup Hardening

- `codex-rs/process-hardening/src/lib.rs`
  - disables core dumps (`setrlimit(RLIMIT_CORE, 0)`),
  - disables debugger attach (`prctl(PR_SET_DUMPABLE, 0)` on Linux, `ptrace(PT_DENY_ATTACH)` on macOS),
  - strips dangerous loader env vars (`LD_*`, `DYLD_*`),
  - fail-fast exit codes if hardening fails.
- `codex-rs/arg0/src/lib.rs`
  - protected startup behavior for helper aliases,
  - `.env` filtering that blocks `CODEX_*` env injection,
  - janitor cleanup for stale helper dirs,
  - Windows runtime thread stack hardening.

### 2) Sandboxing And Approval Policy

- `codex-rs/core/src/sandboxing/mod.rs`
  - unified sandbox selection/transform path across policies and platforms.
- `codex-rs/core/src/seatbelt.rs`
  - dynamic macOS Seatbelt policy generation with writable/read-only roots and `.git` protection.
- `codex-rs/core/src/landlock.rs`
  - Linux sandbox helper flow (bubblewrap/seccomp) with policy serialization.
- `codex-rs/core/src/windows_sandbox.rs`
  - Windows sandbox levels (restricted token / elevated) and setup gating.
- `codex-rs/core/src/safety.rs`
  - patch safety classification against writable roots + policy.
- `codex-rs/core/src/exec_policy.rs`
  - command policy rules engine, explicit forbidden/prompt/allow decisions,
  - controlled prefix-rule amendments + banned broad prefix suggestions.
- `codex-rs/core/src/tools/sandboxing.rs`
  - approval cache semantics and policy-driven approval requirements.

### 3) Command Safety Classification

- `codex-rs/shell-command/src/command_safety/is_safe_command.rs`
  - conservative safe-command allowlisting, read-only git subcommand checks.
- `codex-rs/shell-command/src/command_safety/is_dangerous_command.rs`
  - dangerous command detection (`rm -f/-rf`, shell expansions, etc.).
- `codex-rs/shell-command/src/command_safety/windows_safe_commands.rs`
- `codex-rs/shell-command/src/command_safety/windows_dangerous_commands.rs`
  - deeper Windows-specific PowerShell/CMD parsing and dangerous pattern detection.

### 4) Managed Network Enforcement

- `codex-rs/network-proxy/README.md`
- `codex-rs/network-proxy/src/config.rs`
- `codex-rs/network-proxy/src/network_policy.rs`
- `codex-rs/network-proxy/src/policy.rs`
  - local HTTP/SOCKS/admin proxy with allowlist/denylist policy,
  - safe bind clamping to loopback by default,
  - limited/full network modes,
  - local/private network and unix-socket controls.
- `codex-rs/core/src/tools/network_approval.rs`
  - per-attempt host approval lifecycle and deferred denial handling.
- `codex-rs/core/src/network_policy_decision.rs`
  - explicit policy denial reason classification.

### 5) Robust Exec Runtime Primitives

- `codex-rs/core/src/unified_exec/process_manager.rs`
- `codex-rs/core/src/unified_exec/process.rs`
- `codex-rs/core/src/unified_exec/async_watcher.rs`
  - persistent interactive sessions (`exec_command` + `write_stdin`),
  - output streaming + bounded collection,
  - process count limits and pruning strategy,
  - cancellation-aware watchers, deferred network denial termination.
- `codex-rs/utils/pty/src/process_group.rs`
  - process-group setup/kill helpers,
  - Linux parent-death signal hardening (`PR_SET_PDEATHSIG`).
- `codex-rs/rmcp-client/tests/process_group_cleanup.rs`
  - regression coverage for process-group cleanup behavior.

### 6) Transport/Streaming Resilience

- `codex-rs/codex-client/src/retry.rs`
  - retry policy abstraction (`429`, `5xx`, transport errors) with exponential backoff + jitter.
- `codex-rs/codex-client/src/sse.rs`
  - SSE idle-timeout error handling.
- `codex-rs/codex-api/src/sse/responses.rs`
  - stream parsing with explicit incomplete/failed/completed semantics.
- `codex-rs/core/src/model_provider_info.rs`
  - configurable retry budgets + stream idle timeout with bounded caps.
- `codex-rs/core/src/client.rs`
  - session-scoped websocket-to-HTTP fallback path.

### 7) Durable State, Observability, And Secrets

- `codex-rs/state/src/runtime.rs`
  - SQLite WAL mode, busy timeout, migrations, backfill lease ownership model.
- `codex-rs/state/src/log_db.rs`
  - async batched log ingestion and retention cleanup.
- `codex-rs/otel/README.md`
  - OTEL traces/logs/metrics support and explicit shutdown semantics.
- `codex-rs/secrets/src/lib.rs`
- `codex-rs/secrets/src/local.rs`
- `codex-rs/keyring-store/src/lib.rs`
  - local encrypted secret file + keyring-backed key management + atomic writes.
- `codex-rs/utils/readiness/src/lib.rs`
  - tokenized readiness gating primitive for async components.

### 8) Native Runtime Feature Capabilities

- `codex-rs/protocol/src/request_user_input.rs` (structured user input tool contract)
- `codex-rs/protocol/src/plan_tool.rs` (`update_plan` contract)
- `codex-rs/protocol/src/dynamic_tools.rs` (dynamic tool registration/call response contracts)

## Hivemind Native Current Baseline (Compared)

Observed in:

- `src/native/mod.rs`
- `src/native/adapter.rs`
- `src/native/tool_engine.rs`
- `src/storage/event_store.rs`

Current strengths:

- deterministic native loop FSM + explicit error taxonomy,
- typed native tool contracts with JSON-schema validation,
- scope-aware file/repo/execution checks,
- command allow/deny policy hooks,
- runtime trace capture and rich event model,
- graph query gate/integrity checks in native tool path.

Primary gaps versus codex-rs:

1. No pre-main process hardening layer.
2. No cross-platform sandbox orchestration equivalent (Seatbelt/Landlock/Windows sandbox).
3. No approval-policy engine (prompt/forbid/allow matrix with cached approvals and amendments).
4. No managed network proxy + host-approval lifecycle.
5. No persistent PTY/unified exec runtime (session IDs, stdin continuation, pruning/watchers).
6. Run-command timeout is post-exec elapsed check, not enforced kill-timeout.
7. OpenRouter native client has no policy-driven retries/backoff/fallback transport.
8. No secure secrets subsystem equivalent (keyring + encrypted local store).
9. Native runtime still lacks structured parity features like `request_user_input`, `update_plan`, dynamic tool lifecycle contracts.

## Prioritized Import Set For Phase 4.5

### P0 (must-have hardening)

1. Process startup hardening baseline.
2. Approval + sandbox policy orchestration.
3. Managed network policy enforcement.
4. Enforced command timeout/cancellation and process-group cleanup.
5. Transport retries + idle-timeout handling.

### P1 (production robustness)

1. Persistent unified exec sessions with output streaming and process pruning.
2. Durable runtime state WAL/migration + lease semantics for background jobs.
3. Structured telemetry parity for retries/fallback/sandbox/approval decisions.

### P2 (native feature parity)

1. `request_user_input` tool flow.
2. `update_plan` tool flow.
3. dynamic tool registration/call contract.

## Suggested Success Metrics

- `0` silent policy bypasses in sandbox/approval/network paths (every denial/approval emits attributable events).
- `p95` command-start-to-first-output latency tracked for persistent exec sessions.
- deterministic replay stability preserved for all new hardening events.
- explicit retry/fallback counters available per provider and transport.
- command-timeout enforcement proves termination (not only elapsed measurement).

## Recommendation

Proceed with a dedicated Phase 4.5 focused on runtime hardening first, then native feature parity. Use the attached roadmap (`ops/roadmap/phase-4.5-native-runtime-hardening.md`) as the execution plan.
