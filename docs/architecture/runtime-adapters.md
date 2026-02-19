---
title: Runtime Adapters
description: Runtime abstraction and adapters
order: 6
---

# Hivemind — Runtime Adapters (runtime-adapters.md)

> **Core principle:** Runtimes are replaceable, non-deterministic executors. Hivemind owns orchestration, state, and truth.

This document defines the **runtime adapter layer** in Hivemind: the contract between Hivemind and execution runtimes (e.g. Claude Code, OpenCode, Codex CLI, and native runtime mode). It explains what Hivemind expects, what it explicitly refuses to depend on, and how the system evolves from wrapper-based runtimes to native runtime foundations without architectural breakage.

This is a **contract and strategy document**, not an SDK specification.

---

## 1. Why Runtime Adapters Exist

LLM execution environments differ wildly in:
- Prompt construction
- Tool invocation
- Memory management
- Safety behavior
- Output structure

If Hivemind couples itself to any one runtime’s internals, it becomes brittle.

**Runtime Adapters exist to isolate this variability.**

Hivemind treats every runtime as:
> a non-deterministic compute oracle with observable side effects.

---

## 2. Responsibilities by Layer

### 2.1 What Hivemind Owns

Hivemind owns:
- TaskGraph and TaskFlow semantics
- State and event model
- Scope enforcement
- Worktrees and branches
- Retry and verification policy
- Merge governance
- Observability

These must never depend on runtime-specific behavior.

---

### 2.2 What Runtimes Own

Runtimes own:
- Model selection
- Prompt assembly
- Internal memory and context windows
- Tool-specific heuristics
- Output phrasing

These are intentionally opaque to Hivemind.

---

## 3. The Runtime Adapter Contract

A **Runtime Adapter** is the only component allowed to interact directly with an execution runtime.

Conceptually, an adapter must support the following lifecycle:

```
Prepare → Execute → Observe → Terminate
```

---

## 4. Required Capabilities (Hard Requirements)

Every runtime adapter **must** provide the following capabilities.

### 4.1 Launch & Termination

- Start a runtime process
- Stop it deterministically
- Detect crashes or abnormal exits

---

### 4.2 Scoped Execution

- Execute within a provided working directory (worktree)
- Respect environment restrictions
- Never escape assigned filesystem scope

---

### 4.3 Observable Output

- Stream or capture stdout/stderr
- Emit structured runtime events
- Attribute outputs to task and attempt

Hivemind does not parse reasoning, only observable output.

---

### 4.4 Side-Effect Observation

- Detect file modifications
- Allow diff computation
- Support checkpoint commit creation

Side effects are the primary signal of work.

---

## 5. Explicit Non-Requirements (By Design)

Runtime adapters **must not require**:

- Access to model context windows
- Access to chain-of-thought
- Deterministic replay at token level
- Semantic understanding of edits

If a runtime cannot provide these, Hivemind still functions correctly.

---

## 6. Wrapper-Based Adapters (Sprint 1)

### 6.1 Description

Wrapper adapters invoke existing CLI-based runtimes as subprocesses.

Examples:
- Claude Code
- Codex CLI
- Kilo
- Gemini CLI
- OpenCode

---

### 6.2 Behavior

Wrapper adapters:
- Launch the CLI
- Pass task instructions
- Allow the runtime to operate freely within scope
- Observe outputs and filesystem changes

Interactive execution is deprecated in the CLI. Adapters may still expose
interactive internals, but `flow tick --interactive` is no longer supported.

- Interactive execution is treated as IO transport (stdin/stdout), not a new orchestration capability
- All interaction is recorded as events
- The UI (if any) is a projection over those events and CLI-accessible capabilities

Undo, retry, and rollback are handled mechanically via git and worktrees.

---

### 6.3 Strengths

- Fast to implement
- Leverages mature tooling
- Low maintenance burden
- Enables early user feedback

---

### 6.4 Limitations (Accepted)

- Coarse-grained diffs
- No semantic edit intent
- Limited live editor UX
- Runtime owns context

These are acceptable tradeoffs in Sprint 1.

---

## 7. Hybrid Interception (Sprint 2)

As Hivemind matures, adapters may incrementally add interception:

- Structured tool call logging
- Patch-level diff extraction
- Scoped command allowlists
- Improved checkpoint granularity
- **Scope enforcement** transitions from detection to prevention

Crucially:
> Hybrid interception must not change TaskFlow semantics.

It only improves observability and enforcement.

In Sprint 1, scope violations are detected post-hoc via diffs. In Sprint 2+, violations are prevented via runtime interception. See `docs/design/scope-enforcement.md` for detailed enforcement mechanics across sprints.

---

## 8. Native Runtime Foundation (Sprint 42 / Phase 4)

### 8.1 What “Native” Means

A native runtime foundation includes:
- Be embedded in Hivemind
- Use provider-agnostic model contracts (`ModelClient`)
- Run an explicit finite-state loop (`init -> think -> act -> done`)
- Enforce deterministic budgets (turn/time/token) with structured error taxonomy
- Support deterministic harnessing for tests (`MockModelClient`)

---

### 8.2 Why Native Is Deferred

Sprint 42 introduces a first native runtime adapter mode (`native`) while preserving existing
external adapters as first-class execution backends.

Current native mode is intentionally minimal:
- deterministic loop contracts and error semantics
- explicit capability reporting via `runtime list` / `runtime health`
- explicit `RuntimeCapabilitiesEvaluated` event emission before `RuntimeStarted`

Richer native execution internals (tool engine, payload capture, replayable turn-level model/tool
events) continue in follow-on Phase 4 sprints.

---

### 8.3 Migration Guarantees

The architecture guarantees:
- TaskFlow does not change
- State and events do not change
- Scope and commit models remain valid

Only the adapter implementation changes.

---

## 9. Adapter Selection & Configuration

Projects may define:
- Default runtime adapter
- Fallback adapters
- Adapter-specific limits

Runtime configuration is abstracted behind Hivemind and not exposed directly.

---

## 10. Supported Adapters

### 10.1 Codex CLI (OpenAI)

The Codex adapter wraps OpenAI's Codex CLI for code execution.

**Installation:**
```bash
npm install -g @openai/codex
```

**Model Selection:**
- `gpt-5.3-codex` - Available with ChatGPT accounts
- `gpt-5.3-codex-high` - Requires API account (not available with ChatGPT accounts)

**Configuration:**
```bash
hivemind project runtime-set "my-project" \
  --role worker \
  --adapter codex \
  --binary-path /path/to/codex \
  --model gpt-5.3-codex \
  --timeout-ms 600000
```

**Default Args:**
The Codex adapter automatically includes:
- `exec` - Non-interactive execution mode
- `--skip-git-repo-check` - Allow execution outside git repos
- `-c reasoning_effort=high` - High reasoning effort for optimal performance

---

### 10.2 OpenCode

The OpenCode adapter supports multiple model providers.

**Installation:**
```bash
npm install -g opencode
```

**Available Models:**
- `opencode/kimi-k2.5-free` - Free tier model
- `opencode/minimax-m2.5-free` - Free tier MiniMax
- `minimax-coding-plan/MiniMax-M2.5` - MiniMax M2.5 for coding tasks
- `anthropic/claude-*` - Claude models
- `google/gemini-*` - Gemini models

**Configuration:**
```bash
# Worker with Kimi
hivemind project runtime-set "my-project" \
  --role worker \
  --adapter opencode \
  --binary-path /path/to/opencode \
  --model opencode/kimi-k2.5-free

# Validator with MiniMax M2.5
hivemind project runtime-set "my-project" \
  --role validator \
  --adapter opencode \
  --binary-path /path/to/opencode \
  --model minimax-coding-plan/MiniMax-M2.5
```

**Listing Available Models:**
```bash
opencode models
```

---

### 10.3 Kilo

Kilo is an OpenCode-compatible adapter with additional capabilities.

**Configuration:**
```bash
hivemind project runtime-set "my-project" \
  --role worker \
  --adapter kilo \
  --binary-path /path/to/kilo \
  --model opencode/kimi-k2.5-free
```

---

### 10.4 Claude Code

The Claude Code adapter wraps Anthropic's Claude CLI.

**Configuration:**
```bash
hivemind project runtime-set "my-project" \
  --role worker \
  --adapter claude-code \
  --binary-path /path/to/claude
```

---

## 11. Runtime Health Checks

Verify runtime configuration:
```bash
# Check project runtime health
hivemind runtime health --project "my-project"

# Check specific role
hivemind runtime health --project "my-project" --role worker
hivemind runtime health --project "my-project" --role validator

# Check task-specific runtime
hivemind runtime health --task <task-id> --role worker
```

---

## 12. Runtime Resolution Precedence

Hivemind resolves runtimes in this order:
1. **Task override** - `task runtime-set` (highest priority)
2. **Flow default** - `flow runtime-set`
3. **Project default** - `project runtime-set`
4. **Global default** - `runtime defaults-set` (fallback)

---

## 13. Failure Semantics

If a runtime:
- Crashes
- Hangs
- Violates scope

The adapter:
- Terminates execution
- Emits failure events
- Allows TaskFlow to retry or escalate

Runtime failure is treated as an execution failure, not a system failure.

### 13.1 First-Class Runtime Error Events

Runtime failures are emitted as explicit runtime-domain events:

- `runtime_error_classified`
  - Contains normalized error code/category/recoverability and rate-limit signal.
- `runtime_recovery_scheduled`
  - Captures retry strategy (`same_runtime_retry` or `fallback_runtime`) plus backoff.

These events are observational (non-state-mutating) and exist for operator visibility,
automation, and downstream diagnostics.

### 13.2 Retry and Fallback Policy

Retries are intentionally narrow:

- Retry is scheduled for transient failures (for example: rate limiting or timeouts).
- Retry may reconfigure the task worker runtime to a backup adapter when available.
- Non-transient runtime failures are failed without automatic retry.

Checkpoint handling remains explicit:

- `checkpoints_incomplete` does not auto-fail the running attempt.
- The attempt stays running so checkpoints can still be completed against the same attempt id.

### 13.3 Output-Aware Classification

Adapters preserve stdout/stderr for failed executions. Hivemind uses this output to classify
rate-limit/auth failures, including wrapped-runtime cases where the subprocess may return
exit code `0` while still printing fatal auth/rate-limit errors to stderr.

---

## 14. Security Considerations

Runtime adapters are a security boundary.

They must:
- Run with least privilege
- Be sandboxed when possible
- Never trust runtime outputs

All safety checks are enforced by Hivemind, not delegated.

---

## 15. Invariants

The runtime adapter layer enforces:

- No runtime-specific state leakage
- No coupling to model internals
- No hidden execution paths

Violating these invariants compromises the system.

---

## 16. Summary

The runtime adapter layer allows Hivemind to:
- Move fast today with wrappers
- Learn from real usage
- Evolve toward native execution
- Preserve architectural integrity

Runtimes are tools. Hivemind is the system.
