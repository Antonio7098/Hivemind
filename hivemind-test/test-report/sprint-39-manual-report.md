# Sprint 39 Manual Test Report

Date: 2026-02-18
Scope: Phase 3 / Sprint 39 — Deterministic Agent Context Assembly

## Summary

Manual validation confirms Sprint 39 context assembly behavior is deterministic, explicit, and inspectable:

- attempts emit immutable context manifests with deterministic hash outputs
- context assembly explicitly excludes notepad/implicit-memory sources
- per-attempt include/exclude overrides are auditable and reflected in resolved document manifests
- retry attempts reference prior attempt manifests via explicit retry links
- `attempt inspect --context` exposes retry text + manifest + hash lineage for debugging
- fresh-clone `@hivemind-test` smoke runs remain stable

## Evidence Artifacts

- `19-sprint39-manual.log`
- `20-sprint39-ci-worktree.log`
- `21-sprint39-ci-execution.log`
- `22-sprint39-ci-events.log`
- `sprint-39-manual-checklist.md`

## Validations Performed

1. Deterministic first-attempt manifest input hashes
   - executed two equivalent first-attempt flows with identical governance context inputs
   - extracted `attempt_context_assembled.inputs_hash` for each flow
   - observed equality:
     - `flow1_inputs_hash=9448ad9e0801e254`
     - `flow2_inputs_hash=9448ad9e0801e254`

2. Attempt inspect context visibility
   - inspected retry attempt via `hivemind -f json attempt inspect <attempt-id> --context`
   - verified context payload includes:
     - `retry`
     - `manifest`
     - `manifest_hash`
     - `inputs_hash`
     - `delivered_context_hash`

3. Template resolution + override semantics
   - instantiated template with `doc1`
   - applied per-task override include `doc2` and exclude `doc1`
   - verified resolved manifest documents include `doc2` and omit `doc1`
   - verified `attempt_context_overrides_applied` event emitted with resolved document IDs

4. Retry manifest linkage
   - triggered retry on failed verification path
   - verified retry attempt manifest includes `retry_links` containing prior attempt ID and prior manifest hash

5. Fresh-clone regression smoke and event inspection
   - cloned repository to temp directory and built release binary
   - executed `hivemind-test/test_worktree.sh` and `hivemind-test/test_execution.sh`
   - inspected event store for runtime/task observability markers

## Exit Criteria Check

- [x] Attempt context assembly is deterministic and fully inspectable
- [x] Context replay reproduces identical manifests for identical inputs
- [x] Runtime initialization does not rely on hidden state
- [x] Manual validation in `@hivemind-test` is completed and documented
