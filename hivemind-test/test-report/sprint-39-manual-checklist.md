# Sprint 39 Manual Checklist (Phase 3)

Date: 2026-02-18
Scope: Deterministic Agent Context Assembly

## Environment

- [x] Manual run executed with isolated `HOME`
- [x] Governance artifacts initialized (constitution, documents, system prompt, skills, template)
- [x] Evidence captured in command logs under `hivemind-test/test-report/`

## 39.1 Context Assembly Contract

- [x] Attempt context manifests include ordered inputs: constitution, system prompt, skills, project documents, graph summary
- [x] Manifest `excluded_sources` includes `project_notepad`, `global_notepad`, and `implicit_memory`
- [x] Context size budget/truncation policy fields are present and explicit in manifest budget metadata

Evidence: `19-sprint39-manual.log`

## 39.2 Attempt Context Manifest

- [x] Immutable manifest and hashes are emitted via context events (`attempt_context_assembled`, `attempt_context_delivered`)
- [x] `attempt inspect --context` returns retry context, manifest object, `manifest_hash`, `inputs_hash`, and `delivered_context_hash`
- [x] Context override audit event emitted when include/exclude overrides are active (`attempt_context_overrides_applied`)

Evidence: `19-sprint39-manual.log`

## 39.3 Template Resolution and Retry Linkage

- [x] Template resolution is frozen at attempt start and reflected in manifest (`template_id`, resolved prompt/skill/document references)
- [x] Per-attempt include/exclude overrides are reflected in resolved manifest document set
- [x] Retry attempt manifest includes explicit `retry_links` to prior attempt manifest hash

Evidence: `19-sprint39-manual.log`

## 39.4 Determinism and Regression (`@hivemind-test`)

- [x] Repeated first attempts across equivalent flows produce identical `inputs_hash`
- [x] Fresh-clone `test_worktree.sh` smoke run passes with release binary
- [x] Fresh-clone `test_execution.sh` smoke run passes with release binary
- [x] Event store inspection confirms runtime/task observability remains intact

Evidence: `19-sprint39-manual.log`, `20-sprint39-ci-worktree.log`, `21-sprint39-ci-execution.log`, `22-sprint39-ci-events.log`

## 39.5 Exit Criteria Mapping

- [x] Attempt context assembly is deterministic and fully inspectable
- [x] Context replay reproduces identical manifests for identical inputs
- [x] Runtime initialization does not rely on hidden state
- [x] Manual validation in `@hivemind-test` is completed and documented
