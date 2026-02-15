## Sprint 26: Concurrency Governance

**Goal:** Controlled, policy-driven parallel task execution.

> **User Stories 5 & 6:** Parallel scoped agents and scope conflict handling.

**Execution Note (2026-02-14):** With per-task worktree isolation already in place, Sprint 26 focuses on scheduler policy, concurrency limits, and conflict observability (not filesystem safety primitives).

### 26.1 Execution Concurrency
- [x] Allow multiple task attempts to be dispatched in one `flow tick`
- [x] Enforce per-project concurrency policy via runtime config (`max_parallel_tasks`)
- [x] Enforce optional global cap via `HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL`
- [x] Preserve runtime/worktree isolation guarantees for concurrently scheduled tasks

### 26.2 Scope-Aware Scheduling
- [x] Evaluate task scope compatibility before dispatching each candidate
- [x] Hard conflicts are serialized and emitted as `ScopeConflictDetected` + `TaskSchedulingDeferred`
- [x] Soft conflicts are permitted in parallel but emitted as `ScopeConflictDetected` warnings
- [x] Compatible scopes are dispatched in parallel up to the effective concurrency limit

### 26.3 CLI and API Discoverability
- [x] Add `hivemind flow tick --max-parallel <n>` override
- [x] Add `hivemind project runtime-set --max-parallel-tasks <n>` policy control
- [x] Surface new scheduling telemetry through CLI/server event labeling

### 26.4 Validation and Exit Criteria
- [x] Unit coverage verifies compatible parallel dispatch and hard/soft conflict handling
- [x] Scheduler behavior is deterministic under concurrency limits and scope policy
- [x] Manual `hivemind-test` validation of edge cases (parallel dispatch, conflict serialization, global cap)
- [x] User Stories 5 & 6 are achievable through CLI-first workflow

---

## Sprint 27: Multi-Repo Support

**Goal:** TaskFlows spanning multiple repositories.

> **User Story 7:** Multi-repo project execution.

### 27.1 Multi-Repo Worktrees
- [x] Worktree per repo per task
- [x] Agent receives all repo paths

### 27.2 Multi-Repo Scope
- [x] Scope evaluated per repo
- [x] Violation in any repo fails task

### 27.3 Multi-Repo Merge
- [x] Atomicity at TaskFlow level (default)
- [x] All repos merge or none merge
- [x] Partial failure handling

### 27.4 Exit Criteria
- [x] Tasks can modify multiple repos
- [x] Scope enforced per repo
- [x] Merge is atomic across repos
- [x] User Story 7, 8 achievable

---

## Sprint 28: Additional Runtime Adapters

**Goal:** Support multiple runtimes.

### 28.1 Codex CLI Adapter
- [x] Implement CodexAdapter
- [x] Test integration

### 28.2 Claude Code Adapter
- [x] Implement ClaudeCodeAdapter
- [x] Test integration

### 28.3 Kilo Code Adapter
- [x] Implement KiloAdapter
- [x] Test integration

### 28.4 Runtime Selection
- [x] Per-project default runtime
- [x] Per-task override
- [x] `hivemind runtime list`
- [x] `hivemind runtime health`

### 28.5 Exit Criteria
- [x] Multiple runtimes work
- [x] Runtime selection works
- [x] Same TaskFlow semantics across runtimes

---

## Sprint 29: Event Streaming & Observability

**Goal:** Real-time observability.

### 29.1 Event Stream Command
- [x] `hivemind events stream` (real-time)
- [x] Filters: --flow, --task, --since

### 29.2 Event Query Command
- [x] `hivemind events list` (historical)
- [x] `hivemind events inspect <event-id>`
- [x] Correlation filtering
- [x] Time range filtering

### 29.3 Exit Criteria
- [x] Real-time event stream works
- [x] Historical queries work
- [x] Events are the complete record

---

## Sprint 30: Automation & Scheduling

**Goal:** Triggered TaskFlows.

> **User Story 9:** Scheduled TaskFlows.

### 30.1 Automation Events
- [ ] `AutomationCreated`
- [ ] `AutomationTriggered`

### 30.2 Automation Commands
- [ ] `hivemind automation create <flow> --schedule <cron>`
- [ ] `hivemind automation list`
- [ ] `hivemind automation disable`
- [ ] `hivemind automation trigger` (manual)

### 30.3 Exit Criteria
- [ ] Scheduled triggers work
- [ ] Manual triggers work
- [ ] Automations are observable
- [ ] User Story 9 achievable

---

## Sprint 31: Agent Meta-Operation

**Goal:** Agents can operate Hivemind itself.

### 31.1 CLI as API
- [ ] All commands scriptable
- [ ] JSON output reliable
- [ ] Exit codes semantic

### 31.2 Attribution
- [ ] Agent actions attributed in events
- [ ] Scope applies to meta-operations

### 31.3 Exit Criteria
- [ ] Agents can invoke CLI commands
- [ ] Actions are audited
- [ ] Meta-orchestration possible

---

## Sprint 32: UI Foundation (Optional)

**Goal:** UI is a projection over CLI-accessible state.

### 32.1 Views
- [ ] Project overview
- [ ] Task list / Kanban
- [ ] TaskFlow document view
- [ ] Dependency graph
- [ ] Diff and verification views

### 32.2 Principles
- [ ] UI reads via CLI/events only
- [ ] UI does not modify state directly
- [ ] UI is secondary to CLI

### 32.3 Exit Criteria
- [ ] UI reflects CLI state accurately
- [ ] No UI-only features

---

## Sprint 33: Production Hardening

**Goal:** Production-ready quality.

### 33.1 Error Handling
- [ ] All error paths tested
- [ ] Recovery hints useful
- [ ] No silent failures

### 33.2 Performance
- [ ] Event replay scales to 10k+ events
- [ ] CLI response time < 100ms for reads
- [ ] Parallel execution efficient

### 33.3 Documentation
- [ ] CLI help complete
- [ ] Architecture docs match implementation
- [ ] User guide written

### 33.4 Exit Criteria
- [ ] All user stories achievable
- [ ] All principles upheld
- [ ] Ready for real use

---

## Summary: User Story Coverage

| User Story | Sprint |
|------------|--------|
| US1: Simple Todo Tracking | Sprint 6 |
| US2: Manual Agent Assistance | Sprint 25 |
| US3: Structured TaskFlow | Sprint 25 |
| US4: Verification Failure & Retry | Sprint 19 |
| US5: Parallel Scoped Agents | Sprint 26 |
| US6: Scope Conflict Handling | Sprint 26 |
| US7: Multi-Repo Project Execution | Sprint 27 |
| US8: Shared Repo Across Projects | Sprint 27 |
| US9: Automation | Sprint 30 |
| US10: Pause & Resume | Sprint 9 |

---

## Principle Checkpoints

After each sprint, verify:

1. **Observability:** All state derived from events?
2. **Fail Fast:** Errors surface immediately?
3. **Reliability:** Deterministic, no cleverness?
4. **Errors:** Structured, attributable, actionable?
5. **Structure:** Explicit boundaries and FSMs?
6. **SOLID:** Single responsibility, clear interfaces?
7. **CLI-First:** Feature works via CLI?
8. **Absolute Observability:** No silent side effects?
9. **Automated Checks:** Verification gates progress?
10. **Failures First-Class:** Failures preserved and inspectable?
11. **Incremental:** Foundations proven before expansion?
12. **Modularity:** Replaceable components?
13. **Control:** Power users can drop down a level?
14. **Human Authority:** Humans decide at boundaries?
15. **No Magic:** Everything has a reason and trail?

---

**This roadmap is the contract. Follow it.**
