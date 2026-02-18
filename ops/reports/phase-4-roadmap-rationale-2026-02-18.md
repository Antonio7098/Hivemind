# Phase 4 Roadmap Rationale

## 1. Metadata

- **Date**: 2026-02-18
- **Scope**: Justification for `ops/roadmap/phase-4.md` (Sprint 42-51)
- **Inputs reviewed**:
  - `ops/roadmap/phase-1.md`
  - `ops/roadmap/phase-2.md`
  - `ops/roadmap/phase-3.md`
  - `docs/overview/` (vision, PRD, principles, quickstart, install, governance runbooks)
  - `ideaspace/phase-4-v1.md`
  - `ideaspace/hivemind-rust-echosystem-report.md`
  - `src/` architecture (`core`, `adapters`, `storage`, `cli`, `server`)

---

## 2. Executive Summary

Phase 4 is structured as a controlled migration from adapter-driven external runtimes to a native runtime core, without sacrificing existing production guarantees (event authority, replayability, CLI-first control, and human-gated governance).

The sequencing is deliberate:

1. **Contracts first** (`42-43`) so native runtime behavior is modelled before features.
2. **Execution primitives next** (`44-46`) so tools, context, and graph integration are deterministic and bounded.
3. **Governance/intelligence layers afterward** (`47-49`) so scout/learnings/deviation build on a proven substrate.
4. **Operational closeout last** (`50-51`) so reports, budgets, and e2e hardening reflect implemented reality, not aspirations.

This preserves the Phase 1-3 engineering posture: incremental delivery with explicit invariants and audit trails.

---

## 3. Current-State Observations (Codebase)

### 3.1 Strengths to Preserve

- `src/core/events.rs` already has a rich, explicit event model with stable correlation semantics.
- `src/core/state.rs` projection/replay model is mature and aligned with event-sourced invariants.
- `src/core/registry.rs` already enforces deterministic attempt context assembly (`AttemptContextAssembled`, hashes, budgets, truncation events).
- Runtime operational telemetry exists (`RuntimeStarted`, `RuntimeOutputChunk`, projected `Runtime*Observed` family), giving a proven observability baseline.
- Governance stack from Phase 3 (constitution/templates/documents/graph snapshot) is usable as native runtime input substrate.

### 3.2 Gaps Phase 4 Must Address

- Runtime execution is still centered on external CLI adapters (`opencode`, `codex`, `claude-code`, `kilo`) via `SelectedRuntimeAdapter`.
- Existing runtime projection is observational parsing of stdout/stderr, not a first-class native turn/tool model.
- Tool usage is mostly inferred from textual runtime output; native typed tool-call semantics do not exist yet.
- Context assembly is deterministic but mostly pre-run and static; true mid-run active context operations are not modeled.
- `registry.rs` is very large and currently carries most orchestration concerns; Phase 4 needs clearer native-runtime seams.

These are exactly why the roadmap starts with native contracts and explicit event model hardening.

---

## 4. Why This Sprint Sequencing

### 4.1 Sprint 42-43: Runtime Contracts + Native Event Authority

Rationale:

- A native runtime without first-class contracts would reintroduce hidden behavior.
- Event vocabulary must be locked before tool engine/scout/learnings to prevent churn and replay instability.
- Existing adapters remain available as operational safety net while native path matures.

Tradeoff accepted:

- Slower apparent feature velocity in exchange for lower architectural risk and simpler regression analysis.

### 4.2 Sprint 44-46: Tooling, Active Context, Graph Query Integration

Rationale:

- Tool calls are the highest-risk surface (safety + determinism + attribution), so they must be typed and schema-gated.
- Active context needs explicit operations/events, not ad hoc mutation, to preserve replay guarantees.
- Graph usage should be queryable through bounded contracts before scout behavior depends on it.

Tradeoff accepted:

- Extra upfront engineering for policy/validation systems, in return for predictable long-term operability.

### 4.3 Sprint 47-49: Scout, Constitution Authoring, Learnings, Deviations

Rationale:

- These features are governance and reasoning multipliers, but only safe once runtime/tool/context substrate is proven.
- Scout is intentionally restricted to context operations to avoid accidental expansion into a hidden planner.
- Deviation workflow is delayed until verification + human-approval gates are explicit in native path.

Tradeoff accepted:

- Governance intelligence features arrive later, but arrive with stronger control contracts.

### 4.4 Sprint 50-51: Reports, Budgets, and End-to-End Hardening

Rationale:

- Native reports should be projections of real final event contracts, not placeholders.
- Performance budgets must attach to actual implemented paths.
- Phase closure requires both successful and failing native e2e traces with reproducible replay.

Tradeoff accepted:

- Documentation/reporting work is substantial at the end, but it prevents shipping unmeasured complexity.

---

## 5. Philosophies Applied

### 5.1 Strangler Migration, Not Rewrite

The roadmap avoids a "replace everything" rewrite. External adapters remain valid while native runtime is introduced in parallel. This mirrors existing Hivemind philosophy: introduce capabilities without breaking prior guarantees.

### 5.2 Event-First Before Intelligence-First

The roadmap intentionally treats Scout/learnings/deviation as downstream. First, model concrete runtime/tool/context facts. Then build higher-level behavior on top.

### 5.3 Determinism and Provenance as Product Features

Hashes, manifests, blob references, and stable ordering are not implementation details. They are core product guarantees that make failures explainable.

### 5.4 Human Authority at Critical Boundary Changes

Any policy-changing operation (constitution authoring, criteria-changing deviations, merge boundaries) remains explicit and human-gated.

---

## 6. Integration of Ideaspace Inputs

### 6.1 `ideaspace/phase-4-v1.md`

Adopted:

- Native runtime skeleton and null-model harness
- Explicit native runtime event family
- Typed tool system with policy enforcement
- Active context window operations
- Scout agent contract
- Constitution authoring proposal model
- Learnings and deviation protocol
- Native execution reports and performance hardening

Expanded/Concretized:

- Converted conceptual sections into sprint-by-sprint deliverables with concrete exit criteria
- Added compatibility constraints with current adapter-based production path
- Added explicit manual test obligations per sprint
- Added phase-level completion gates and principle checkpoints

### 6.2 `ideaspace/hivemind-rust-echosystem-report.md`

Adopted at roadmap-policy level:

- Keep provider abstraction model-agnostic
- Use schema-driven tool contracts and strict validation
- Preserve deterministic hashing/provenance discipline
- Treat streaming and runtime permissions as first-class concerns

Constrained by current codebase reality:

- Avoid framework sprawl in early sprints
- Introduce dependencies only where they support explicit invariants
- Prioritize contract compatibility with existing event/state machinery

---

## 7. Explicit Codex Pattern Study Requirement

Phase 4 process now includes explicit pattern-study requirements for `codex-main/.npmrc` and concrete upstream implementation patterns.

Observed `.npmrc` patterns:

- `shamefully-hoist=true`
- `strict-peer-dependencies=false`
- `node-linker=hoisted`
- `prefer-workspace-packages=true`

Why this matters:

- These settings reflect practical monorepo dependency ergonomics and reproducibility priorities.
- Phase 4 includes runtime/tooling work that may introduce Node-based helper surfaces or test harness utilities.
- Capturing and applying proven upstream patterns reduces packaging/CI friction and avoids rediscovering solved dependency issues.

Policy decision:

- Pattern study is mandatory in planning.
- Concrete reusable code patterns should be copied/adapted when directly relevant, with clear attribution in sprint artifacts.

---

## 8. Risks and Mitigations

1. **Risk**: Native runtime destabilizes existing execution path.
   - **Mitigation**: Keep adapter fallback first-class; add explicit compatibility tests at phase gates.

2. **Risk**: Event schema churn creates replay regressions.
   - **Mitigation**: Lock event families early (Sprint 43) and add idempotence/replay tests before higher features.

3. **Risk**: Tool execution expands attack surface.
   - **Mitigation**: Deny-by-default command policy, scope gating, structured violation events, and bounded built-in tool set.

4. **Risk**: Context explosion or non-deterministic prompt assembly.
   - **Mitigation**: Context op contracts + budget policies + hash-based golden tests.

5. **Risk**: Governance drift between automated behavior and human intent.
   - **Mitigation**: Human approval gates for constitution changes/deviations and auditable proposal/event trails.

6. **Risk**: Registry complexity continues to grow monolithically.
   - **Mitigation**: Introduce native-runtime contract boundaries early and enforce module ownership by sprint.

---

## 9. Success Criteria for This Roadmap

Phase 4 succeeds only if all are true:

- Native runtime execution is event-complete, replayable, and deterministic.
- Tool calling is typed, policy-enforced, and attributable.
- Active context evolution is explicit and inspectable.
- Governance boundaries (constitution/deviation/merge) remain human-authoritative.
- Existing adapter workflows remain compatible during migration.
- Manual and automated validation provide both success-path and failure-path debuggability.

---

## 10. Recommended Crate Set by Sprint

The roadmap now has an explicit crate appendix. This is the corresponding rationale mapping:

| Sprint | Recommended crates | Why this set |
|--------|--------------------|--------------|
| 42 | `tokio`, `reqwest`, `reqwest-eventsource`, `tracing` | Native loop, provider transport, SSE streaming, structured observability baseline. |
| 43 | `blake3`, `serde_json` (+ canonical JSON helper), `sqlx` or `rusqlite` | Deterministic hashes/provenance and durable runtime payload metadata. |
| 44 | `schemars`, `jsonschema`, `serde_path_to_error`, `cap-std` (optional) | Tool contracts, strict validation, actionable schema errors, capability boundary path. |
| 45 | `proptest`, `insta` | Property and golden testing for context window determinism/prompt stability. |
| 46-48 | No required new crates by default | Focus on contract integration and governance semantics without dependency churn. |
| 49 | `tantivy` (optional) | Only if learnings retrieval scale requires local indexing. |
| 50 | `criterion`, `tracing-opentelemetry` (optional) | Performance budget enforcement and ops telemetry extension. |
| 51 | No required new crates | Hardening/e2e compatibility closeout should avoid late dependency risk. |

Dependency philosophy:

- Prefer minimal additions per sprint.
- Add crates only when they enforce a declared invariant.
- Avoid late-phase dependency churn unless required for correctness or safety.

## 11. Closing

The roadmap intentionally favors correctness and operational trust over rapid feature accumulation. This is consistent with Hivemind’s principles and with current codebase realities.

Phase 4 is designed to deliver a native runtime that is not merely powerful, but governable: observable, deterministic, replayable, and safe to operate in production.
