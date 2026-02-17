## Sprint 34: Governance Storage Foundation

**Goal:** Establish Hivemind-owned storage and event contracts for governance/context artifacts without coupling them to repository history.

> **Principles 1, 8, 11, 15:** Observability is truth, no hidden state, incremental foundations, no magic.

### 34.1 Storage Topology and Boundaries
- [x] Define canonical governance layout under `~/hivemind/` managed by the registry CLI:
  - [x] `projects/<project-id>/constitution.yaml`
  - [x] `projects/<project-id>/documents/`
  - [x] `projects/<project-id>/notepad.md`
  - [x] `projects/<project-id>/graph_snapshot.json`
  - [x] `global/skills/`
  - [x] `global/system_prompts/`
  - [x] `global/templates/`
  - [x] `global/notepad.md`
- [x] Keep code ownership in Git and governance ownership in Hivemind with no implicit repo writes
- [x] Add explicit export/import boundary design (not auto-enabled)

### 34.2 Event and Projection Contracts
- [x] Introduce governance event families for create/update/migrate and attachment lifecycle
- [x] Ensure all governance state is reconstructable from events plus deterministic projections
- [x] Add projection versioning and schema markers for forward-safe migrations

### 34.3 Backward-Compatible Migration
- [x] Add migration from current local registry layout to governance-aware layout
- [x] Preserve existing project/task/flow behavior and event replay determinism
- [x] Emit explicit migration events and include rollback instructions in output hints

### 34.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 34 manual checklist under `@hivemind-test` (`/home/antonio/programming/Hivemind/hivemind-test`)
- [ ] Manually validate storage topology creation and CLI-managed lifecycle behavior
- [ ] Manually validate migration and rollback behavior on representative local data
- [ ] Publish Sprint 34 manual test report artifact in `@hivemind-test`

### 34.5 Exit Criteria
- [x] Governance directory structure is created and managed by CLI
- [x] Governance mutations emit structured events with correlation IDs
- [x] Replay can rebuild governance projections deterministically
- [x] Existing non-governance workflows remain stable
- [ ] Manual validation in `@hivemind-test` is completed and documented (pending manual runs)

---

## Sprint 35: Documents and Global Context Artifacts

**Goal:** Deliver all document-related artifact management in one sprint (project documents, skills, system prompts, templates, and notepads).

> **User Story 11:** Teams manage context artifacts as first-class, observable Hivemind state.

### 35.1 Project Documents
- [x] Add CLI for project document lifecycle (`create`, `list`, `inspect`, `update`, `delete`)
- [x] Support metadata (`title`, `tags`, `owner`, `updated_at`) and immutable revision history
- [x] Add attachment controls for execution inclusion (explicit include/exclude)

### 35.2 Global Skills and System Prompts
- [x] Add CLI for global skill registry management
- [x] Add CLI for global system prompt registry management
- [x] Validate artifact schemas and reject malformed content with structured errors

### 35.3 Templates and Instantiation Inputs
- [x] Add template lifecycle commands referencing `system_prompt_id`, `skill_ids[]`, `document_ids[]`
- [x] Resolve template references with strict validation and actionable failure hints
- [x] Emit `TemplateInstantiated`-style events with resolved artifact IDs

### 35.4 Notepads (Non-Injectable)
- [x] Add `global notepad` and `project notepad` CRUD commands
- [x] Mark notepads as non-executional and non-validating by contract
- [x] Ensure notepad content is never injected into runtime input by default

### 35.5 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 35 manual checklist under `@hivemind-test`
- [x] Manually validate document/skill/system prompt/template/notepad CRUD and inspect flows
- [x] Manually validate attachment resolution behavior and failure paths
- [x] Confirm notepad content is not injected into runtime context in manual runs
- [x] Publish Sprint 35 manual test report artifact in `@hivemind-test`

### 35.6 Exit Criteria
- [x] All document-related artifacts are CLI-manageable and event-observable
- [x] Skills/system prompts/templates are globally reusable across projects
- [x] Notepads are clearly separated from execution context
- [x] Artifact operations are replay-safe and deterministic
- [x] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 36: Constitution Core

**Goal:** Make project constitution explicit, mandatory, structured, and observable.

> **Principles 5, 7, 14:** Structured operations, CLI-first control, human authority on critical governance boundaries.

### 36.1 Constitution Schema v1
- [x] Define schema with partitions, rule types, parameters, and severity (`hard`, `advisory`, `informational`)
- [x] Add strict schema validation with compatibility/version fields
- [x] Document rule semantics and invalid configuration failure modes

### 36.2 Constitution Lifecycle Commands
- [x] Add `hivemind constitution init/show/validate/update`
- [x] Require exactly one constitution per project
- [x] Require explicit confirmation for constitution mutation commands

### 36.3 Constitution Event Model
- [x] Emit lifecycle events (`ConstitutionInitialized`, `ConstitutionUpdated`, validation events)
- [x] Preserve full audit trail for mutation intent and actor attribution
- [x] Record constitution digest/version in project state projection

### 36.4 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 36 manual checklist under `@hivemind-test`
- [x] Manually validate constitution init/show/validate/update behavior and confirmation gates
- [x] Manually validate auditability of constitution events and projection state updates
- [x] Publish Sprint 36 manual test report artifact in `@hivemind-test`

### 36.5 Exit Criteria
- [x] Every active project has one valid constitution
- [x] Constitution cannot be silently changed or bypassed
- [x] Constitution mutations are attributable, inspectable, and replayable
- [x] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 37: UCP Graph Integration and Snapshot Projection

**Goal:** Integrate the UCP codebase graph engine into Hivemind and project its output into Hivemind-managed snapshot lifecycle/events/governance flow.

> **User Story 12:** Architecture governance is tied to observable code facts, not prose assertions.
>
> **Implementation split:** Core graph extraction is specified in `unified-content-protocol/ops/sprint-37-codebase-ucm-graph.md`.
>
> **UCP prerequisites:** `unified-content-protocol/ops/sprint-36-ucm-codegraph-readiness.md` and `unified-content-protocol/ops/sprint-37-codebase-ucm-graph.md` are complete.

### 37.1 UCP Integration Contract
- [x] Consume UCP graph output as the source engine (no duplicate parser/extractor implementation in Hivemind)
- [x] Pin and record UCP engine version used for extraction (`ucp_engine_version`)
- [x] Require UCP `CodeGraphProfile v1` compliance metadata (`profile_version`) on all accepted snapshots
- [x] Require UCP `canonical_fingerprint` and stable `logical_key` semantics for diff/replay reliability
- [x] Enforce accepted graph scope from UCP (repository, directory, file, top-level symbol, imports/depends_on) for context-oriented use

### 37.2 Hivemind Snapshot Projection Model
- [x] Materialize UCP output under `projects/<project-id>/graph_snapshot.json`
- [x] Define Hivemind snapshot envelope fields (schema/version metadata, provenance, UCP profile version, UCP canonical fingerprint, summary stats)
- [x] Preserve UCP-style structure+blocks projection for downstream context assembly and inspectability

### 37.3 Lifecycle and Observability
- [x] Trigger snapshot build/rebuild on project attach, checkpoint completion, merge completion, and explicit refresh command
- [x] Add/maintain explicit command: `hivemind graph snapshot refresh`
- [x] Emit snapshot lifecycle events (`started`, `completed`, `failed`, `diff_detected`) with correlation IDs

### 37.4 Integrity and Staleness Gates
- [x] Detect stale snapshots by comparing snapshot commit provenance vs current HEAD before constitution checks
- [x] Fail loud with actionable recovery hint when snapshot is stale/missing (`Run: hivemind graph snapshot refresh`)
- [x] Verify snapshot integrity using UCP-provided canonical fingerprint contract
- [x] Use stable `logical_key` references for semantic diffing across block ID churn

### 37.5 Compatibility and Non-Regression
- [x] Existing project/task/graph/flow execution semantics remain unchanged when codegraph integration is enabled
- [x] Existing event replay/state reconstruction remains deterministic with graph snapshot integration present
- [x] Existing CLI contracts (output formats, error contracts, exit codes) remain stable
- [x] Existing runtime adapters and verification/merge flows operate without regression
- [x] Projects that do not use constitution checks continue to function without new mandatory graph coupling

### 37.6 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 37 manual integration checklist under `@hivemind-test` (`/home/antonio/programming/Hivemind/hivemind-test`)
- [x] Manually validate UCP graph ingestion for:
  - [x] project attach trigger
  - [x] checkpoint completion trigger
  - [x] merge completion trigger
  - [x] manual refresh command
- [x] Manually validate staleness and recovery paths (`hivemind graph snapshot refresh`) with constitution checks enabled
- [x] Manually validate non-constitution projects and existing flow execution to confirm no regressions
- [x] Publish manual test report artifact in `@hivemind-test` with observed results and remediation notes

### 37.7 Exit Criteria
- [x] Hivemind uses UCP graph engine as authoritative extraction backend
- [x] Hivemind accepts only profile-compliant UCP graph artifacts
- [x] Snapshot lifecycle triggers and event telemetry are explicit and observable
- [x] Constitution engine receives stable, reproducible graph input
- [x] Hivemind-owned versioning, eventing, and failure semantics are fully enforced
- [x] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 38: Constitution Enforcement

**Goal:** Enforce constitution rules during execution boundaries and merge governance.

> **Principles 2, 4, 10:** Fail fast, explicit error taxonomy, failures are first-class.

### 38.1 Enforcement Engine
- [ ] Implement `validate(graph_snapshot, constitution)` evaluation engine
- [ ] Support severity-aware outcomes: block (`hard`), report (`advisory`), log (`informational`)
- [ ] Emit `ConstitutionViolationDetected`-style events with rule IDs and evidence

### 38.2 Enforcement Gates
- [ ] Run constitution checks after checkpoint completion
- [ ] Run constitution checks before merge prepare/approve/execute gates
- [ ] Add explicit `hivemind constitution check --project <id>` command

### 38.3 UX and Explainability
- [ ] Provide structured violation outputs with exact rule mapping and remediation hints
- [ ] Preserve violation history per flow/task for retry and review loops
- [ ] Ensure no hidden retries or automatic rule downgrades

### 38.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 38 manual checklist under `@hivemind-test`
- [ ] Manually validate hard/advisory/informational constitution outcomes and gate behavior
- [ ] Manually validate checkpoint and merge-boundary enforcement flows
- [ ] Publish Sprint 38 manual test report artifact in `@hivemind-test`

### 38.5 Exit Criteria
- [ ] Hard violations block progression deterministically
- [ ] Advisory/informational violations remain visible and queryable
- [ ] Enforcement behavior is deterministic under replay
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 39: Deterministic Agent Context Assembly

**Goal:** Freeze reproducible attempt context from explicit governance artifacts.

> **User Story 13:** Every attempt can be reproduced from a context manifest and event trail.

### 39.1 Context Assembly Contract
- [ ] Define ordered assembly inputs:
  - [ ] Constitution (always)
  - [ ] Selected system prompt
  - [ ] Attached skills
  - [ ] Attached project documents
  - [ ] High-level graph summary
- [ ] Exclude notepads and implicit memory sources by default
- [ ] Add size budget and truncation policy with explicit eventing

### 39.2 Attempt Context Manifest
- [ ] Persist immutable context manifest per attempt (resolved IDs, revisions, hashes)
- [ ] Emit context assembly and delivery events with correlation IDs
- [ ] Add `attempt inspect --context` style visibility for debugging

### 39.3 Template Resolution Semantics
- [ ] Resolve template references at attempt start and freeze the resolved set
- [ ] Permit explicit per-attempt include/exclude overrides with audit events
- [ ] Guarantee retry behavior references prior attempt manifests explicitly

### 39.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 39 manual checklist under `@hivemind-test`
- [ ] Manually validate deterministic context-manifest generation across repeated runs
- [ ] Manually validate retry context-manifest linkage and `attempt inspect --context` behavior
- [ ] Publish Sprint 39 manual test report artifact in `@hivemind-test`

### 39.5 Exit Criteria
- [ ] Attempt context assembly is deterministic and fully inspectable
- [ ] Context replay reproduces identical manifests for identical inputs
- [ ] Runtime initialization does not rely on hidden state
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 40: Governance Replay and Recovery

**Goal:** Guarantee reversibility of governance artifacts using event replay and bounded snapshotting.

> **Principles 1, 9, 10:** Replayability, automated checks, failures preserved.

### 40.1 Replay Semantics for Governance
- [ ] Extend replay engine to rebuild constitution/doc/template/skill projections
- [ ] Add governance replay verification command(s)
- [ ] Validate idempotence across repeated projection rebuilds

### 40.2 Snapshot and Restore
- [ ] Add optional periodic governance snapshots for faster recovery
- [ ] Define restore procedure that preserves event authority
- [ ] Emit explicit events for snapshot created/restored operations

### 40.3 Corruption and Drift Handling
- [ ] Detect projection/file drift relative to canonical event history
- [ ] Provide deterministic repair workflow (`detect`, `preview`, `repair`)
- [ ] Classify and surface failures with structured, actionable errors

### 40.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 40 manual checklist under `@hivemind-test`
- [ ] Manually validate replay-based recovery after projection/file loss scenarios
- [ ] Manually validate drift detection/preview/repair workflow end-to-end
- [ ] Publish Sprint 40 manual test report artifact in `@hivemind-test`

### 40.5 Exit Criteria
- [ ] Governance state can be recovered from events after projection loss
- [ ] Replay and restore paths are tested for determinism and idempotence
- [ ] Drift detection and repair are CLI-driven and auditable
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Sprint 41: Governance Hardening and Production Readiness

**Goal:** Ship the governance/context layer with reliability and operational clarity.

### 41.1 Testing and Reliability
- [ ] Add unit/integration/e2e coverage for governance lifecycle and enforcement gates
- [ ] Add regression suites for context manifest determinism and replay integrity
- [ ] Stress test artifact operations under concurrent flow activity

### 41.2 Observability and Operations
- [ ] Extend event query filters for artifact IDs, template IDs, and constitution rule IDs
- [ ] Add operator diagnostics for missing artifacts, invalid references, and stale snapshots
- [ ] Publish runbooks for migration, drift recovery, and constitution policy changes

### 41.3 Documentation and Adoption
- [ ] Update CLI help and docs for all governance commands and invariants
- [ ] Publish architecture/design updates reflecting Phase 3 contracts
- [ ] Add quickstart scenario for constitution + template-driven execution

### 41.4 Manual Testing (`@hivemind-test`)
- [ ] Add/update Sprint 41 manual checklist under `@hivemind-test`
- [ ] Execute a full manual Phase 3 end-to-end validation run and document outcomes
- [ ] Publish Sprint 41 manual test report artifact in `@hivemind-test`

### 41.5 Exit Criteria
- [ ] Governance/context features meet principle checkpoints with no hidden state
- [ ] Operators can inspect, explain, and recover governance state end-to-end
- [ ] Phase 3 capabilities are stable for broad project use
- [ ] Manual validation in `@hivemind-test` is completed and documented

---

## Summary: Phase 3 Capability Coverage

| Capability | Target Sprint |
|------------|---------------|
| Hivemind-owned governance storage | 34 |
| Documents + skills + prompts + templates + notepads | 35 |
| Mandatory project constitution | 36 |
| UCP-backed code graph snapshots and integration | 37 |
| Constitution enforcement gates | 38 |
| Deterministic context manifests | 39 |
| Governance replay and recovery | 40 |
| Hardening, docs, and operability | 41 |

---

## Phase 3 Completion Gates

- [ ] All Sprint 34-41 manual testing reports are present under `@hivemind-test`
- [ ] `hivemind/changelog.json` is updated with Phase 3 roadmap-driven deliverables
- [ ] Documentation is updated and aligned with implementation:
  - [ ] `docs/overview/`
  - [ ] `docs/architecture/`
  - [ ] `docs/design/`
- [ ] Roadmap checkboxes and completion status reflect the final validated state

---

## Phase 3 Principle Checkpoints

After each sprint, verify:

1. **Observability:** Did every governance mutation emit events?
2. **No Hidden State:** Can state be reconstructed from events and deterministic projections?
3. **Fail Loud:** Are schema, reference, and policy failures explicit and actionable?
4. **CLI-First:** Are all operations fully available via CLI with machine-readable output?
5. **Determinism:** Do replay and context assembly produce stable results?
6. **Human Authority:** Are constitution and merge boundary decisions explicit?
7. **No Magic:** Can every decision path be inspected and explained?
