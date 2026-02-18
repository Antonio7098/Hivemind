## Sprint 41.5: DB Migration Interlude

**Goal:** Migrate Hivemind state persistence from filesystem-only event logs to a transactional DB backend before Phase 4 scale-up.

> **Principles 1, 3, 11, 15:** Observability is truth, reliability over cleverness, incremental foundations, no magic.

### 41.5.1 Canonical Event Store Migration
- [x] Replace filesystem-global event append/read/stream backend with canonical SQLite persistence at `~/.hivemind/db.sqlite`
- [x] Preserve deterministic `sequence` assignment and monotonic `EventId::from_ordered_u64(sequence)` semantics
- [x] Preserve replay determinism and existing event filter semantics (`project`, `graph`, `flow`, `task`, `attempt`, payload-aware selectors)
- [x] Preserve append atomicity and lock-safety under concurrent CLI process activity

### 41.5.2 Compatibility and Operability
- [x] Keep `events.jsonl` as an append-only compatibility mirror for manual inspection and existing tooling
- [x] Keep governance/document content bodies on filesystem paths (project/global artifacts) while event/state authority moves to DB
- [x] Keep CLI behavior/output/error contracts stable for existing commands and scripts

### 41.5.3 Quality Gates
- [x] `cargo fmt --all`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test --all-features`
- [x] Add targeted storage tests for SQLite append/reload and mirror behavior

### 41.5.4 Manual Testing (`@hivemind-test`)
- [x] Add/update Sprint 41.5 manual checklist under `@hivemind-test`
- [x] Run required manual worktree/execution smoke scripts against migrated backend
- [x] Validate event observability through DB-backed reads and JSONL mirror inspection
- [x] Publish Sprint 41.5 manual test report artifact in `@hivemind-test`

### 41.5.5 Exit Criteria
- [x] Canonical event persistence is DB-backed and transactional
- [x] Existing CLI/event/replay semantics are non-regressed
- [x] Observability remains explicit via `events list|stream|inspect` and JSONL mirror
- [x] Manual validation in `@hivemind-test` is completed and documented
