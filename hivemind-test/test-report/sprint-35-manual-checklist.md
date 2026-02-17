# Sprint 35 Manual Checklist (Phase 3)

Date: 2026-02-17
Scope: Documents and Global Context Artifacts

## Environment

- [x] Run in isolated HOME sandbox
- [x] Project and governance storage initialized
- [x] Evidence captured as command logs

## 35.1 Project Documents

- [x] Create document (`project governance document create`)
- [x] Inspect document metadata (`title`, `tags`, `owner`, `updated_at`)
- [x] Update document and verify immutable revision history increments
- [x] Confirm inspect returns latest content and full revisions

Evidence: `05-sprint35-manual.log`

## 35.2 Global Skills and System Prompts

- [x] Create/list/inspect/update/delete lifecycle validated for global skills
- [x] Create/list/inspect/update/delete lifecycle validated for global system prompts
- [x] Schema/identifier validation paths observed via CLI contract behavior

Evidence: `05-sprint35-manual.log`

## 35.3 Templates and Instantiation Inputs

- [x] Create global template referencing `system_prompt_id`, `skill_ids[]`, `document_ids[]`
- [x] Instantiate template for project and verify resolved output
- [x] Confirm `template_instantiated` event appears in project event stream

Evidence: `05-sprint35-manual.log`

## 35.4 Notepads (Non-Injectable)

- [x] Project notepad CRUD exercised
- [x] Global notepad CRUD exercised
- [x] Global notepad show returns `exists=false`, `content=null` after delete
- [x] Runtime prompt includes attached document context
- [x] Runtime prompt does **not** include project notepad content by default

Evidence: `05-sprint35-manual.log`, `06-sprint35-runtime-context.log`

## 35.5 Exit Criteria Mapping

- [x] All document-related artifacts are CLI-manageable and event-observable
- [x] Skills/system prompts/templates are globally reusable across projects
- [x] Notepads are clearly separated from execution context
- [x] Artifact operations are replay-safe and deterministic in observed runs
- [x] Manual validation is documented with logs and report artifacts
