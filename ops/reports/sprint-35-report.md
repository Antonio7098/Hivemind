# Sprint 35: Documents and Global Context Artifacts

## 1. Sprint Metadata

- **Sprint**: 35
- **Title**: Documents and Global Context Artifacts
- **Branch**: `sprint/35-documents-global-context`
- **Date**: 2026-02-17
- **Owner**: Antonio

## 2. Objectives

- Deliver full project governance document lifecycle (CRUD + inspect/list) with immutable revision history.
- Add task-level attachment controls for explicit runtime document inclusion/exclusion.
- Deliver global governance registries for skills, system prompts, templates, and notepad.
- Implement strict template reference resolution and emit `TemplateInstantiated` events.
- Enforce non-executional/not-injected notepad behavior by contract.
- Complete documentation, changelog/version updates, and manual validation artifacts.

## 3. Delivered Changes

- **CLI surface (Sprint 35 commands)**
  - Added `project governance document` subcommands: `create`, `list`, `inspect`, `update`, `delete`.
  - Added `project governance attachment include|exclude` for task-scoped document attachment lifecycle.
  - Added `project governance notepad create|show|update|delete`.
  - Added `global skill`, `global system-prompt`, `global template` (including `instantiate`), and `global notepad` command groups.

- **Core governance behavior**
  - Added project/global governance artifact lifecycle APIs and schema validation in the registry.
  - Added immutable document revision tracking with metadata (`title`, `owner`, `tags`, `updated_at`).
  - Added strict template resolution against system prompt/skill/document references.
  - Added `TemplateInstantiated` event payload + dispatch integration across CLI labels, server payload mapping, and replay/state handling.
  - Injected explicitly attached project documents into runtime context assembly.
  - Preserved non-injectable notepad contract (project/global notepad content excluded from runtime context by default).
  - Updated notepad `show` semantics so empty scaffold files are treated as logically absent (`exists=false`, `content=null`).

- **Tests**
  - Added integration test coverage for Sprint 35 governance lifecycle and observability:
    - `cli_sprint35_governance_artifacts_and_template_instantiation`
  - Assertions include:
    - document revision lifecycle
    - template instantiation event visibility
    - runtime prompt includes attached document content
    - runtime prompt excludes project notepad content by default

## 4. Documentation and Release Artifacts

- Updated:
  - `docs/design/cli-semantics.md` (detailed Sprint 35 command semantics + event/failure contracts)
  - `docs/architecture/cli-capabilities.md` (project/global governance capabilities)
  - `docs/architecture/event-model.md` (governance/context event family including `TemplateInstantiated`)
  - `ops/roadmap/phase-3.md` (Sprint 35 checklist + exit criteria marked complete)
- Updated release metadata:
  - `Cargo.toml` version bumped to `0.1.26`
  - `changelog.json` includes `v0.1.26` Sprint 35 entry

## 5. Automated Validation

Executed:

- `make validate`
  - `cargo fmt --all --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo nextest run --all-features` (209/209 passed)
  - `cargo doc --no-deps --document-private-items`

Result: ✅ all checks pass.

## 6. Manual Validation (`hivemind-test`)

Artifacts stored under `hivemind-test/test-report/`:

1. `05-sprint35-manual.log`
   - End-to-end CRUD/inspect for project documents, project notepad, global skill/system-prompt/template/notepad
   - attachment include behavior and template instantiation event visibility
   - global notepad delete/show semantics (`exists=false`, `content=null`)

2. `06-sprint35-runtime-context.log`
   - flow execution with attached document
   - `runtime_started.prompt` includes attachment section and latest document revision content
   - explicit checks recorded:
     - `has_attachment_header=true`
     - `has_document_id=true`
     - `has_revision_content=true`
     - `notepad_not_injected=true`

3. `sprint-35-manual-checklist.md`
   - Sprint 35 manual checklist mapped to roadmap items and exit criteria

## 7. Principle Checkpoint

- **Observability is truth (1, 8):** governance lifecycle and template instantiation are explicit events.
- **Fail fast / explicit taxonomy (2, 4):** malformed/invalid governance artifacts return structured, actionable errors.
- **CLI-first (7):** all Sprint 35 capabilities are exposed via CLI command groups.
- **Automated checks mandatory (9):** full validation suite passing.
- **No magic (15):** attachment-based context inclusion is explicit; notepad non-injection is explicit and test-verified.
