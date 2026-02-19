## Sprint 42: Native Runtime Contracts and Harness Foundation

**Goal:** Introduce a native runtime foundation without changing current TaskFlow semantics or breaking existing external adapters.

**Prerequisite:** Complete `ops/roadmap/phase-3.5-interlude-db-migration.md` so Phase 4 event/query growth lands on a DB-backed storage engine.

> **Principles 1, 3, 11, 12:** Observability is truth, reliability over cleverness, incremental foundations, modularity and replaceability.

### 42.1 Runtime Contract Layer
- [x] Define native runtime contracts under `src/native/` (or equivalent):
  - [x] `ModelClient` trait for provider-agnostic inference
  - [x] `AgentLoop` finite state machine (`init -> think -> act -> done`)
  - [x] `NativeRuntimeConfig` (turn limits, timeout budgets, token budgets)
  - [x] Structured `NativeRuntimeError` taxonomy aligned with existing `HivemindError`
- [x] Keep external adapters (`opencode`, `codex`, `claude-code`, `kilo`) as first-class, unchanged fallback paths

### 42.2 Runtime Selection Integration
- [x] Extend runtime selection to accept a `native` adapter mode while preserving precedence:
  - [x] task override -> flow default -> project default -> global default
- [x] Add explicit capability reporting to `hivemind runtime list` and `hivemind runtime health`
- [x] Emit explicit runtime capability events for adapter/runtime selection decisions

### 42.3 Null Model Determinism Harness
- [x] Implement a `MockModelClient` with scripted deterministic turn outputs
- [x] Add unit tests for every `AgentLoop` state transition and invalid transition path
- [x] Add malformed-model-output tests that fail loud with structured recovery hints

### 42.4 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 42 manual checklist under `@hivemind-test` (`/home/antonio/programming/Hivemind/hivemind-test`)
- [x] Validate runtime selection (`native` vs existing adapters) and unchanged flow semantics
- [x] Publish Sprint 42 manual test report artifact in `@hivemind-test`

### 42.5 Exit Criteria
- [x] Native contracts compile and are test-covered
- [x] Existing adapter-based execution remains non-regressed
- [x] Runtime selection behavior is explicit and observable
- [x] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 43: Native Runtime Event Model and Blob Storage

**Goal:** Make every native runtime action replayable through explicit event contracts and hash-addressed payload storage.

> **Principles 1, 8, 10, 15:** Event authority, absolute observability, failures preserved, no hidden magic.

### 43.1 Native Runtime Event Family
- [x] Introduce native runtime event payloads:
  - [x] `AgentInvocationStarted`
  - [x] `AgentTurnStarted`
  - [x] `ModelRequestPrepared`
  - [x] `ModelResponseReceived`
  - [x] `ToolCallRequested`
  - [x] `ToolCallStarted`
  - [x] `ToolCallCompleted`
  - [x] `ToolCallFailed`
  - [x] `AgentTurnCompleted`
  - [x] `AgentInvocationCompleted`
- [x] Include correlation IDs (`project`, `graph`, `flow`, `task`, `attempt`) on all native events

### 43.2 Payload and Provenance Strategy
- [x] Add hash-addressed blob storage for large prompt/response payloads:
  - [x] canonical path under `~/.hivemind/blobs/`
  - [x] digest + byte-size + media-type metadata in events
- [x] Default to metadata + hashes in event payloads; enable full payload capture by explicit config
- [x] Persist provider/model metadata and runtime version provenance for each invocation

### 43.3 Replay Determinism Proof
- [x] Add replay tests proving deterministic state reconstruction from native events + blob references
- [x] Add idempotence tests for projection rebuild under repeated replay
- [x] Ensure native events do not alter legacy flow/task semantics when feature-disabled

### 43.4 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 43 manual checklist under `@hivemind-test`
- [x] Validate native event trail completeness for successful and failed invocations
- [x] Validate blob reference integrity and recovery behavior
- [x] Publish Sprint 43 manual test report artifact in `@hivemind-test`

### 43.5 Exit Criteria
- [x] Native runtime emits complete, attributable event trails
- [x] Replay reproduces identical native runtime projection state
- [x] Large payload strategy is observable and configurable
- [x] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 44: Tool Engine v1 (Typed, Safe, Replayable)

**Goal:** Build a native tool-calling subsystem with strict schema validation, deterministic recording, and scope-aware safety boundaries.

> **Principles 2, 4, 5, 9:** Fail loud, explicit taxonomy, structured execution, mandatory checks.

### 44.1 Tool Registry and Contracts
- [x] Define `Tool` contract including:
  - [x] name + version
  - [x] JSON schema input/output contract
  - [x] required scope and permissions
  - [x] timeout/cancellation envelope
- [x] Add registry-based dispatch with deterministic tool lookup
- [x] Reject unknown tool names and invalid schemas with structured errors

### 44.2 Built-In Tools (Minimal Set)
- [x] Implement minimal built-in tool set:
  - [x] `read_file`
  - [x] `list_files`
  - [x] `write_file` (scope-gated)
  - [x] `run_command` (deny-by-default policy)
  - [x] `git_status` and `git_diff` (read-only)
- [x] Ensure every tool execution emits request/start/completion/failure events

### 44.3 Safety and Policy Enforcement
- [x] Wire tool execution to existing scope enforcement model (`Scope`, repo access, execution limits)
- [x] Add command allowlist/denylist policy config with explicit violation eventing
- [x] Make policy violations fail attempts deterministically (no silent auto-retry)

### 44.4 Test and Benchmark Coverage
- [x] Unit tests for schema validation, permission gates, and failure classification
- [x] Property tests for deterministic tool call replay
- [x] Performance baselines for tool dispatch overhead and validation latency

### 44.5 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 44 manual checklist under `@hivemind-test`
- [x] Validate allowed tool success paths and denied tool violation paths
- [x] Validate replay and inspectability of tool traces
- [x] Publish Sprint 44 manual test report artifact in `@hivemind-test`

### 44.6 Exit Criteria
- [x] Tool engine is typed, schema-validated, and fully event-observable
- [x] Scope/policy violations are explicit and attributable
- [x] Replay reconstructs tool outcomes deterministically
- [x] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 45: Active Context Window and Deterministic Prompt Assembly

**Goal:** Upgrade current attempt context assembly into a mutable, operation-driven context window with reproducible prompt manifests.

> **Principles 1, 3, 8, 13:** Observable context state, deterministic behavior, no hidden context, abstraction with control.

### 45.1 Context Window Model
- [ ] Introduce attempt-scoped `ContextWindow` with explicit operations:
  - [ ] `add`
  - [ ] `remove`
  - [ ] `expand`
  - [ ] `prune`
  - [ ] `snapshot`
- [ ] Preserve existing context manifest contract as baseline compatibility layer

### 45.2 Context Operation Events
- [ ] Add event payloads:
  - [ ] `ContextWindowCreated`
  - [ ] `ContextOpApplied`
  - [ ] `ContextWindowSnapshotCreated`
- [ ] Include operation provenance (actor/runtime/tool, reason, before/after hash)

### 45.3 Deterministic Prompt Manifest v2
- [ ] Define prompt manifest v2 with stable section ordering and canonical serialization
- [ ] Enforce deterministic hashing for:
  - [ ] context window state
  - [ ] rendered prompt payload
  - [ ] delivered runtime input
- [ ] Maintain compatibility bridge for `AttemptContextAssembled`/`AttemptContextDelivered`

### 45.4 Budget and Constraint Policies
- [ ] Replace ad hoc truncation with explicit budget policy object (global + per-section)
- [ ] Enforce deduplication and max-depth constraints for context expansion
- [ ] Emit explicit truncation/pruning reasons as events

### 45.5 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 45 manual checklist under `@hivemind-test`
- [ ] Validate deterministic context evolution across repeated runs
- [ ] Validate context op inspection and truncation explainability
- [ ] Publish Sprint 45 manual test report artifact in `@hivemind-test`

### 45.6 Exit Criteria
- [ ] Context window is mutable, bounded, and replayable
- [ ] Prompt assembly is deterministic and hash-stable
- [ ] Legacy attempt context inspection remains functional
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 46: Graph Query Runtime Integration (UCP Substrate)

**Goal:** Make UCP graph snapshot data directly queryable by runtime/scout tools with deterministic, bounded query semantics.

> **User Story 12 continuation:** Architecture governance and context selection should come from observable code facts.

### 46.1 Graph Query Contract
- [ ] Implement graph query primitives over existing snapshot artifacts:
  - [ ] `neighbors(node)`
  - [ ] `dependents(node)`
  - [ ] `subgraph(seed, depth, edge_types)`
  - [ ] `filter(type/path/partition)`
- [ ] Guarantee deterministic ordering of query responses

### 46.2 Runtime and Tool Integration
- [ ] Expose graph queries through native tool engine with explicit bounds
- [ ] Add CLI inspection commands for graph query debugging
- [ ] Record query cost and result-size telemetry in events

### 46.3 Staleness and Integrity Gates
- [ ] Reuse existing snapshot staleness checks before serving runtime graph queries
- [ ] Hard-fail on stale/missing snapshot with actionable recovery hint (`hivemind graph snapshot refresh`)
- [ ] Preserve UCP canonical fingerprint verification in query pipeline

### 46.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 46 manual checklist under `@hivemind-test`
- [ ] Validate query determinism, bounds, and stale snapshot handling
- [ ] Publish Sprint 46 manual test report artifact in `@hivemind-test`

### 46.5 Exit Criteria
- [ ] Runtime can query graph substrate via bounded tools
- [ ] Query behavior is deterministic and observable
- [ ] Integrity/staleness gates prevent invalid graph context
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 47: Scout Agent (Deterministic Context Builder)

**Goal:** Add a restricted scout stage that builds initial active context without editing code or mutating repositories.

> **Principles 5, 7, 14:** Structured roles, CLI-first control, human governance boundaries retained.

### 47.1 Scout Role and Contract
- [ ] Define `Scout` role in runtime planning/execution contracts
- [ ] Inputs: task intent, constitution, graph facts, attached docs/skills/prompts
- [ ] Outputs: only context operations and rationale metadata

### 47.2 Restricted Execution Profile
- [ ] Restrict scout to read-only tools (graph/document/read-file)
- [ ] Prohibit `write_file`, `run_command`, and git mutation tools in scout mode
- [ ] Emit explicit policy violation events for prohibited scout operations

### 47.3 Worker Handoff Semantics
- [ ] Persist scout-generated context snapshot as worker input baseline
- [ ] Ensure worker context hash provenance links back to scout outputs
- [ ] Add inspect command/view for scout contributions in attempt history

### 47.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 47 manual checklist under `@hivemind-test`
- [ ] Validate scout determinism and restricted tool enforcement
- [ ] Validate scout-to-worker context handoff reproducibility
- [ ] Publish Sprint 47 manual test report artifact in `@hivemind-test`

### 47.5 Exit Criteria
- [ ] Scout builds bounded, relevant context deterministically
- [ ] Scout cannot mutate code or escape restricted tool profile
- [ ] Worker receives attributable scout-generated context inputs
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 48: Constitution Enforcement in Native Loop + Constitution Authoring Agent

**Goal:** Enforce constitution continuously in native runtime boundaries and introduce a human-approved constitution authoring workflow.

> **Principles 2, 4, 14, 15:** Loud policy failure, explicit violations, human authority, no unexplained decisions.

### 48.1 Native Enforcement Hooks
- [ ] Add constitution enforcement checks at:
  - [ ] checkpoint completion
  - [ ] task success boundary
  - [ ] merge prepare/approve/execute boundaries
  - [ ] high-risk tool call boundaries (policy-defined)
- [ ] Emit structured violation evidence with rule IDs and severity

### 48.2 Constitution Authoring Agent
- [ ] Introduce `ConstitutionAuthor` role that proposes (never applies) constitution updates
- [ ] Build proposal payload including:
  - [ ] inferred rules
  - [ ] graph-backed evidence
  - [ ] confidence and risk notes
  - [ ] proposed YAML diff
- [ ] Require explicit human approval command to apply proposals

### 48.3 Governance and Auditability
- [ ] Persist constitution proposal artifacts under governance storage
- [ ] Add event family for proposal lifecycle (`proposed`, `reviewed`, `approved`, `rejected`)
- [ ] Ensure replay can reconstruct authoring history and outcomes

### 48.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 48 manual checklist under `@hivemind-test`
- [ ] Validate hard/advisory native enforcement gates
- [ ] Validate constitution proposal generation and explicit approval/rejection flows
- [ ] Publish Sprint 48 manual test report artifact in `@hivemind-test`

### 48.5 Exit Criteria
- [ ] Native runtime cannot bypass constitution hard rules
- [ ] Constitution authoring workflow is proposal-only and human-gated
- [ ] Enforcement and proposal trails are fully inspectable
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 49: Learnings and Deviation Protocol

**Goal:** Add institutional memory and controlled plan drift contracts with explicit verification and governance.

> **Principles 9, 10, 14:** Checked progression, failures preserved, human authority on critical boundary shifts.

### 49.1 Learnings Model
- [ ] Add learnings lifecycle events:
  - [ ] `LearningLogged`
  - [ ] `LearningUpvoted`
  - [ ] `LearningTagged`
  - [ ] `LearningArchived`
- [ ] Add project-scoped learnings storage and query commands

### 49.2 Scout/Worker Bounded Learnings Integration
- [ ] Allow optional top-N learnings injection by deterministic tag/module matching
- [ ] Enforce strict inclusion budget and provenance in context manifest
- [ ] Emit events for every learning inclusion decision

### 49.3 Deviation Protocol
- [ ] Add deviation lifecycle events:
  - [ ] `DeviationProposed`
  - [ ] `DeviationValidated`
  - [ ] `DeviationApproved`
  - [ ] `DeviationRejected`
- [ ] Require explicit verifier evaluation for deviation proposals
- [ ] Require explicit human approval for acceptance-criteria-changing deviations

### 49.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 49 manual checklist under `@hivemind-test`
- [ ] Validate deterministic learning retrieval and bounded context inclusion
- [ ] Validate deviation block/unblock behavior with verifier and human gates
- [ ] Publish Sprint 49 manual test report artifact in `@hivemind-test`

### 49.5 Exit Criteria
- [ ] Learnings are queryable, attributable, and replay-safe
- [ ] Deviation workflow is explicit, reviewable, and blocking when unresolved
- [ ] Criteria-changing deviations require human approval by contract
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 50: Native Execution Reports and Operational Observability

**Goal:** Deliver deterministic, auditable native runtime reporting for tasks and flows with explicit performance and reliability telemetry.

> **Principles 1, 8, 9, 15:** Facts from events, total observability, mandatory checks, explainable outputs.

### 50.1 Task Report Projection
- [ ] Add native task report projection including:
  - [ ] context operations summary
  - [ ] tool usage summary
  - [ ] runtime turn summary
  - [ ] diffs/commits/check results
  - [ ] constitution violations
  - [ ] deviations (if present)
- [ ] Ensure report is generated deterministically from events + git state

### 50.2 Flow Report Projection
- [ ] Add flow-level aggregate report:
  - [ ] task outcomes
  - [ ] unresolved violations
  - [ ] merge readiness status
  - [ ] runtime and tool reliability metrics
- [ ] Add CLI commands for report generation and inspection

### 50.3 Performance and Reliability Budgets
- [ ] Define measurable budgets and checks:
  - [ ] graph query p95
  - [ ] context assembly p95
  - [ ] event append p95
  - [ ] tool dispatch overhead
- [ ] Add benchmark tests and regression thresholds in CI

### 50.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 50 manual checklist under `@hivemind-test`
- [ ] Validate report determinism and auditability across repeated runs
- [ ] Validate reliability/performance diagnostics visibility
- [ ] Publish Sprint 50 manual test report artifact in `@hivemind-test`

### 50.5 Exit Criteria
- [ ] Native reports are deterministic, readable, and auditable
- [ ] Operational telemetry exposes reliability/performance regressions
- [ ] Report commands are CLI-first and machine-readable
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 51: Native End-to-End Validation and Production Hardening

**Goal:** Prove complete single-repo native execution path, preserve fallback adapter compatibility, and close Phase 4 with production-readiness confidence.

> **Phase close objective:** Native runtime is trusted because it is observable, replayable, bounded, and governable.

### 51.1 Native E2E Scenario (Single Repo)
- [ ] Validate full scenario:
  - [ ] project + governance setup
  - [ ] task + graph + flow creation
  - [ ] scout context build
  - [ ] worker native runtime execution with tools
  - [ ] checkpoint + verification loop
  - [ ] constitution enforcement
  - [ ] merge protocol
- [ ] Validate both successful and intentionally failing paths with complete debuggability

### 51.2 Compatibility and Migration Safety
- [ ] Validate adapter fallback (`opencode`/`codex`/`claude-code`/`kilo`) remains supported
- [ ] Validate feature flags/config migration for existing projects
- [ ] Validate replay compatibility across mixed native/external runtime history

### 51.3 Hardening and Documentation
- [ ] Finalize architecture/design docs for native runtime contracts
- [ ] Update quickstart/install/runtime docs with native workflows
- [ ] Publish operator runbooks for native runtime incidents/recovery

### 51.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 51 manual checklist under `@hivemind-test`
- [ ] Execute full manual Phase 4 end-to-end validation run
- [ ] Publish Sprint 51 manual test report artifact in `@hivemind-test`

### 51.5 Exit Criteria
- [ ] Native runtime path completes end-to-end with auditable evidence
- [ ] Native failure paths are explicit and recoverable
- [ ] Backward compatibility with existing adapters is maintained
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Summary: Phase 4 Capability Coverage

| Capability | Target Sprint |
|------------|---------------|
| Native runtime contracts and harness | 42 |
| Native runtime event model and blob storage | 43 |
| Typed safe tool engine | 44 |
| Active context window operations | 45 |
| UCP graph query integration for runtime | 46 |
| Scout agent deterministic context builder | 47 |
| Native constitution enforcement + authoring proposals | 48 |
| Learnings + deviation governance | 49 |
| Native execution reports + observability budgets | 50 |
| Native end-to-end hardening and phase closeout | 51 |

---

## Phase 4 Completion Gates

- [ ] All Sprint 42-51 manual testing reports are present under `@hivemind-test`
- [ ] `hivemind/changelog.json` is updated with Phase 4 deliverables
- [ ] Documentation is updated and aligned with implementation:
  - [ ] `docs/overview/`
  - [ ] `docs/architecture/`
  - [ ] `docs/design/`
  - [ ] `docs/operations/` (if introduced)
- [ ] Native and external runtime compatibility tests pass in CI
- [ ] Roadmap checkboxes and completion status reflect final validated state

---

## Phase 4 Principle Checkpoints

After each sprint, verify:

1. **Observability:** Did native runtime and tool decisions emit complete events?
2. **No Hidden State:** Can native context/runtime state be replayed deterministically?
3. **Fail Loud:** Are policy/schema/runtime failures explicit and actionable?
4. **CLI-First:** Are all new capabilities operable via machine-readable CLI commands?
5. **Determinism:** Are context hashes, prompt hashes, and replay outcomes stable?
6. **Human Authority:** Are constitution, deviation, and merge boundary approvals explicit?
7. **No Magic:** Can every native decision path be inspected and explained?

---

## Phase 4 Sequencing Rationale (Implementation Note)

- Keep existing external adapters operational while native runtime is introduced.
- Prioritize contracts + observability before higher-level features.
- Build native tools and active context only after deterministic event primitives are proven.
- Treat Scout, learnings, and deviation as governance layers on top of a verified runtime substrate.
- Close the phase only after end-to-end native validation and compatibility checks with existing workflows.

---

## Appendix: Recommended Crate Set by Sprint

> Source basis: `ideaspace/hivemind-rust-echosystem-report.md`.  
> Policy: adopt minimally, pin deliberately, and prefer crates that enforce replayability and safety invariants.

| Sprint | Primary crate recommendations | Notes |
|--------|-------------------------------|-------|
| 42 | `tokio`, `reqwest`, `reqwest-eventsource`, `tracing` | Native loop + streaming transport + structured runtime traces. |
| 43 | `blake3`, `serde_json` (+ canonical JSON helper), `sqlx` or `rusqlite` | Hash-addressed payloads, deterministic fingerprinting, and durable blob/event metadata persistence. |
| 44 | `schemars`, `jsonschema`, `serde_path_to_error`, `cap-std` (optional in v1) | Typed tool schema generation/validation and actionable payload errors; capability-based sandboxing path. |
| 45 | `proptest`, `insta` | Determinism/property and golden tests for context ops + prompt manifest stability. |
| 46 | (No required new crates; reuse UCP integration) | Keep graph query layer bounded and deterministic before adding dependencies. |
| 47 | (No required new crates) | Scout is a role/protocol constraint; prefer reuse of runtime + tool crates. |
| 48 | (No required new crates) | Constitution authoring/enforcement is mainly event/governance model work. |
| 49 | `tantivy` (optional, only if local search for learnings is needed) | Keep learnings simple first; only add indexing when retrieval pressure justifies it. |
| 50 | `criterion`, `tracing-opentelemetry` (optional) | Benchmark and operational telemetry hardening. |
| 51 | (No required new crates) | End-to-end validation and compatibility gates; avoid dependency churn at phase closeout. |

### Recommended Project Study Targets During Phase 4

- `openai/codex` (runtime harness, permissions, protocol boundaries)
- `modelcontextprotocol/rust-sdk` (tool/context protocol layering)
- `wasmtime` and `cap-std` (sandbox defense-in-depth patterns)
