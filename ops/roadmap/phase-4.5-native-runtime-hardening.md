# Phase 4.5: Native Runtime Hardening And Robustness

**Goal:** Bring Hivemind native runtime from functional correctness to production-grade runtime hardening by importing proven patterns from `@codex-main/codex-rs` while preserving event determinism and governance guarantees.

**Input study:** `ops/reports/phase-4.5-codex-rs-runtime-hardening-study-2026-02-28.md`.

> **Principles 1, 2, 3, 8, 9, 15:** Observability as truth, fail loud, reliability over cleverness, absolute attribution, mandatory checks, no hidden magic.

---

## Sprint 52: Host And Process Hardening Baseline

**Goal:** Add fail-fast host/process hardening before native runtime startup.

### 52.1 Pre-main hardening layer
- [x] Introduce pre-main hardening module for native binaries:
  - [x] disable core dumps on supported UNIX platforms
  - [x] deny debugger attach where supported
  - [x] clear dangerous loader env vars (`LD_*`, `DYLD_*`)
- [x] Add explicit startup failure codes and structured error events

### 52.2 Runtime environment hardening
- [x] Add protected runtime env construction policy for all command/tool execution:
  - [x] allow controlled inherit modes (`all`/`core`/`none`)
  - [x] default deny sensitive env patterns (`*KEY*`, `*SECRET*`, `*TOKEN*`) unless explicitly overridden
  - [x] deterministic env overlays with explicit provenance
- [x] Add inherited env filtering for reserved internal prefixes

### 52.3 Operational hardening tests
- [x] Add unit/integration tests for env sanitization and startup hardening guards
- [x] Add failure-path tests proving fail-fast behavior when hardening setup fails

### 52.4 Exit Criteria
- [x] Native runtime startup applies hardening checks before runtime loop begins
- [x] Environment leakage controls are deterministic and test-covered
- [x] Hardening failures are explicit, attributable, and non-silent

---

## Sprint 53: Sandbox And Approval Orchestrator

**Goal:** Add policy-grade sandbox and approval orchestration around native tool execution.

### 53.1 Sandbox policy model
- [x] Introduce explicit sandbox policy contract:
  - [x] `read-only`
  - [x] `workspace-write` with writable roots + read-only overlays
  - [x] `danger-full-access`
  - [x] external/host sandbox passthrough mode
- [x] Add platform-aware sandbox selection hooks and policy tagging in events

### 53.2 Approval policy model
- [x] Add explicit approval modes:
  - [x] `never`
  - [x] `on-failure`
  - [x] `on-request`
  - [x] `unless-trusted`
- [x] Add approval cache semantics (`approved_for_session`) with explicit review decisions

### 53.3 Exec policy and dangerous-command protection
- [x] Introduce rules-backed exec policy manager for `run_command`/native exec surfaces
- [x] Add safe/dangerous command analyzers (UNIX + Windows-aware)
- [x] Add bounded prefix-rule amendments with banned broad-prefix protections

### 53.4 Exit Criteria
- [x] Every command/tool execution path evaluates sandbox + approval policy explicitly
- [x] Dangerous command and policy conflicts fail deterministically with actionable errors
- [x] Approval outcomes are cached and event-observable

---

## Sprint 54: Managed Network Policy And Host Approvals

**Goal:** Enforce network behavior through explicit proxy policy and interactive host approvals.

### 54.1 Managed network proxy
- [x] Introduce local managed network proxy mode for native runtime execution:
  - [x] HTTP proxy listener
  - [x] optional SOCKS5 listener
  - [x] local admin control endpoint
- [x] Enforce safe bind defaults (loopback clamp unless explicit dangerous override)

### 54.2 Host/domain policy engine
- [x] Add allowlist/denylist host policy with deny-wins precedence
- [x] Add local/private address protection controls
- [x] Add limited network mode with method restrictions

### 54.3 Network approval lifecycle
- [x] Add per-attempt host/protocol approval flow (immediate + deferred modes)
- [x] Add denial reason taxonomy and explicit failure projection
- [x] Add deferred denial watchers that terminate running command sessions when policy/user denial lands

### 54.4 Exit Criteria
- [x] Native network egress is governed by explicit, testable policy
- [x] Host approvals are attributable and replay-visible
- [x] Network-denied operations fail closed, not open

---

## Sprint 55: Unified Exec Sessions And Runtime Cancellation Safety

**Goal:** Introduce persistent interactive command sessions with robust lifecycle management.

### 55.1 Unified exec contract
- [x] Add native interactive command APIs:
  - [x] `exec_command` (spawn + optional initial output capture)
  - [x] `write_stdin` (resume existing process session)
- [x] Add stable process/session IDs with collision-safe allocation

### 55.2 Process lifecycle hardening
- [x] Enforce process-group isolation and termination semantics
- [x] Add cancellation-token propagation and trailing-output grace handling
- [x] Add deterministic output chunking bounds and truncation metadata

### 55.3 Capacity and cleanup controls
- [x] Add open-session caps and pruning strategy (prefer exited/LRU outside protected recent set)
- [x] Add runtime warnings as session counts approach cap
- [x] Ensure full cleanup on turn abort/runtime shutdown

### 55.4 Exit Criteria
- [x] Long-lived interactive command sessions are safe, bounded, and observable
- [x] Aborts and shutdowns terminate process groups reliably
- [x] Session-cap/pruning behavior is deterministic and tested

---

## Sprint 56: Transport Resilience, Retry, And Fallback

**Goal:** Harden model transport behavior under transient failures and degraded network conditions.

### 56.1 Retry policy framework
- [x] Add provider-level retry policy fields:
  - [x] max attempts (bounded)
  - [x] base delay
  - [x] retry on 429/5xx/transport toggles
- [x] Implement exponential backoff with jitter for retryable failures

### 56.2 Streaming robustness
- [x] Add streaming idle-timeout controls with explicit stream-failed classification
- [x] Add structured handling for incomplete/failed stream terminal events
- [x] Emit retry delay and retryable classification in runtime events

### 56.3 Fallback transport strategy
- [x] Add explicit fallback transport path (e.g., websocket to HTTP) when configured transport becomes unhealthy
- [x] Persist per-session fallback activation state and telemetry
- [x] Ensure fallback behavior is visible in runtime/report projections

### 56.4 Exit Criteria
- [x] Transient model transport failures recover under bounded retries
- [x] Idle/disconnect failures are explicit and attributable
- [x] Fallback transport activation is deterministic and observable

---

## Sprint 57: Durable Runtime State, Secure Secrets, And Readiness Gates

**Goal:** Strengthen runtime durability and sensitive-data handling for native operations.

### 57.1 Runtime state durability
- [x] Introduce dedicated native runtime state DB with:
  - [x] WAL mode + busy timeout
  - [x] versioned migrations
  - [x] lease/heartbeat ownership for background jobs
- [x] Add batched async runtime log ingestion and retention cleanup

### 57.2 Secret handling hardening
- [x] Add native secrets manager:
  - [x] encrypted local store
  - [x] keyring-backed key material
  - [x] atomic write/replace semantics
  - [x] in-memory wipe for temporary key buffers where applicable

### 57.3 Component readiness gates
- [x] Add tokenized readiness flags for asynchronous runtime dependencies (proxy/services/background workers)
- [x] Enforce startup sequencing on readiness before enabling task execution

### 57.4 Exit Criteria
- [x] Native runtime state and logs survive restart/replay with deterministic recovery behavior
- [x] Secret material handling is hardened at rest and during write/update operations
- [x] Runtime components expose explicit readiness transitions

---

## Sprint 58: Native Feature Parity Lift (`request_user_input`, `update_plan`, dynamic tools)

**Goal:** Import high-value native runtime capabilities required for robust operator workflows.

### 58.1 `request_user_input` native flow
- [ ] Add structured native `request_user_input` tool contract
- [ ] Add CLI/app-server interaction flow for multi-question response capture
- [ ] Persist answers and link them to invocation/turn provenance

### 58.2 `update_plan` native flow
- [ ] Add `update_plan` native tool contract with explicit step/status model
- [ ] Add projection/report support for plan timeline state across attempts
- [ ] Enforce deterministic plan mutation rules (single in-progress item guard)

### 58.3 Dynamic tools lifecycle
- [ ] Add dynamic tool registration contract and per-thread/attempt association
- [ ] Add dynamic tool call request/response event family with schema validation
- [ ] Add replay-safe storage/reconstruction for dynamic tool definitions and outputs

### 58.4 Exit Criteria
- [ ] Native runtime supports structured human input and plan updates with full provenance
- [ ] Dynamic tools are typed, validated, and replay-safe
- [ ] Capability lift does not regress existing Phase 4 native invariants

---

## Phase 4.5 Completion Gates

- [ ] Sprint 52-58 implementation and manual test reports are present under `@hivemind-test`
- [ ] Native runtime hardening controls are represented in config/docs/runbooks
- [ ] Event/replay determinism remains intact with new hardening events
- [ ] Compatibility with external adapter fallback paths remains non-regressed
- [ ] `ops/reports` contains phase closeout hardening validation summary

---

## Phase 4.5 Principle Checkpoints

After each sprint, verify:

1. **Fail Loud:** Did every hardening failure path emit explicit typed errors?
2. **Determinism:** Are new policy/approval/network/tool events replay-stable?
3. **No Hidden Escalation:** Are all sandbox/network/policy escalations explicit and attributable?
4. **Operator Clarity:** Can runtime decisions be inspected from events/reports without inference?
5. **Bounded Recovery:** Are retries/fallbacks bounded and reason-tagged?
