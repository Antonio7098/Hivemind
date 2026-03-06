# Phase 4.7: Native Harness Context, Session Semantics, And Capability Lift

**Goal:** Bring Hivemind native runtime from a hardened execution substrate to a context-aware, tool-native harness with prompt-safe terminal semantics, structured operator interaction, and replay-safe runtime working memory.

**Input studies:**

- `ops/reports/phase-4.5-codex-rs-runtime-hardening-study-2026-02-28.md`
- `ops/reports/codex-rs-harness-capabilities-exploration-2026-03-06.md`

> **Principles 1, 2, 3, 8, 9, 10, 15:** Observability as truth, fail loud, reliability over cleverness, absolute attribution, mandatory checks, preserve failures, no hidden magic.

**Sequencing note:** The capability-lift work previously tracked as Sprint 58 in Phase 4.5 is intentionally relocated into this phase after the native harness has the context and session foundations needed to support it safely.

**Manual validation rule:** For every sprint in this phase, manual validation must include at least one real-project exercise in `@hivemind-test` where the native harness uses real provider-backed LLM calls to build, extend, debug, or resume work on a real app rather than a toy prompt-only scenario. The same reference project may be evolved across sprints if that produces better continuity and stronger evidence.

---

## Sprint 58: Native Tool Loop And Prompt Assembly Foundation

**Goal:** Refactor the native harness into a real tool-result loop with explicit runtime-local history and deterministic prompt assembly.

### 58.1 Runtime-local turn item model
- [ ] Introduce a native `TurnItem` family for runtime working memory:
  - [ ] user input item
  - [ ] assistant text item
  - [ ] tool call item
  - [ ] tool result item
  - [ ] compacted summary item
- [ ] Distinguish model-visible items from event-only/runtime-only artifacts
- [ ] Tag each item with correlation metadata and stable provenance references

### 58.2 In-loop tool execution
- [ ] Refactor `AgentLoop` so tool calls are executed inside the conversational loop, not as a disconnected post-process
- [ ] Ensure tool results are appended back into runtime-local history before the next model turn
- [ ] Preserve deterministic native event ordering for request/start/completion/failure paths

### 58.3 Prompt assembly contract
- [ ] Build prompts from explicit components:
  - [ ] base runtime instructions
  - [ ] task/retry context
  - [ ] selected runtime-local history items
  - [ ] tool contracts
  - [ ] compacted summaries
- [ ] Emit prompt-delivery hashes and context-manifest metadata for observability
- [ ] Reserve configurable headroom for the active turn and tool results

### 58.4 Validation and replay tests
- [ ] Add deterministic mock-model tests for multi-turn tool/result conversations
- [ ] Add malformed tool-call and partial-history normalization tests
- [ ] Add replay tests proving runtime-local history can be reconstructed from journal/event sources

### 58.5 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 58 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls to build or extend a real app through multiple tool-result turns
- [ ] Capture prompt assembly evidence, tool-call traces, and final build/test outcomes for the exercised app task
- [ ] Publish Sprint 58 manual test report artifact in `@hivemind-test`

### 58.6 Exit Criteria
- [ ] Native harness executes tools as part of the turn loop
- [ ] Prompt assembly is explicit, attributable, and test-covered
- [ ] Runtime working memory is clearly separated from authoritative orchestration state
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 59: Runtime Context Manager, Truncation, And Compaction

**Goal:** Add a Codex-style runtime-local context manager that keeps prompts bounded without losing operator-relevant history.

### 59.1 Context budget accounting
- [ ] Introduce runtime-local context budget policy separate from orchestration-level context-window operations
- [ ] Add approximate byte/token estimation per history item
- [ ] Add configurable prompt headroom and overflow classification

### 59.2 Record-time truncation and normalization
- [ ] Truncate oversized tool outputs at record time rather than only at prompt-construction time
- [ ] Normalize malformed or incomplete call/output sequences before prompt delivery
- [ ] Filter unsupported modalities with explicit loss markers when required

### 59.3 Compaction model
- [ ] Add compacted summary items with source references and summary hashes
- [ ] Add deterministic compaction triggers based on budget pressure and item class
- [ ] Preserve source linkage from summaries back to raw journal/event artifacts

### 59.4 Validation and observability
- [ ] Emit truncation/compaction telemetry in runtime events and reports
- [ ] Add tests for stable compaction decisions under repeated replay
- [ ] Add tests proving prompt-visible history remains within configured bounds

### 59.5 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 59 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls on a sufficiently large app task to trigger non-trivial context growth
- [ ] Validate truncation/compaction explainability while the agent continues building or modifying the real app successfully
- [ ] Publish Sprint 59 manual test report artifact in `@hivemind-test`

### 59.6 Exit Criteria
- [ ] Native runtime can stay within bounded prompt budgets without silent loss
- [ ] Tool output truncation and history compaction are explicit and attributable
- [ ] Compaction remains reconstructable from authoritative artifacts
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 60: Session-Aware Terminal Context And Exec Semantics

**Goal:** Make persistent terminal sessions prompt-safe by separating live transcripts, session working memory, and model-visible context.

### 60.1 Three-layer transcript model
- [ ] Introduce explicit separation between:
  - [ ] live terminal transcript events
  - [ ] session working memory
  - [ ] model-visible context items
- [ ] Ensure raw PTY output streams remain available to UI/observers without being injected verbatim into prompts
- [ ] Add waiting-for-input and still-running status hints for interactive sessions

### 60.2 `exec_command` and `write_stdin` semantics
- [ ] Make `exec_command` return bounded initial output plus session/process metadata
- [ ] Make `write_stdin` return only delta output and lifecycle state, not full transcript replay
- [ ] Surface explicit status fields:
  - [ ] running
  - [ ] exited
  - [ ] waiting_for_input
  - [ ] truncated_output

### 60.3 Session retention and summary checkpoints
- [ ] Add bounded rolling transcript retention (head/tail or equivalent)
- [ ] Add automatic session summary checkpoints for long-lived sessions
- [ ] Add session-cap-aware pruning rules that preserve recent and active sessions

### 60.4 Validation and failure tests
- [ ] Add tests for prompt-safe long-running terminal sessions
- [ ] Add tests proving interactive sessions do not grow unbounded model-visible context
- [ ] Add tests for cancellation, exit, and deferred-denial edge cases after compaction/checkpointing

### 60.5 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 60 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls to build/debug a real app while using long-lived terminal sessions (for example builds, tests, dev servers, or interactive CLIs)
- [ ] Validate that live terminal output remains observable while model-visible terminal context stays bounded and useful across follow-up turns
- [ ] Publish Sprint 60 manual test report artifact in `@hivemind-test`

### 60.6 Exit Criteria
- [ ] Terminal sessions remain interactive and observable without bloating prompt history
- [ ] Model-visible terminal context is bounded, useful, and stateful
- [ ] Session summaries are deterministic and replay-safe
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 61: Native Feature Parity Lift (`request_user_input`, `update_plan`, dynamic tools)

**Goal:** Deliver the high-value native protocol capabilities previously scoped in Phase 4.5 Sprint 58, now on top of the proper context/tooling foundation.

### 61.1 `request_user_input` native flow
- [ ] Add structured native `request_user_input` contract with support for:
  - [ ] multi-question forms
  - [ ] options/choice lists
  - [ ] secret/sensitive answer flags
- [ ] Add CLI/app-server interaction flow for attributed answer capture
- [ ] Persist answers with invocation/turn provenance and append them as runtime-local user-input history items

### 61.2 `update_plan` native flow
- [ ] Add `update_plan` contract with explicit step/status model
- [ ] Add projection/report support for plan timeline state across attempts
- [ ] Enforce deterministic plan mutation rules, including a single in-progress guard where applicable

### 61.3 Dynamic tools lifecycle
- [ ] Add dynamic tool registration contract with per-attempt/per-thread association
- [ ] Add dynamic tool call request/response events with schema validation and provenance
- [ ] Add replay-safe storage and reconstruction for dynamic tool definitions and outputs

### 61.4 Validation and UX coverage
- [ ] Add tests for multi-turn clarification and answer replay
- [ ] Add tests for deterministic plan evolution across retries/resume
- [ ] Add tests proving dynamic tools behave like built-ins once registered

### 61.5 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 61 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls to build or extend a real app and must exercise at least one of:
  - [ ] `request_user_input`
  - [ ] `update_plan`
  - [ ] dynamic tools
- [ ] Validate that operator answers, plan changes, and dynamic-tool outcomes remain attributable and visible in the real app workflow
- [ ] Publish Sprint 61 manual test report artifact in `@hivemind-test`

### 61.6 Exit Criteria
- [ ] Native runtime supports structured human input and plan updates with full provenance
- [ ] Dynamic tools are typed, validated, and replay-safe
- [ ] Capability lift does not regress the prompt/session invariants introduced in Sprints 58-60
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 62: Runtime Turn Journal, Resume, And Session Checkpoints

**Goal:** Add a runtime turn journal that allows the native harness to resume practical working memory without making prompt memory authoritative.

### 62.1 Turn-journal artifact model
- [ ] Add per-invocation journal artifacts covering:
  - [ ] prompt deliveries
  - [ ] tool call/result references
  - [ ] compaction markers
  - [ ] session summary checkpoints
- [ ] Emit stable hashes and versioned schema metadata for journal entries

### 62.2 Resume and reconstruction flow
- [ ] Reconstruct runtime-local history from event + journal sources on resume
- [ ] Restore active session summaries and model-visible terminal state safely
- [ ] Differentiate resumable working memory from authoritative orchestration state

### 62.3 Replay and provenance guards
- [ ] Add replay tests proving journal reconstruction is deterministic
- [ ] Add drift detection when journal artifacts and authoritative events disagree
- [ ] Fail loud on unrecoverable journal corruption or incompatible schema versions

### 62.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 62 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls to make meaningful progress on a real app, is interrupted mid-stream, and then resumes from journal/checkpoint state
- [ ] Validate that the resumed run can continue the real app task with understandable context recovery and no hidden state assumptions
- [ ] Publish Sprint 62 manual test report artifact in `@hivemind-test`

### 62.5 Exit Criteria
- [ ] Native runtime can resume bounded working memory after interruption/restart
- [ ] Resume behavior is observable and attributable
- [ ] Journaling improves operator continuity without weakening event authority
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 63: MCP, Hooks, And External Tool Federation

**Goal:** Add ecosystem-grade extension points after the core harness, context, and session semantics are stable.

### 63.1 Hooks and notifications
- [ ] Add native hook event contracts for at least:
  - [ ] after-agent
  - [ ] after-tool-use
- [ ] Add registry/dispatch support for configured external hooks
- [ ] Keep hook outcomes explicit, bounded, and non-magical in runtime events

### 63.2 MCP integration
- [ ] Add MCP server/connection lifecycle for external tool federation
- [ ] Integrate MCP tools into the native tool contract path with schema validation
- [ ] Add approval/policy handling for MCP tool calls and sanitized result projection

### 63.3 Governance and safety integration
- [ ] Preserve sandbox/approval/provenance controls across hook and MCP boundaries
- [ ] Add explicit operator-facing failure modes for unavailable or denied external tools
- [ ] Add tests for deterministic eventing of external tool/hook decisions

### 63.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 63 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls to build or extend a real app while exercising at least one external integration path (hook or MCP-backed tool)
- [ ] Validate that external integration activity, approvals, and failures remain attributable within the real app workflow
- [ ] Publish Sprint 63 manual test report artifact in `@hivemind-test`

### 63.5 Exit Criteria
- [ ] Hivemind can federate external tools without bypassing native governance controls
- [ ] Hook and MCP activity is attributable and observable
- [ ] External integrations do not regress replay determinism or prompt-bounding guarantees
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Phase 4.7 Completion Gates

- [ ] Sprint 58-63 implementation and manual test reports are present under `@hivemind-test`
- [ ] The capability-lift scope previously tracked in Phase 4.5 Sprint 58 has been fully rehomed to this phase
- [ ] Native harness supports bounded, prompt-safe terminal and tool-result working memory
- [ ] Human input, plan updates, dynamic tools, hooks, and MCP integrations are attributable and replay-visible where enabled
- [ ] Runtime turn-journal and resume behavior preserve Hivemind's event-authority model
- [ ] Each sprint includes at least one real-project, real-app manual validation run backed by real LLM calls
- [ ] `ops/reports` contains a Phase 4.7 closeout validation summary

---

## Phase 4.7 Principle Checkpoints

After each sprint, verify:

1. **Event Authority:** Did runtime working memory remain subordinate to events/journals rather than replacing them?
2. **Fail Loud:** Did truncation, compaction, resume drift, and external-tool failures emit explicit typed outcomes?
3. **Prompt Safety:** Are prompts bounded intentionally rather than by accidental loss?
4. **Operator Clarity:** Can session state, compaction, and human-input decisions be inspected without inference?
5. **No Hidden Magic:** Did hooks, MCP, and dynamic tools remain explicit and attributable?

---

## Follow-On Candidates (Not Required For Phase 4.7)

- [ ] collaborative subagent primitives inside the native harness
- [ ] richer output-schema and reasoning-effort controls
- [ ] advanced per-tool context-retention policies
- [ ] automated summary-quality evaluation for compaction checkpoints