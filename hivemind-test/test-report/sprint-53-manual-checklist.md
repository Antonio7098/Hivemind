# Sprint 53 Manual Checklist

Date: 2026-02-28
Owner: Antonio
Sprint: 53 (Sandbox and Approval Orchestrator)

## 53.1 Sandbox Policy Model

- [x] Validate explicit sandbox policy contract supports `read-only`, `workspace-write`, `danger-full-access`, and `host-passthrough`.
- [x] Validate platform-aware sandbox default selection and policy tagging surfaces in native tool-call traces/events.
- [x] Validate writable-root enforcement for `workspace-write` mode on `write_file`.

## 53.2 Approval Policy Model

- [x] Validate approval modes `never`, `on-failure`, `on-request`, and `unless-trusted` are selectable.
- [x] Validate explicit review decision path (`approve|deny`) and deterministic deny behavior.
- [x] Validate bounded `approved_for_session` cache semantics (`approval_review_decision:cached` on subsequent call).

## 53.3 Exec Policy and Dangerous Command Protection

- [x] Validate rules-backed exec policy manager gates `run_command` and denies by default unless allowlisted/amended.
- [x] Validate UNIX + Windows dangerous command analyzers produce deterministic policy failures.
- [x] Validate bounded prefix amendments and broad-prefix rejection (`*`, shell-wide prefixes) behavior.

## 53.4 Operational Verification

- [x] Run `hivemind-test/test_worktree.sh` against release binary.
- [x] Run `hivemind-test/test_execution.sh` against release binary.
- [x] Inspect canonical SQLite event evidence for runtime + filesystem observability.

## Artifacts

- `hivemind-test/test-report/2026-02-28-sprint53-test_worktree.log`
- `hivemind-test/test-report/2026-02-28-sprint53-test_execution.log`
- `hivemind-test/test-report/2026-02-28-sprint53-event-inspection.log`
- `hivemind-test/test-report/2026-02-28-sprint53-filesystem-inspection.log`
- `hivemind-test/test-report/sprint-53-manual-report.md`
