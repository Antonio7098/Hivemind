# Sprint 36: Constitution Core

## 1. Sprint Metadata

- **Sprint**: 36
- **Title**: Constitution Core
- **Branch**: `sprint/36-constitution-core`
- **Date**: 2026-02-17
- **Owner**: Antonio

## 2. Objectives

- Define and enforce Constitution Schema v1 (partitions, rule types, parameters, severity, compatibility/version fields).
- Add CLI-first constitution lifecycle commands: `init`, `show`, `validate`, `update`.
- Require explicit confirmation for constitution mutations and make mutation intent/actor attribution auditable.
- Emit constitution lifecycle/validation events and persist constitution digest/version in project state projection.
- Complete manual validation evidence under `hivemind-test`.

## 3. Delivered Changes

- **CLI surface**
  - Added top-level command group: `hivemind constitution`.
  - Added subcommands:
    - `hivemind constitution init <project> [--content|--from-file] --confirm [--actor] [--intent]`
    - `hivemind constitution show <project>`
    - `hivemind constitution validate <project>`
    - `hivemind constitution update <project> (--content|--from-file) --confirm [--actor] [--intent]`

- **Core constitution model and validation**
  - Added Constitution Schema v1 model with strict YAML schema decoding.
  - Added semantic validation checks for:
    - compatibility/version fields
    - duplicate/empty partition IDs and paths
    - duplicate/empty rule IDs
    - rule partition references and self-dependency checks
    - coverage threshold bounds
  - Added deterministic constitution digest generation and canonical YAML writes.

- **Event model and projection**
  - Added lifecycle events:
    - `ConstitutionInitialized`
    - `ConstitutionUpdated`
    - `ConstitutionValidated`
  - Added mutation audit fields (`actor`, `mutation_intent`, `confirmed`, digest/revision metadata).
  - Added project projection fields:
    - `constitution_digest`
    - `constitution_schema_version`
    - `constitution_version`
    - `constitution_updated_at`

- **Tests**
  - Added integration test: `cli_sprint36_constitution_lifecycle_and_auditability`.
  - Coverage includes confirmation gate enforcement, lifecycle commands, validation behavior, event emission visibility, and project projection updates.

## 4. Documentation and Release Artifacts

- Updated roadmap checklist: `ops/roadmap/phase-3.md` (Sprint 36 marked complete).
- Updated architecture/design docs:
  - `docs/architecture/cli-capabilities.md`
  - `docs/architecture/event-model.md`
  - `docs/architecture/state-model.md`
  - `docs/architecture/architecture.md`
  - `docs/design/cli-semantics.md`
- Added manual test artifacts:
  - `hivemind-test/test-report/07-sprint36-manual.log`
  - `hivemind-test/test-report/sprint-36-manual-checklist.md`
  - `hivemind-test/test-report/sprint-36-manual-report.md`
- Updated release metadata:
  - `Cargo.toml` version bumped to `0.1.27`
  - `changelog.json` entry added for `v0.1.27`

## 5. Automated Validation

Executed:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features --locked --offline -- -D warnings`
- `cargo test --all-features --locked --offline --test integration`
- `HIVEMIND_DATA_DIR=/tmp/hm-data HIVEMIND_WORKTREE_DIR=/tmp/hm-worktrees cargo test --all-features --locked --offline --lib`
- `cargo test --doc --all-features --locked --offline`
- `cargo doc --no-deps --document-private-items --locked --offline`

Result: ✅ command suite above passes.

Coverage note:
- `cargo llvm-cov --all-features --lcov --output-path lcov.info --locked --offline` was attempted.
- In this sandbox, llvm-cov test execution hit worktree permission constraints for a subset of registry tests and did not complete end-to-end.

## 6. Manual Validation (`hivemind-test`)

Manual run evidence is captured in:

- `hivemind-test/test-report/07-sprint36-manual.log`
- `hivemind-test/test-report/sprint-36-manual-checklist.md`
- `hivemind-test/test-report/sprint-36-manual-report.md`

Validated manually:

- confirmation gating for mutation commands (`init` fails without `--confirm`)
- successful `init/show/validate/update` command lifecycle
- auditability in event stream for:
  - `constitution_initialized`
  - `constitution_updated`
  - `constitution_validated`
- projection state updates in `project inspect` (`constitution_*` fields)

## 7. Principle Checkpoint

- **1/8 Observability is truth / absolute observability**: constitution lifecycle + validation are explicit events with digest/revision metadata.
- **2/4 Fail fast + explicit error taxonomy**: strict schema and semantic validation return structured errors with actionable hints.
- **5/11 Structure scales + incremental foundations**: constitution governance is delivered as a focused vertical slice before enforcement-in-flow sprint work.
- **7 CLI-first**: all constitution lifecycle actions are CLI-addressable.
- **14 Human authority**: mutations require explicit human confirmation and preserve actor intent metadata.
- **15 No magic**: no implicit constitution mutation paths; audit trails are explicit.
