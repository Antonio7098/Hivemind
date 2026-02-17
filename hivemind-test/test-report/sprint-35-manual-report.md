# Sprint 35 Manual Test Report

Date: 2026-02-17
Scope: Phase 3 / Sprint 35 — Documents and Global Context Artifacts

## Summary

Manual validation confirms Sprint 35 governance artifact flows are operational and observable:

- project documents support CRUD + inspect with immutable revisions
- task-level attachment include/exclude lifecycle is recorded
- global skill/system-prompt/template/notepad registries are CLI-manageable
- template instantiation emits `template_instantiated` events with resolved IDs
- notepad content remains non-executional and is not injected into runtime context by default

## Evidence Artifacts

- `05-sprint35-manual.log`
- `06-sprint35-runtime-context.log`
- `sprint-35-manual-checklist.md`

## Validations Performed

1. Project governance initialization and document lifecycle
   - `project governance init`
   - `project governance document create/update/inspect`
   - verified metadata and revision history (`revision: 1 -> 2`)

2. Task attachment lifecycle
   - `project governance attachment include`
   - verified `governance_attachment_lifecycle_updated` in event stream

3. Project/global notepad lifecycle
   - project notepad create/show
   - global notepad create/show/delete/show
   - verified deleted global notepad reports `exists=false`, `content=null`

4. Global artifact registries
   - `global skill create`
   - `global system-prompt create`
   - `global template create/instantiate`
   - verified `template_instantiated` event payload in project stream

5. Runtime context injection behavior
   - configured flow execution with attached project document
   - verified runtime prompt contains:
     - attachment section header
     - `document_id: doc-main`
     - latest document content (`revision two`)
   - verified runtime prompt excludes project notepad marker content

## Exit Criteria Check

- [x] All document-related artifacts are CLI-manageable and event-observable
- [x] Skills/system prompts/templates are globally reusable across projects
- [x] Notepads are clearly separated from execution context
- [x] Artifact operations are replay-safe and deterministic (observed under repeated CLI/event inspection)
- [x] Manual validation is completed and documented
