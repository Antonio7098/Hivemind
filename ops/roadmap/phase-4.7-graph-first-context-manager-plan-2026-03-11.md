# Phase 4.7 Graph-First Context Manager Plan
Date: 2026-03-11
Status: Proposed
Primary roadmap anchor: `ops/roadmap/phase-4.7-native-harness-context-and-capability-lift.md` (Sprint 59 / Sprint 60)
## Purpose
Define the next implementation plan for native runtime context management so Hivemind treats CodeGraph/GraphCode as the primary semantic workspace and uses raw text only where exact lexical evidence is required.

This plan is intentionally about **context engineering**, not prompt rewriting and not task engineering.

## Problem To Solve
Current runtime context management still leans too heavily on file-sized text carryover, recency-biased transcript history, and size-triggered truncation. That keeps prompts bounded, but it is not yet the most intelligent use of CodeGraph.

Observed gaps:
- files are treated as the main memory unit instead of symbols, regions, and dependency neighborhoods
- large clean windows are compressed by text size rather than by semantic structure and task relevance
- transcript/tool history still carries structural meaning that should live in graph-backed memory
- the runtime lacks an explicit relevance-scored working-set model with explainable downgrade timing
- exact edit sufficiency is not yet a first-class contract distinct from general awareness

## Target End State
Every turn is assembled from a runtime-managed working set with three explicit packets:
1. **Execution packet** — exact text required for the next safe action
2. **Structural packet** — graph-backed symbol/dependency context for orientation and navigation
3. **Memory packet** — compact evidence of what has already been tried, edited, or validated

The graph is the map. Exact text is the terrain.

## Design Rules
1. CodeGraph is the default long-lived memory for repository-code tasks.
2. Raw file text is only retained when exact lexical detail is needed.
3. Clean windows degrade to graph-backed summaries before they degrade to arbitrary truncation.
4. Dirty windows preserve exact edited regions and nearby symbol boundaries.
5. No write should proceed without an explicit proof that the target edit region has been seen exactly.
6. Transcript/tool history should preserve evidence, not carry orientation that belongs in the graph.
7. Promotion/demotion decisions must be attributable and explainable in native telemetry.
8. Every context object should expose a downgrade horizon and allow bounded lease extensions when the model anticipates future need.

## Core Model
### A. Working-set unit
Replace file-first thinking with context objects scored independently:
- symbol
- symbol cluster
- file region
- dependency neighborhood
- test target
- validation artifact
- dirty edit locus

### B. Multi-resolution context levels
Each context object should support progressive detail levels:
- L0: handle only (`path`, `symbol`, kind, provenance, freshness)
- L1: structural card (signature, role, neighbors, related tests)
- L2: exact local excerpt (enclosing symbol / target region)
- L3: expanded excerpt set (neighboring helpers / assertions / call sites)
- L4: full local file region
- L5: full file body (rare)

### C. Relevance-scored visibility
Each context object should carry:
- a relevance score
- a current representation level
- a turns-until-next-downgrade horizon
- a minimum floor
- an optional lease extension state

Relevance should be driven by task alignment, recent actions, validation evidence, and graph proximity to edited or failing loci. Downgrade should be gradual (`exact excerpt -> graph card -> handle`) rather than binary disappearance.

### D. V1 relevance scoring proposal
The first implementation should score context objects in a bounded `0..100` range and map scores to representation levels rather than binary keep/drop visibility.

**Context objects:**
- symbol
- file region / excerpt
- dependency neighborhood
- dirty edit locus
- test/assertion block
- validation artifact
- tool-result summary

**Core score inputs:**
- direct task mention / objective alignment: `+30`
- currently dirty edit target: `+40`
- failing assertion / compile error source: `+35`
- same file or enclosing symbol as dirty locus: `+25`
- one-hop dependency of dirty or failing locus: `+20`
- related test target: `+20`
- cited by recent action/edit: `+16`
- reread / revisited in the recent horizon: `+12`
- recent read / hydration: `+8`
- unresolved uncertainty / repeated consultation: `+8`
- bounded model lease extension: `+5..+10` max

**Propagation policy:**
- seed propagation from dirty symbols, active exact excerpts, failing tests/assertions, and latest validated command targets
- use typed graph edges with bounded depth (`<=2` for v1)
- strong edges: `contains`, `implements`, `required_by_contract`, `failed_in`, `tested_by`
- medium edges: `calls`, `called_by`, `emits`, `consumes`
- weak edges: `imports`, `same_file_sibling`, broad module neighbors
- decayed propagation bonus on later hops so broad dependency closure cannot keep everything hot

**Decay policy:**
- no decay for the first 2 untouched turns after promotion
- then apply `-4` per untouched turn by default
- dirty edit loci, failing assertions, and explicitly leased items decay more slowly or hold their floor

**Representation thresholds:**
- `85..100`: expanded excerpt set
- `65..84`: exact excerpt
- `40..64`: graph card + signature / key excerpt
- `20..39`: handle only
- `<20`: omitted from prompt, retained in runtime memory

**Downgrade horizon:**
- compute the next lower threshold for the current representation level
- estimate turns until crossing that threshold under current decay assumptions
- expose `turns_until_next_downgrade` to the model and telemetry rather than a binary removal deadline

**Lease policy:**
- the model may request a bounded lease extension for a context object or floor level
- leases are time-boxed, score-capped, and budget-authoritative
- the runtime can grant, shorten, degrade, or deny the requested lease with an explicit reason

**Safety floors:**
- dirty edit targets: minimum floor = exact excerpt
- failing assertion / failing test blocks: minimum floor = exact excerpt
- active contracts / interfaces / schemas: minimum floor = graph card or exact signature
- stale exploratory tool output: minimum floor = summary only or none

## Planned Workstreams
### 1. Graph-first compaction for large clean windows
When a clean window exceeds budget:
- preserve file identity and provenance
- preserve the exact last-read symbol/region if available
- replace the rest with a CodeGraph structural card:
  - containing symbols
  - sibling symbols
  - caller/callee or import/export neighborhood
  - related tests
- only fall back to blind text truncation if graph-backed rendering is unavailable

**Exit check:** oversized clean windows render primarily as symbol/dependency context instead of arbitrary head/tail text.

### 2. Relevance-driven promotion, propagation, and leases
Introduce scoring that combines:
- task mention / objective alignment
- recent read or write activity
- graph distance to dirty files, failing tests, or active symbols
- likelihood of imminent mutation
- unresolved uncertainty
- bounded model-requested lease extensions

Add bounded dependency propagation so support code read early can regain relevance when later edits/tests are downstream of it.

**Exit check:** context retention is driven by task/graph relevance, not simple recency, and the runtime can explain both the current score and the next downgrade horizon.

### 3. Edit-sufficiency guardrails
Before any write-capable action, require exact lexical visibility for:
- target symbol/region
- immediately surrounding boundary or signature context
- relevant assertion/test block when validation depends on it

If only graph awareness exists, auto-promote the exact excerpt first.

**Exit check:** the runtime can explain why an edit was safe to attempt.

### 4. Semantic memory instead of transcript ballast
Convert retained history into compact evidence objects rather than raw replay:
- read/write facts
- tool outcome summaries
- validation outcomes
- dirty-path records
- graph-session transitions

**Exit check:** transcript items stop carrying structural orientation that the graph already knows.

### 5. Tool and validation context shaping
Continue summarizing noisy tools, but make summaries graph-aware where possible:
- `list_files` -> directory/symbol neighborhood summary
- `read_file` -> symbol/region hydration record
- `run_command` -> failure class + affected targets/tests
- graph/code queries -> attributable session deltas instead of opaque dumps

**Exit check:** tool outputs preserve decision-relevant facts with minimal prompt surface.

### 6. Observability and evals
Add provenance for every context packet decision:
- why an item was promoted/demoted
- why a clean window became graph-summary form
- why an excerpt was required before edit
- which graph neighbors were selected and why
- why a lease extension was granted, shortened, or denied
- why a score changed due to dependency propagation

Manual/provider-backed evals must compare:
- prompt size by turn
- exact-text vs graph-summary ratio
- tool calls to first correct edit
- redundant rereads / redundant navigation
- successful task completion quality

**Exit check:** we can prove smaller prompts without worse task completion.

## Non-Goals
- no new Hivemind-specific graph/query engine
- no prompt/task engineering disguised as context work
- no hidden model-side memory
- no replacement of exact excerpts with graph summaries during actual edits

## Recommended Execution Order
1. implement graph-first compaction for oversized clean windows
2. add relevance scoring, downgrade horizons, dependency propagation, and promotion telemetry
3. add edit-sufficiency auto-promotion before writes
4. convert retained history into semantic memory objects
5. run deep provenance/provider-backed evals and tune thresholds only after evidence

## Success Criteria
This plan succeeds when a real provider-backed coding run shows:
- materially smaller prompts
- fewer redundant tool calls
- active context centered on symbols/edit loci instead of whole files
- graph-backed structural awareness replacing stale transcript/file carryover
- no loss of edit safety or task completion quality
- the model can see and use downgrade horizons without pinning large amounts of stale context