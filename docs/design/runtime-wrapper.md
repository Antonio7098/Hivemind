---
title: Runtime Wrapper
description: Runtime wrapper mechanics
order: 6
---

# Hivemind — Runtime Wrapper Mechanics

> **Principle 12:** Maximum modularity, minimum lock-in.
> **Principle 3:** Reliability over cleverness.

This document specifies the **concrete mechanics** of wrapper-based runtime adapters. It defines exactly how Hivemind launches, monitors, and observes external CLI runtimes like Claude Code, Codex CLI, and others.

This is an implementation guide for Sprint 1 runtime integration.

---

## 1. Wrapper Architecture

### 1.1 Definition

A **wrapper adapter** invokes an external CLI runtime as a subprocess and observes its effects.

```
┌─────────────────────────────────────────────┐
│                 Hivemind                     │
│  ┌─────────────────────────────────────┐    │
│  │         Runtime Adapter              │    │
│  │  ┌─────────────────────────────┐    │    │
│  │  │   Subprocess (CLI Runtime)   │    │    │
│  │  │   e.g., claude-code          │    │    │
│  │  └─────────────────────────────┘    │    │
│  └─────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
```

### 1.2 Adapter Responsibilities

The adapter is responsible for:
- Subprocess lifecycle management
- Environment preparation
- Output capture
- Side-effect observation
- Error translation

The adapter is **not** responsible for:
- Prompt construction (runtime owns this)
- Model selection (runtime owns this)
- Tool implementation (runtime owns this)

---

## 2. Adapter Lifecycle

### 2.1 Lifecycle Sprints

```
Initialize → Prepare → Execute → Observe → Terminate → Report
```

Each sprint has defined inputs, outputs, and failure modes.

---

## 3. Sprint: Initialize

### 3.1 Purpose

Validate that the runtime is available and functional.

### 3.2 Operations

1. **Check Runtime Availability**
   - Verify CLI binary exists
   - Verify CLI binary is executable
   - Verify minimum version (if applicable)

2. **Check Runtime Health**
   - Run health check command (if available)
   - Verify authentication (if required)

3. **Load Configuration**
   - Read adapter-specific configuration
   - Validate required fields

### 3.3 Outputs

- AdapterInitialized event (success)
- AdapterInitializationFailed error (failure)

### 3.4 Failure Handling

Initialization failure is **non-recoverable**. TaskFlow cannot proceed with a broken adapter.

---

## 4. Sprint: Prepare

### 4.1 Purpose

Set up the execution environment for a specific task attempt.

### 4.2 Operations

1. **Create Worktree**
   - `git worktree add <path> <base_revision>`
   - Set as working directory for subprocess

2. **Prepare Environment**
   ```
   HIVEMIND_TASK_ID=<task_id>
   HIVEMIND_ATTEMPT_ID=<attempt_id>
   HIVEMIND_PROJECT_ID=<project_id>
   HIVEMIND_WORKTREE=<worktree_path>
   ```
   - Remove sensitive environment variables
   - Set resource limits

3. **Record Baseline**
   - Snapshot filesystem state (file list + hashes)
   - Record git HEAD

4. **Prepare Task Input**
   - Format task description
   - Include context documents
   - Include prior attempt feedback (if retry)

### 4.3 Outputs

- PrepareCompleted event
- Baseline snapshot stored

### 4.4 Failure Handling

Preparation failure → attempt fails before execution.

---

## 5. Sprint: Execute

### 5.1 Purpose

Launch the runtime and let it work.

### 5.2 Subprocess Launch

```python
# Conceptual, not literal implementation
process = subprocess.Popen(
    [runtime_binary, *runtime_args],
    cwd=worktree_path,
    env=prepared_env,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
```

### 5.3 Input Delivery

Task input is delivered via:
- **stdin** (preferred for most CLIs)
- **Command-line arguments** (if CLI requires it)
- **File** (if CLI requires file input)

Delivery method is adapter-specific.

### 5.4 Output Capture

During execution:
- Stream stdout to capture buffer
- Stream stderr to capture buffer
- Emit RuntimeOutputChunk events periodically

### 5.5 Timeout Enforcement

```
task_timeout = task.timeout or project.default_timeout or global_default
```

If timeout exceeded:
- Send SIGTERM to process
- Wait grace period
- Send SIGKILL if still running
- Emit RuntimeTimeout error

### 5.6 Process Monitoring

Monitor for:
- Normal exit (exit code 0)
- Error exit (exit code != 0)
- Crash (signal termination)
- Hang (no output for extended period)

### 5.7 Outputs

- RuntimeStarted event (on launch)
- RuntimeOutputChunk events (during execution)
- RuntimeExited event (on completion)

---

### 5.8 Interactive Execution (PTY-Backed Session Mode)

Some runtimes are inherently interactive: they prompt for follow-up input, confirmations, or iterative guidance while they run.

Hivemind may support an optional **interactive execution mode** for wrapper adapters.

**Invariant:** interactive mode is an IO transport. It must not enable behavior that cannot be achieved via non-interactive CLI execution.

In interactive mode, the adapter:
- Launches the runtime inside a pseudo-terminal (PTY)
- Streams PTY output to the caller while still emitting `RuntimeOutputChunk` events
- Accepts user-provided input lines and forwards them to the PTY
- Emits explicit events for every user input and interruption

#### 5.8.1 Session Loop

At a high level:

1. Read bytes from the PTY
2. Emit `RuntimeOutputChunk` with the output content
3. Write output to the CLI (TTY) so the user can see it
4. Read user input (line-based)
5. Emit `RuntimeInputProvided`
6. Write the input line to the PTY (with newline)

Output labeling ("Agent:" vs "Tool:") is **best-effort presentation** only. Hivemind must not depend on parsing correctness.

#### 5.8.2 Interrupt Semantics

Ctrl+C is not a crash.

In interactive mode, Ctrl+C results in:

- Emit `RuntimeInterrupted`
- Terminate the runtime deterministically (same termination policy as timeouts: graceful signal, then kill)
- Emit `RuntimeTerminated`

---

## 6. Sprint: Observe

### 6.1 Purpose

Determine what the runtime actually did.

### 6.2 Filesystem Observation

1. **Compute Changes**
   ```
   for each file in worktree:
     if file not in baseline:
       → created
     elif hash(file) != baseline_hash:
       → modified

   for each file in baseline:
     if file not in worktree:
       → deleted
   ```

2. **Collect Diffs**
   - For each modified file: compute unified diff
   - For created files: full content as diff
   - For deleted files: record deletion

### 6.3 Git Observation

1. **Check Commits**
   - `git log baseline_head..HEAD`
   - Record any commits created by runtime

2. **Check Branches**
   - `git branch --list`
   - Detect any new branches

3. **Check Uncommitted**
   - `git status --porcelain`
   - Record staged and unstaged changes

### 6.4 Output Parsing

Attempt to extract structured information from runtime output:
- Tool invocations (if logged)
- Error messages
- Completion status

Output parsing is **best-effort**. Hivemind never depends on parsing success.

### 6.5 Outputs

- FileModified events (per changed file)
- DiffComputed event (aggregate diff)
- CheckpointCommitCreated event (if commits exist)

---

## 7. Sprint: Terminate

### 7.1 Purpose

Clean up subprocess resources.

### 7.2 Operations

1. **Ensure Process Terminated**
   - Verify process has exited
   - Force kill if still running

2. **Release Resources**
   - Close file handles
   - Release any locks

3. **Preserve Artifacts**
   - Do not delete worktree yet
   - Artifacts needed for verification

### 7.3 Outputs

- RuntimeTerminated event

---

## 8. Sprint: Report

### 8.1 Purpose

Produce a structured execution report for TaskFlow.

### 8.2 Report Structure

```
ExecutionReport:
  attempt_id: string
  task_id: string

  # Process info
  exit_code: int
  exit_signal: int | null
  duration_ms: int

  # Output
  stdout: string (truncated if large)
  stderr: string (truncated if large)

  # Changes
  files_created: [string]
  files_modified: [string]
  files_deleted: [string]
  commits_created: [string]

  # Diffs
  diff_summary: string

  # Errors
  errors: [Error]
```

### 8.3 Report Uses

The report is used by:
- Scope verification (check changes against scope)
- Agent verification (evaluate correctness)
- Event emission (record what happened)
- Retry logic (provide context for next attempt)

---

## 9. Error Translation

Adapter translates runtime-specific errors to Hivemind errors.

### 9.1 Exit Code Mapping

| Exit Code | Interpretation |
|-----------|----------------|
| 0 | Success (pending verification) |
| 1-127 | Runtime error (check stderr) |
| 128+ | Signal termination (128 + signal) |

### 9.2 Common Errors

| Condition | Hivemind Error |
|-----------|----------------|
| Process crashed | RUNTIME_CRASHED |
| Timeout exceeded | RUNTIME_TIMEOUT |
| Binary not found | RUNTIME_CONNECTION_FAILED |
| Auth failure | RUNTIME_CONNECTION_FAILED |
| Malformed output | RUNTIME_OUTPUT_MALFORMED |

Production hardening notes:

- Failure handling is centralized in the registry and emits `runtime_error_classified` for
  every runtime failure path (initialize/prepare/execute/nonzero exit/checkpoint gating).
- Recovery intent is emitted via `runtime_recovery_scheduled` when automatic retry/fallback
  is selected.
- Stdout/stderr are retained for failed execution reports so rate-limit/auth signals can be
  classified even for wrapped runtimes that return exit code `0` but emit fatal errors on stderr.

### 9.3 Error Context

All runtime errors include:
- Exit code
- Stderr content (truncated)
- Duration
- Worktree path

---

## 10. Adapter Configuration

### 10.1 Per-Adapter Settings

```yaml
adapters:
  claude-code:
    binary: "claude"
    args: ["--headless", "--no-confirm"]
    timeout_default: 300
    max_output_size: 1048576
    env_passthrough:
      - ANTHROPIC_API_KEY
    health_check: "claude --version"
```

### 10.2 Per-Task Overrides

Tasks may override:
- Timeout
- Max output size
- Additional arguments

### 10.3 Configuration Validation

On adapter initialization:
- Validate all required fields present
- Validate binary exists and is executable
- Validate health check passes

---

## 11. Supported Runtimes (Sprint 1)

### 11.1 Claude Code

```
Binary: claude
Input: stdin (prompt)
Output: stdout (response + tool logs)
Notes: Use --headless for non-interactive mode
```

### 11.2 Codex CLI

```
Binary: codex
Input: stdin or --prompt flag
Output: stdout
Notes: Requires OPENAI_API_KEY
```

### 11.3 Kilo

```
Binary: kilo
Input: stdin
Output: stdout
Notes: OpenCode-compatible wrapper; supports OpenCode-style model identifiers (for example `opencode/kimi-k2.5-free`)
```

### 11.4 OpenCode

```
Binary: opencode
Input: stdin
Output: stdout
Notes: Configurable backend
```

### 11.5 Adding New Runtimes

New runtimes require:
- Adapter configuration entry
- Input delivery method
- Output parsing hints (optional)
- Health check command

No code changes required for basic support.

---

## 12. Observability

### 12.1 Events Emitted

| Event | When |
|-------|------|
| RuntimeStarted | Process launched |
| RuntimeOutputChunk | Periodically during execution |
| RuntimeInputProvided | User input forwarded to runtime (interactive mode only) |
| RuntimeInterrupted | User interrupted runtime (interactive mode only) |
| RuntimeExited | Process terminated |
| RuntimeTerminated | Cleanup complete |
| RuntimeErrorClassified | Runtime failure normalized into code/category/recoverability |
| RuntimeRecoveryScheduled | Retry/fallback strategy and backoff recorded |
| FileModified | Per changed file |
| DiffComputed | After observation |
| CheckpointCommitCreated | If runtime committed |

### 12.2 Logging

Adapter logs include:
- Process launch command
- Environment variables (sanitized)
- Timing information
- Resource usage (if available)

### 12.3 Debugging Support

For debugging failed attempts:
- Full stdout/stderr preserved
- Worktree preserved on failure
- Execution report includes all context

---

## 13. Security Considerations

### 13.1 Environment Sanitization

Remove from subprocess environment:
- Hivemind internal secrets
- Credentials for other services
- Sensitive paths

Passthrough only explicitly allowed variables.

### 13.2 Resource Limits

Where possible, enforce:
- CPU time limits
- Memory limits
- File descriptor limits
- Output size limits

### 13.3 Network Isolation

Sprint 1: No network isolation (limitation accepted)
Sprint 2+: Consider network namespace isolation

### 13.4 Trust Model

Hivemind **never trusts** runtime output:
- Output is observational data
- Diffs are computed independently
- State is derived from events, not runtime claims

---

## 14. Invariants

The wrapper adapter guarantees:

- Subprocess runs in isolated worktree
- Subprocess environment is controlled
- Timeout is enforced
- Output is captured
- Side effects are observed
- Errors are translated to Hivemind errors
- Events are emitted for all sprints

Violating these invariants is a SystemError.

---

## 15. Summary

Wrapper adapters provide:

- **Leverage:** Use existing mature CLI tools
- **Speed:** No model integration required
- **Replaceability:** Swap runtimes without code changes
- **Observability:** Full capture of behavior

Limitations accepted:

- **Coarse diffs:** File-level, not edit-level
- **Limited interception:** Cannot prevent violations, only detect
- **Opaque reasoning:** Cannot see chain-of-thought

These limitations are acceptable for Sprint 1. The architecture supports evolution to deeper integration without breaking TaskFlow semantics.
