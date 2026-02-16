# Phase 3 Testing Report - 2026-02-16

## Executive Summary

Running hivemind phase 3 flows with codex adapter. Documenting bugs and DX issues encountered.

---

## Setup Issues

### DX Issue #1: Invalid Model Name Silently Fails

**Location:** `ops/automation/phase-3/bootstrap_phase3_flows.sh:14`

**Problem:** The default `WORKER_MODEL="gpt-5.3-codex-high"` is not a valid codex model. The valid model is `gpt-5.3-codex`. Runtime fails with exit code 1, but no clear error message indicates the model name is invalid.

**Workaround:** Manually update flow runtime config:
```bash
hivemind flow runtime-set <flow-id> --role worker --adapter codex --model gpt-5.3-codex ...
```

**Suggested Fix:** 
1. Validate model names at configuration time
2. Provide better error output when runtime exits with non-zero code
3. Bootstrap script should use valid default model

---

### DX Issue #2: Flow Runtime Not Inherited from Project

**Location:** Flow creation / runtime configuration

**Problem:** When flows are created via `bootstrap_phase3_flows.sh`, the runtime configuration set at the project level is not properly inherited by the flows. Each flow has to have its runtime manually configured.

**Observed Behavior:**
- Project runtime set with `gpt-5.3-codex-high` (invalid model)
- Flows created from project still use the invalid model
- After updating project runtime to valid model, flows still use old invalid model

**Suggested Fix:** Either:
1. Flows should inherit project runtime at creation time
2. Or there should be a command to sync all flow runtimes from project defaults

---

### DX Issue #3: Interactive Mode Deprecated Without Clear Migration

**Location:** `flow tick --interactive`

**Problem:** Running `flow tick --interactive` returns error:
```
Error: [user:interactive_mode_deprecated] Interactive mode is deprecated and no longer supported
```

But there's no guidance on what to use instead or how to migrate.

**Suggested Fix:** 
1. Update documentation to remove interactive mode references
2. Provide migration guide if there's an alternative

---

### DX Issue #4: Codex Binary Not Auto-Detected

**Location:** Bootstrap script / runtime setup

**Problem:** The bootstrap script requires `CODEX_BIN` to be set or `codex` in PATH, but after installing via npm, the binary is in `~/.npm-global/bin/codex`, not automatically in PATH.

**Workaround:**
```bash
export PATH="$HOME/.npm-global/bin:$PATH"
# or
export CODEX_BIN="$HOME/.npm-global/bin/codex"
```

**Suggested Fix:** Bootstrap script or hivemind should check common npm global installation paths.

---

## Runtime Execution Observations

### Working Features

1. **Worktree Isolation:** Git worktrees are correctly created per task
2. **Event Sourcing:** Events are properly recorded and queryable
3. **Task State Machine:** Task states transition correctly (pending → ready → running → success/failed)
4. **Flow Dependencies:** Flows correctly wait for dependencies (launch gate → sprint 34)
5. **Runtime Output:** `runtime_output_chunk` events capture runtime output

---

### DX Issue #5: Model Name Format Inconsistency Between Adapters

**Location:** Runtime configuration across adapters

**Problem:** Different adapters require different model name formats:
- Codex: `gpt-5.3-codex` (no prefix)
- OpenCode: `opencode/minimax-m2.5-free` (provider/model format)

This is confusing and not documented clearly. Setting `minimax-m2.5` for OpenCode fails with:
```
ProviderModelNotFoundError: ProviderModelNotFoundError
providerID: "minimax-m2.5"
```

**Suggested Fix:** 
1. Document model name format for each adapter
2. Add `hivemind runtime models <adapter>` command to list valid models
3. Validate model format per adapter at configuration time

---

## Bugs Found

### Bug #1: Model Configuration Not Validated

**Severity:** MEDIUM

**Problem:** Invalid model names are accepted at configuration time, only failing at runtime with a non-zero exit code that doesn't explain the root cause.

**Reproduction:**
1. Set model to invalid value: `gpt-5.3-codex-high`
2. Run flow
3. Runtime exits with code 1, no helpful error message

**Expected:** Validation error at configuration time, or at minimum, clear error message indicating invalid model.

---

### Bug #2: Flow Restart Issues

**Severity:** MEDIUM

**Problem:** `flow restart` command fails with "Graph already used by an active flow" even when the flow is aborted. Multiple flows get created for the same graph causing conflicts.

**Workaround:** Create new graphs and flows instead of restarting.

**Suggested Fix:** Clean up stale flow references before restart, or allow restart on aborted flows.

---

## Test Progress

| Component | Status | Notes |
|-----------|--------|-------|
| Bootstrap Script | ✅ Works | After model fix |
| Launch Gate Flow | ✅ Completed | Simple checkpoint task |
| Sprint 34 Flow | 🔄 Running | With opencode/minimax-m2.5-free |
| Codex Integration | ✅ Works | With valid model name |
| OpenCode Integration | ✅ Works | Requires provider/model format |

---

## Recommendations

1. **Priority 1:** Add model validation to runtime configuration commands
2. **Priority 1:** Update bootstrap script to use valid default model
3. **Priority 2:** Make flow runtime inherit from project at creation time
4. **Priority 2:** Add `hivemind runtime validate` command
5. **Priority 2:** Document model name format per adapter
6. **Priority 3:** Better error messages for runtime failures
7. **Priority 3:** Fix flow restart for aborted flows

---

*Report generated during phase 3 testing session*
