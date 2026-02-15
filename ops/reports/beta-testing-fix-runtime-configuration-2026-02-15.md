# Beta Testing Fix Report: Runtime Configuration

> Fix for Codex and OpenCode runtime adapter configuration issues discovered during Phase 3 Sprint 34 execution.

---

## 1. Metadata

| Field | Value |
|-------|-------|
| Fix Title | Runtime Adapter Configuration (Codex/OpenCode) |
| Date | 2026-02-15 |
| Fix Window | 2026-02-15 18:00 → 20:45 UTC |
| Branch | `fix/runtime-configuration` |
| Owner(s) | Antonio |
| Related Issues / Reports | Phase 3 Sprint 34 execution |

---

## 2. Source Reports & Phases

| Artifact / Information | Origin Phase or Report | Reference / Link |
|------------------------|------------------------|------------------|
| Sprint 34 execution failure | Phase 3 Sprint 34 | `ops/roadmap/phase-3.md` |
| Runtime adapter docs | Architecture | `docs/architecture/runtime-adapters.md` |
| CLI semantics | Design | `docs/design/cli-semantics.md` |

---

## 3. Problem Statement

- **Symptoms:**
  1. Codex runtime exiting immediately with exit code 1 and no output
  2. Error: "The 'gpt-5.3-codex-high' model is not supported when using Codex with a ChatGPT account"
  3. OpenCode model `opencode/minimax-m2.5` not found
  4. Codex sandbox mode `read-only` blocking all file writes

- **Impact:** All Sprint 34 tasks failed to execute. No code changes could be made by the agent.

- **Detection Source:** Manual execution during Phase 3 Sprint 34 kickoff

---

## 4. Root Cause Summary

- **Primary Cause:**
  1. Incorrect model name for Codex (`gpt-5.3-codex-high` requires API account, not ChatGPT)
  2. Missing `reasoning_effort=high` config for optimal Codex performance
  3. Incorrect model name for OpenCode validator (`opencode/minimax-m2.5` should be `minimax-coding-plan/MiniMax-M2.5`)
  4. Default Codex sandbox mode `workspace-write` is too restrictive for Hivemind's worktree-based isolation

- **Contributing Factors:**
  - Lack of runtime configuration documentation
  - No validation of model availability before execution
  - Sandbox mode not documented as requiring bypass

- **Why Now:** Phase 3 execution exposed these issues when attempting real task execution

---

## 5. Fix Scope

| Area | Changes |
|------|---------|
| CLI | `src/adapters/codex.rs` - Added `--dangerously-bypass-approvals-and-sandbox` and `-c reasoning_effort=high` to default args |
| Core/Registry | None |
| Docs | `docs/architecture/runtime-adapters.md` - Added Section 10-12 with supported adapters, models, and health checks |
| Docs | `docs/overview/install.md` - Added runtime adapter prerequisites |
| Docs | `docs/overview/quickstart.md` - Added Codex and MiniMax configuration examples |
| Tooling/Tests | None |
| Other | None |

---

## 6. Validation & Regression Matrix

| Check | Result | Evidence / Command |
|-------|--------|---------------------|
| cargo fmt --all --check | PASS | `cargo fmt --all --check` |
| cargo clippy --all-targets --all-features -- -D warnings | PASS | `cargo clippy --all-targets --all-features -- -D warnings` |
| cargo test --all-features | PASS | `cargo test --all-features` |
| Runtime health check | PASS | `hivemind runtime health --project <id> --role worker` |
| Manual QA: Codex execution | PASS | Test flow with "Say hello" completed successfully |
| Manual QA: Checkpoint completion | PASS | `hivemind checkpoint complete` succeeded |

---

## 7. Observability & Documentation Updates

- Added runtime adapter configuration guide to `docs/architecture/runtime-adapters.md`
- Documented supported models for Codex and OpenCode
- Added `--dangerously-bypass-approvals-and-sandbox` justification
- Updated quickstart with Codex and MiniMax examples
- Added runtime health check commands to documentation

---

## 8. Residual Risk & Follow-ups

| Risk / Debt | Mitigation Plan | Owner | Due |
|-------------|-----------------|-------|-----|
| No `flow restart` command | Add `hivemind flow restart <flow-id>` command | TBD | Phase 3 |
| No model availability validation | Add pre-flight check for model availability | TBD | Phase 3 |
| Cargo build caching issues | Document `cargo clean` workaround | Antonio | Done |
| Sandbox bypass required | Document security implications | Antonio | Done |

---

## 9. Outstanding Actions

- [x] Changelog updated (N/A - fix on branch)
- [x] PR prepared: `fix/runtime-configuration` pushed to origin
- [ ] Merge PR to main
- [ ] Resume Sprint 34 execution

---

## 10. DX Issues Identified

### Issue #1: No `flow restart` command for aborted flows
**Severity:** Medium  
**Description:** An aborted flow cannot be restarted - must create a new flow.  
**Workaround:** Create new flow from same graph.  
**Recommendation:** Add `hivemind flow restart <flow-id>` command.

### Issue #2: Codex sandbox mode too restrictive by default
**Severity:** High (blocking)  
**Description:** Default sandbox `workspace-write` blocks writes to worktree.  
**Fix Applied:** Added `--dangerously-bypass-approvals-and-sandbox` to default args.

### Issue #3: Codex model name confusion
**Severity:** Medium  
**Description:** `gpt-5.3-codex-high` requires API account, `gpt-5.3-codex` works with ChatGPT.  
**Fix Applied:** Documentation updated with model selection guidance.

### Issue #4: Build caching issues
**Severity:** Low  
**Description:** Cargo build caching prevented new binary from being produced.  
**Workaround:** `cargo clean && cargo build --release`.

---

## 11. Commits

| SHA | Message |
|-----|---------|
| `0b6e3c5` | fix(runtime): add reasoning_effort=high to codex adapter defaults |
| `8c610c0` | docs: add runtime adapter configuration guide |
| `c9e267d` | fix(runtime): use --dangerously-bypass-approvals-and-sandbox for Codex |

---

## 12. What's Working

| Feature | Status |
|---------|--------|
| Flow lifecycle (create/start/tick) | Working |
| Task execution state machine | Working |
| Worktree isolation | Working |
| Event streaming | Working |
| Checkpoint system | Working |
| Runtime health checks | Working |
| Codex adapter | Working (with fix) |
| OpenCode adapter | Working |
| Inter-flow dependencies | Working |
| Task dependency resolution | Working |
| PR merge flow | Working |

---

_Report generated: 2026-02-15_
