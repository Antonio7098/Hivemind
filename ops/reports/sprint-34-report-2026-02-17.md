# Sprint 34: Governance Storage Foundation

## 1. Sprint Metadata

- **Sprint**: 34
- **Title**: Governance Storage Foundation
- **Branch**: `sprint/34-governance-storage-foundation`
- **Date**: 2026-02-17
- **Owner**: Antonio

## 2. Objectives

- Move governance artifacts out of repository history into Hivemind-owned storage rooted under `~/hivemind`.
- Provide CLI lifecycle coverage (`project governance init/migrate/inspect`) with observable events and deterministic projections.
- Introduce resilient worktree + governance migration flows, including automatic import of legacy `.hivemind` assets.
- Switch worktree storage to global `~/hivemind/worktrees` with `HIVEMIND_WORKTREE_DIR` overrides and update docs/tests accordingly.
- Deliver complete documentation (CLI semantics, multi-repo, scope-enforcement, roadmap) and sprint reporting with manual/automated validation evidence.

## 3. Delivered Changes

- **Governance CLI + Registry**
  - Added `project governance init`, `migrate`, and `inspect` commands with structured output and event emission.
  - Introduced governance artifact layouts (project/global) and deterministic projection keys with version tracking.
  - Implemented migration helpers that copy legacy `.hivemind` content into global storage and report migrated paths.
  - Added resilient file locking and improved copy semantics to avoid races when integrating flows or rewriting scaffold files.

- **Worktree Foundations**
  - `WorktreeConfig` now defaults to the global `~/hivemind/worktrees` path (configurable via `HIVEMIND_WORKTREE_DIR`).
  - Registry + CLI flows consume the new location; redundant clones and lock contention sources cleaned up.
  - Manual test scripts (`test_worktree.sh`, `test_execution.sh`) detect the repo automatically, resolve the binary from the shared Cargo target, and emit evidence from the global directory.

- **Documentation + Roadmap**
  - `docs/design/multi-repo.md` and `docs/design/scope-enforcement.md` describe the global worktree layout + env overrides.
  - `docs/design/cli-semantics.md` documents the new governance commands and updated cleanup semantics.
  - `ops/roadmap/phase-3.md` marks Sprint 34 storage/event/migration deliverables complete while leaving manual testing items pending future sprints.
  - `changelog.json` gains a v0.1.25 entry; `Cargo.toml` bumped to 0.1.25.

## 4. Automated Validation

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo nextest run --all-features` (208 tests)
- `cargo doc --no-deps --document-private-items`
- `make validate` (wraps fmt, lint, nextest, docs) ✅

## 5. Manual Validation (`../hivemind-test`)

Evidence stored under `hivemind-test/test-report/`:

1. **Worktree lifecycle** – `2026-02-17-sprint34-worktree.log`
   - Creates a fresh repo via isolated HOME (`/tmp/hivemind-test/.hm_home`).
   - Runs project/task/flow creation, starts task to materialize worktree, inspects via CLI, lists global path `~/hivemind/worktrees/...`.
   - Confirms governance storage under the global directory instead of repo-local `.hivemind`.
2. **Execution flow** – `2026-02-17-sprint34-execution.log`
   - Creates a dedicated project, runs flow start/tick, observes governance + execution events.
   - Verifies the task attempt creation, checkpoint lifecycle events, and presence of files within the global worktree directory.

## 6. Follow-up / Notes

- Manual validation checklist items in the roadmap remain to be templatized for future sprints (marked pending in Phase 3 doc).
- Additional CLI refactors (e.g., breaking up `handle_project` in `src/main.rs`) can be scheduled outside Sprint 34 to reduce lint exemptions.
- Continue monitoring shared Cargo target usage to ensure binaries remain discoverable in automation environments.
