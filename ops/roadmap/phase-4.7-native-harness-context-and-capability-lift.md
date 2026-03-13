# Phase 4.7: Native Harness Context, Session Semantics, And Capability Lift

**Goal:** Bring Hivemind native runtime from a hardened execution substrate to a mode-aware, tool-native, context-bounded harness with explicit prompt assembly, GraphCode-backed navigation, prompt-safe session semantics, deterministic task progress signaling, and replay-safe working memory.

**Input studies:**

- `ops/reports/phase-4.5-codex-rs-runtime-hardening-study-2026-02-28.md`
- `ops/reports/codex-rs-harness-capabilities-exploration-2026-03-06.md`

> **Principles 1, 2, 3, 8, 9, 10, 15:** Observability as truth, fail loud, reliability over cleverness, absolute attribution, mandatory checks, preserve failures, no hidden magic.

**Sequencing note:** The capability-lift work previously tracked as Sprint 58 in Phase 4.5 remains intentionally relocated into this phase, but it must land on top of the original 4.7 foundation: native tool loop, bounded runtime-local context, prompt-safe terminal/session handling, and replay-safe journaling.

**Stabilization note:** Correctness and recovery fixes discovered while hardening Sprint 58 behavior (for example checkpoint completion reliability, malformed-output recovery, deterministic history normalization, or compaction bugs that cause non-convergence) may land immediately as fast-follow stabilization work. Do **not** treat those fixes as permission to pull the broader runtime context-manager design forward; lane-aware budgeting, record-time truncation, semantic context-lane policy, and compaction-strategy architecture remain Sprint 59 scope.

**Manual validation rule:** For every sprint in this phase, manual validation must include at least one real-project exercise in `@hivemind-test` where the native harness uses real provider-backed LLM calls to build, extend, debug, or resume work on a real app rather than a toy prompt-only scenario. The same reference project may be evolved across sprints if that produces better continuity and stronger evidence.

## GraphCode Substrate Rule (Mandatory)

- [x] Before implementing any Phase 4.7 GraphCode behavior, read both the Hivemind integration docs **and** the current UCP graph + CodeGraph docs first:
  - [x] `ops/roadmap/phase-3.md` Sprint 37 (`UCP Graph Integration and Snapshot Projection`)
  - [x] `docs/architecture/cli-capabilities.md` graph snapshot/query sections
  - [x] `docs/design/cli-semantics.md` graph snapshot + graph query sections
  - [x] `docs/design/codegraph-db-schema.md`
  - [x] `../unified-content-protocol/docs/ucp-cli/codegraph.md`
  - [x] `../unified-content-protocol/docs/ucp-api/README.md`
  - [x] `../unified-content-protocol/docs/ucp-api/graph-runtime.md`
  - [x] `../unified-content-protocol/docs/ucp-api/codegraph-programmatic.md`
  - [x] `../unified-content-protocol/docs/ucp-api/python-query-tools.md`
- [ ] Import/use the existing UCP graph runtime **and** CodeGraph layers locally; do **not** create a second graph/query/session engine inside Hivemind
- [ ] Hivemind must **not** implement a new generic graph runtime or a new CodeGraph extractor/query engine from scratch; the local UCP Graph + CodeGraph implementation is the sole traversal/extraction substrate for Phase 4.7 work
- [ ] Treat Graph/CodeGraph as a runtime-managed substrate whose lifecycle, policy, attribution, freshness orchestration, and operator UX stay in Hivemind while graph semantics, code semantics, traversal/session behavior, hydration, and persistence continue to come from the locally imported UCP implementation
- [ ] Replace the current static graph snapshot projection (`projects/<project-id>/graph_snapshot.json` and related runtime assumptions) with a runtime-managed GraphCode artifact/session registry; any transitional file projection must be treated as a derivative compatibility artifact, not the new source of truth

---

## Sprint 58: Native Tool Loop, Agent Modes, And Prompt Assembly Foundation

**Goal:** Refactor the native harness into a real tool-result loop with explicit runtime-local history, mode-aware behavior, and deterministic prompt assembly.

### 58.1 Runtime-local turn item model
- [x] Introduce a native `TurnItem` family for runtime working memory:
  - [x] user input item
  - [x] assistant text item
  - [x] tool call item
  - [x] tool result item
  - [x] graph/code-navigation item
  - [x] compacted summary item
- [x] Distinguish model-visible items from event-only/runtime-only artifacts
- [x] Tag each item with correlation metadata and stable provenance references

### 58.2 In-loop tool execution
- [x] Refactor `AgentLoop` so tool calls are executed inside the conversational loop, not as a disconnected post-process
- [x] Ensure tool results are appended back into runtime-local history before the next model turn
- [x] Preserve deterministic native event ordering for request/start/completion/failure paths
- [x] Keep graph/code-navigation inside the same native tool-result loop rather than as hidden prompt preprocessing

### 58.3 Agent mode model
- [x] Introduce `AgentMode` separate from runtime role:
  - [x] `planner`
  - [x] `freeflow`
  - [x] `task_executor`
- [x] Preserve existing `worker` / `validator` runtime-role semantics
- [x] Attach mode provenance to invocation and turn events
- [x] Make allowed tool/capability sets mode-aware

### 58.4 Prompt assembly contract
- [x] Build prompts from explicit components:
  - [x] base runtime instructions
  - [x] mode contract
  - [x] objective state (`task` / `retry` / `checkpoint` / `planner target` / `freeflow goal`)
  - [x] selected runtime-local history items
  - [x] graph/code-navigation session items
  - [x] tool contracts
  - [x] compacted summaries
- [x] Emit prompt-delivery hashes and context-manifest metadata for observability
- [x] Reserve configurable headroom for the active turn and tool results

### 58.5 Validation and replay tests
- [x] Add deterministic mock-model tests for multi-turn tool/result conversations
- [x] Add malformed tool-call and partial-history normalization tests
- [x] Add replay tests proving runtime-local history can be reconstructed from journal/event sources
- [x] Add tests proving mode-specific prompt assembly is deterministic

### 58.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 58 manual checklist under `@hivemind-test`
- [x] Run a real-project validation where Hivemind native harness uses real LLM calls to build or extend a real app through multiple tool-result turns
- [x] Capture prompt assembly evidence, tool-call traces, mode attribution, and final build/test outcomes for the exercised app task
- [x] Publish Sprint 58 manual test report artifact in `@hivemind-test`

### 58.7 Exit Criteria
- [x] Native harness executes tools as part of the turn loop
- [x] Prompt assembly is explicit, attributable, and test-covered
- [x] Agent mode is explicit without breaking runtime-role semantics
- [x] Runtime working memory is clearly separated from authoritative orchestration state
- [x] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 59: Runtime Context Manager, GraphCode Context, Truncation, And Compaction

**Goal:** Add a runtime-local context manager that keeps prompts bounded while making the GraphCode substrate the primary semantic workspace, with CodeGraph leading for repository code tasks and the generic graph runtime available for non-code structured context.

**Detailed design companion:** `ops/roadmap/phase-4.7-graph-first-context-manager-plan-2026-03-11.md`

### 59.1 Context budget accounting
- [ ] Introduce runtime-local context budget policy separate from orchestration-level context-window operations
- [ ] Add approximate byte/token estimation per history item
- [ ] Add configurable prompt headroom and overflow classification
- [ ] Track lane-level budget usage for:
  - [ ] objective state
  - [ ] graph/code-navigation state
  - [ ] recent interaction state
  - [ ] compacted summaries

### 59.2 Record-time truncation and normalization
- [ ] Truncate oversized tool outputs at record time rather than only at prompt-construction time
- [ ] Normalize malformed or incomplete call/output sequences before prompt delivery
- [ ] Filter unsupported modalities with explicit loss markers when required
- [ ] Keep GraphCode navigation outputs bounded and structured

### 59.3 GraphCode-backed context lanes
- [ ] Introduce a runtime-managed GraphCode substrate registry bound to workspace/project identity:
  - [x] substrate kind (`graph` / `codegraph`)
  - [x] storage backend and reference (`json` / `sqlite`, graph key/path)
  - [x] UCP fingerprint / profile metadata
  - [x] repo/worktree provenance
  - [x] extractor/config/runtime versioning
  - [x] freshness state
  - [x] active session references
- [x] Introduce runtime-managed graph/codegraph navigation session state:
  - [x] current focus nodes
  - [x] pinned nodes
  - [x] recent traversals
  - [x] selected working set / session export references
  - [x] hydrated excerpts (CodeGraph)
  - [x] path/explanation artifacts
  - [x] snapshot/fingerprint provenance
  - [x] freshness state
- [x] Use the generic graph runtime for non-code UCP document/workspace context and CodeGraph for repository code tasks
- [x] Prefer GraphCode-backed navigation/hydration over raw code transcript accumulation for code tasks
- [x] Keep GraphCode lifecycle runtime-managed, not model-invoked
- [x] Require implementers to use the locally imported UCP graph runtime / CodeGraph integration and existing docs/contracts before extending behavior
- [x] Keep Hivemind DB focused on authoritative registry metadata, lifecycle attribution, and refresh/rebind state; persist large graph/codegraph payloads through UCP-supported storage backends wherever possible
- [ ] If Hivemind exposes higher-order scripted graph/code queries, model them as bounded wrappers over UCP programmatic graph/codegraph sessions with explicit limits and attributable tool/session references rather than inventing a new Hivemind-specific query DSL
- [x] Replace the existing static graph snapshot file projection as the primary runtime query substrate; if a file projection remains during migration, make the GraphCode registry authoritative and the file derivative only

### 59.4 Compaction model
- [x] Add compacted summary items with source references and summary hashes
- [x] Add deterministic compaction triggers based on budget pressure and item class
- [x] Preserve source linkage from summaries back to raw journal/event artifacts
- [x] Prefer graph/codegraph session summaries over transcript replay, with CodeGraph primary for code-heavy work

### 59.5 Validation and observability
- [x] Emit truncation/compaction telemetry in runtime events and reports
- [x] Add tests for stable compaction decisions under repeated replay
- [x] Add tests proving prompt-visible history remains within configured bounds
- [x] Add tests proving GraphCode context remains attributable, bounded, and freshness-aware
- [x] Add migration tests proving older graph snapshot projections are either upgraded into GraphCode registry records or rejected loudly with actionable remediation
- [x] Add benchmark/eval coverage for canonical GraphCode workflows (for example: entrypoint-to-implementation pathing, likely-test ranking, branch-and-compare, and rank-before-hydrate)

### 59.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 59 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls on a sufficiently large app task to trigger non-trivial context growth
- [ ] Validate truncation/compaction explainability while the agent continues building or modifying the real app successfully
- [ ] Validate that GraphCode-backed context is derived from the local UCP graph/codegraph integration rather than a duplicated Hivemind parser/query layer
- [ ] Validate that the runtime GraphCode registry becomes the source of truth rather than the old `graph_snapshot.json` path
- [ ] Validate at least one benchmark-style GraphCode workflow on a real repo (for example path explanation, test ranking, or rank-before-hydrate) and capture the evidence in the manual report
- [x] Publish Sprint 59 manual test report artifact in `@hivemind-test`

### 59.7 Exit Criteria
- [x] Native runtime can stay within bounded prompt budgets without silent loss
- [x] Tool output truncation and history compaction are explicit and attributable
- [x] CodeGraph is the primary semantic code-context lane for repository code tasks, while the generic graph runtime remains available for non-code structured context
- [x] Compaction remains reconstructable from authoritative artifacts
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 60: Session-Aware Terminal Context, Exec Semantics, And Edit-Driven GraphCode Freshness

**Goal:** Make persistent terminal sessions prompt-safe while keeping GraphCode freshness aligned with edits and substrate rebinds.

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

### 60.4 Edit-driven GraphCode freshness
- [ ] Detect successful repo mutations from native tools and runtime filesystem observations
- [ ] Mark affected graph/codegraph session regions and substrate artifacts as stale after edits
- [ ] Prefer UCP incremental rebuild/refresh paths where available and fall back to full rebuild loudly and observably when they are not
- [ ] Surface explicit `stale` / `refreshing` / `fresh` status to both model and operator
- [ ] Keep runtime-managed graph refresh logic subordinate to the local UCP-backed extraction/query contract
- [ ] Persist dirty-path/invalidation records and refresh job lifecycle in DB with explicit trigger attribution
- [ ] Rebind active navigation sessions to refreshed GraphCode artifact/session IDs when refresh succeeds and selectors still resolve
- [ ] Surface an explicit degraded/recovery-needed state when automatic rebind is impossible

### 60.5 Validation and failure tests
- [ ] Add tests for prompt-safe long-running terminal sessions
- [ ] Add tests proving interactive sessions do not grow unbounded model-visible context
- [ ] Add tests for cancellation, exit, and deferred-denial edge cases after compaction/checkpointing
- [ ] Add tests for post-edit GraphCode freshness transitions, including successful rebind and explicit degraded-state cases

### 60.6 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 60 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls to build/debug a real app while using long-lived terminal sessions (for example builds, tests, dev servers, or interactive CLIs)
- [ ] Validate that live terminal output remains observable while model-visible terminal context stays bounded and useful across follow-up turns
- [ ] Validate that edits cause visible GraphCode freshness transitions and session rebind/degraded outcomes without requiring the model to manage graph lifecycle manually
- [ ] Publish Sprint 60 manual test report artifact in `@hivemind-test`

### 60.7 Exit Criteria
- [ ] Terminal sessions remain interactive and observable without bloating prompt history
- [ ] Model-visible terminal context is bounded, useful, and stateful
- [ ] Session summaries are deterministic and replay-safe
- [ ] GraphCode freshness follows edits automatically and explicitly
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 61: Native Capability Lift By Mode

**Goal:** Deliver the high-value native protocol capabilities on top of the proper tool-loop, context, and session foundation.

### 61.1 `request_user_input` native flow
- [ ] Add structured native `request_user_input` contract with support for:
  - [ ] multi-question forms
  - [ ] options/choice lists
  - [ ] secret/sensitive answer flags
- [ ] Add CLI/app-server interaction flow for attributed answer capture
- [ ] Persist answers with invocation/turn provenance and append them as runtime-local user-input history items

### 61.2 Planner control-plane mutations
- [ ] Add planner-mode native plan/taskflow mutation contract
- [ ] Add explicit step/status model for planner-visible plan updates
- [ ] Add projection/report support for plan timeline state across attempts
- [ ] Enforce deterministic plan mutation rules, including a single in-progress guard where applicable
- [ ] Keep planner-only orchestration mutations unavailable in `task_executor` mode

### 61.3 Task Executor deterministic progress signals
- [ ] Add executor-safe native operations for:
  - [ ] `complete_checkpoint`
  - [ ] `report_blocker`
  - [ ] `propose_change`
- [ ] Reuse the existing checkpoint completion registry path rather than shelling out through CLI
- [ ] Add typed events and projections for blocker/change-proposal lifecycle
- [ ] Ensure all progress signals are attributable to attempt + task + mode
- [ ] Do **not** allow `task_executor` mode to mutate plan state directly

### 61.4 Freeflow and dynamic tools lifecycle
- [ ] Allow `freeflow` mode to use the same native tool loop with broader project-scoped edit/navigation behavior, including bounded graph/codegraph exploration where policy allows
- [ ] Keep freeflow out of formal task-graph mutation unless explicitly invoking planner/control-plane operations
- [ ] Add dynamic tool registration contract with per-attempt/per-thread association where enabled
- [ ] Add dynamic tool call request/response events with schema validation and provenance
- [ ] Add replay-safe storage and reconstruction for dynamic tool definitions and outputs
- [ ] Make dynamic tool availability mode- and policy-aware
- [ ] Ensure scripted graph/codegraph tool wrappers remain bounded, attributable, and tied to runtime-managed GraphCode session references

### 61.5 Validation and UX coverage
- [ ] Add tests for multi-turn clarification and answer replay
- [ ] Add tests for deterministic plan evolution in planner mode
- [ ] Add tests proving `task_executor` cannot mutate plans
- [ ] Add tests proving dynamic tools behave like built-ins once registered
- [ ] Add tests for checkpoint/blocker/proposal replay and visibility
- [ ] Add tests proving dynamic tools and freeflow capabilities cannot bypass GraphCode substrate bounds or mode policy

### 61.6 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 61 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls to build or extend a real app and must exercise at least one of:
  - [ ] `request_user_input`
  - [ ] planner-mode plan mutation
  - [ ] checkpoint/blocker/proposal flow in task execution
  - [ ] dynamic tools
- [ ] Validate that operator answers, planner changes, executor progress signals, and dynamic-tool outcomes remain attributable and visible in the real app workflow
- [ ] Publish Sprint 61 manual test report artifact in `@hivemind-test`

### 61.7 Exit Criteria
- [ ] Native runtime supports structured human input with full provenance
- [ ] Planner can mutate plans/tasks deterministically
- [ ] Task Executor can signal progress without mutating the plan
- [ ] Dynamic tools are typed, validated, and replay-safe where enabled
- [ ] Capability lift does not regress the prompt/session invariants or GraphCode substrate bounds introduced in Sprints 58-60
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 62: Runtime Turn Journal, Resume, And GraphCode Recovery

**Goal:** Add a runtime turn journal that allows the native harness to resume practical working memory without making prompt memory authoritative.

### 62.1 Turn-journal artifact model
- [ ] Add per-invocation journal artifacts covering:
  - [ ] prompt deliveries
  - [ ] tool call/result references
  - [ ] graph/code-navigation session checkpoints
  - [ ] compaction markers
  - [ ] session summary checkpoints
- [ ] Emit stable hashes and versioned schema metadata for journal entries
- [ ] Journal GraphCode artifact IDs, substrate kind, backend/storage references, fingerprint/profile metadata, and navigation session IDs rather than relying on prompt-visible context to reconstruct graph state

### 62.2 Resume and reconstruction flow
- [ ] Reconstruct runtime-local history from event + journal sources on resume
- [ ] Restore active GraphCode navigation state when artifact/session/fingerprint state is still valid
- [ ] Restore active session summaries and model-visible terminal state safely
- [ ] Restore mode-specific objective state safely (`planner`, `freeflow`, `task_executor`)
- [ ] Differentiate resumable working memory from authoritative orchestration state
- [ ] Restore GraphCode navigation state from runtime registry records and substrate storage references, not from legacy static snapshot-file assumptions

### 62.3 Replay and provenance guards
- [ ] Add replay tests proving journal reconstruction is deterministic
- [ ] Add drift detection when journal artifacts and authoritative events disagree
- [ ] Fail loud on unrecoverable journal corruption or incompatible schema versions
- [ ] Fail loud on incompatible or stale-unrecoverable GraphCode session/artifact state

### 62.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 62 manual checklist under `@hivemind-test`
- [ ] Run a real-project validation where Hivemind native harness uses real LLM calls to make meaningful progress on a real app, is interrupted mid-stream, and then resumes from journal/checkpoint state
- [ ] Validate that the resumed run can continue the real app task with understandable context recovery, restored mode state, and no hidden state assumptions
- [ ] Publish Sprint 62 manual test report artifact in `@hivemind-test`

### 62.5 Exit Criteria
- [ ] Native runtime can resume bounded working memory after interruption/restart
- [ ] Resume behavior is observable and attributable
- [ ] Journaling improves operator continuity without weakening event authority
- [ ] GraphCode recovery is explicit and safe
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 63: MCP, Hooks, And External Tool Federation

**Goal:** Add ecosystem-grade extension points after the core harness, context, session semantics, and native capability model are stable.

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
- [ ] Ensure external tools do not bypass mode contracts, GraphCode substrate rules, freshness/rebind policy, or executor determinism

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
- [ ] `planner`, `freeflow`, and `task_executor` modes are explicit and attributable without breaking `worker` / `validator` runtime-role semantics
- [ ] The GraphCode substrate is runtime-managed, imported locally from the existing UCP graph runtime + CodeGraph integration, and exposed to agents as bounded navigation/hydration rather than a duplicated Hivemind-owned graph/parser lifecycle
- [ ] A runtime-managed GraphCode artifact/session registry has replaced the old static graph snapshot projection as the primary runtime substrate, with any remaining file artifacts clearly marked transitional or derivative
- [ ] Human input, planner updates, executor checkpoint/blocker/proposal signals, dynamic tools, hooks, and MCP integrations are attributable and replay-visible where enabled
- [ ] Runtime turn-journal and resume behavior preserve Hivemind's event-authority model
- [ ] Each sprint includes at least one real-project, real-app manual validation run backed by real LLM calls
- [ ] `ops/reports` contains a Phase 4.7 closeout validation summary

---

## Phase 4.7 Principle Checkpoints

After each sprint, verify:

1. **Event Authority:** Did runtime working memory remain subordinate to events/journals rather than replacing them?
2. **Fail Loud:** Did truncation, compaction, resume drift, freshness changes, and external-tool failures emit explicit typed outcomes?
3. **Prompt Safety:** Are prompts bounded intentionally rather than by accidental loss?
4. **Operator Clarity:** Can session state, compaction, mode behavior, and human-input decisions be inspected without inference?
5. **No Hidden Magic:** Did hooks, MCP, dynamic tools, and GraphCode behavior remain explicit and attributable?
6. **No Duplicate GraphCode Engine:** Did implementation continue to rely on the locally imported UCP graph runtime + CodeGraph integration rather than re-implementing graph traversal, storage, extraction, or query logic inside Hivemind?

---

## Follow-On Candidates (Not Required For Phase 4.7)

- [ ] collaborative subagent primitives inside the native harness
- [ ] richer output-schema and reasoning-effort controls
- [ ] advanced per-tool context-retention policies
- [ ] automated summary-quality evaluation for compaction checkpoints