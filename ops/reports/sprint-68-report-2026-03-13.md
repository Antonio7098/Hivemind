# Sprint 68 Report: Conditional Steps, Wait Semantics, And Workflow Signals

## 1. Sprint Metadata

- **Sprint**: 68
- **Title**: Conditional Steps, Wait Semantics, And Workflow Signals
- **Branch**: `sprint/68-workflow-control-plane`
- **Date**: 2026-03-13
- **Owner**: Antonio

## 2. Objectives

- Add typed conditional-step evaluation over workflow context and append-only bag data only.
- Add durable wait-step semantics for signals, timers, and operator intervention.
- Make workflow signals explicit, attributable, idempotent, and replay-safe.
- Preserve human authority at control boundaries and recurse pause/resume/abort through workflow subtrees.
- Close the sprint with automated validation plus real CLI/manual smoke evidence, including a provider-backed OpenCode runtime exercise.

## 3. Delivered Changes

- Added typed workflow control-plane domain models for conditional branches, condition expressions, wait conditions/config, durable wait status, and workflow signals.
- Added new workflow event payloads for condition evaluation, wait activation/completion, and signal delivery, plus projection support for wait state and signal history.
- Extended workflow step definitions and runtime step/run state so conditional and wait behavior is fully event-sourced and inspectable.
- Added registry logic for typed condition evaluation, branch selection, non-selected subtree skipping, wait activation, signal matching, timer/signal wait completion, and explicit signal idempotency enforcement.
- Added subtree pause/resume/abort propagation for nested workflow runs.
- Extended the workflow CLI/API surface with `workflow signal` and authoring support for `--conditional-json` / `--wait-json`.
- Updated workflow observability labels/categories and workflow architecture/design docs for the new control-plane semantics.
- Added Sprint 68 automated coverage for conditional/wait state progression, duplicate signal rejection, and end-to-end CLI signal-driven completion.
- Captured manual Sprint 68 artifacts under `hivemind-test/sprints/phase-5/sprint-68/`, including error/edge-case smokes and real OpenCode runtime evidence.

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Conditional and wait semantics are explicit, typed, and replay-safe | PASS | typed workflow domain additions, explicit workflow events, projection support, unit/integration tests |
| 2 | Signals resume workflows without hidden state or scheduler ambiguity | PASS | `workflow signal`, durable `wait_status`, explicit signal/wait events, CLI/manual smoke |
| 3 | Human/operator interventions remain bounded and attributable | PASS | `human_signal` waits, signal provenance fields, pause/resume/abort subtree propagation, event inspection |
| 4 | Automated and manual smoke validation are completed and documented | PASS | targeted tests, formatting/lint/test/doc gates, Sprint 68 `@hivemind-test` artifacts, provider-backed runtime smoke |

**Overall: 4/4 criteria passed**

## 5. Validation

Executed during Sprint 68 closeout:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo doc --no-deps --document-private-items`
- `cargo test conditional_and_wait_steps_are_typed_and_signal_driven -- --nocapture`
- `cargo test duplicate_workflow_signal_idempotency_key_fails_loudly -- --nocapture`
- `cargo test --test integration cli_workflow_signal_drives_wait_step_completion -- --nocapture`

Results:

- formatting: PASS
- clippy (`-D warnings`): PASS
- full all-features test suite: PASS
- docs build: PASS
- focused Sprint 68 regression slices: PASS

## 6. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/sprints/phase-5/sprint-68/checklist.md`
- `hivemind-test/sprints/phase-5/sprint-68/report.md`
- `hivemind-test/sprints/phase-5/sprint-68/evidence/2026-03-13-workflow-control-smoke-transcript.log`
- `hivemind-test/sprints/phase-5/sprint-68/evidence/2026-03-13-workflow-control-edge-cases.log`
- `hivemind-test/sprints/phase-5/sprint-68/evidence/2026-03-13-opencode-nemotron-workflow-smoke.log`
- `hivemind-test/sprints/phase-5/sprint-68/evidence/2026-03-13-opencode-nemotron-complex-runtime-events.json`
- `hivemind-test/sprints/phase-5/sprint-68/evidence/2026-03-13-opencode-nemotron-minimal-runtime-events.json`

Manual checks confirmed:

- typed conditional branching and wait-for-signal flows work against a real repository fixture through the CLI
- pause/resume/abort and signal delivery remain visible and attributable in workflow-filtered event inspection
- negative control-plane cases fail loudly through the real CLI
- a real `opencode/nemotron-3-super-free` workflow-backed task can launch through Hivemind, emit runtime events, and complete a minimal checkpoint-only prompt successfully
- a more ambitious repo-writing prompt fails loudly and observably via `no_observable_progress_timeout`, proving the failure path remains explicit

## 7. Documentation Updates

Updated or added:

- `ops/roadmap/phase-5.md`
- `docs/architecture/workflow-foundation.md`
- `docs/design/workflow-control-plane.md`
- `ops/reports/sprint-68-report-2026-03-13.md`

## 8. Principle Checkpoint

- **Observability is truth**: condition inputs/results, wait activation/completion, and signal delivery are all explicit workflow events.
- **Fail loud**: invalid control-plane authoring and duplicate/invalid signal keys fail with named user errors; silent model behavior remains an explicit timeout.
- **Reliability over cleverness**: conditions consume typed context/bag data only, with no implicit free-text parsing or hidden last-writer-wins behavior.
- **CLI-first**: conditional/wait authoring and signal delivery are exposed directly via CLI and mirrored in API routes.
- **Automated checks mandatory**: targeted Sprint 68 coverage plus full closeout validation were completed before merge.
- **Human authority**: `human_signal` waits and subtree lifecycle commands keep operator intervention explicit and attributable.
- **No magic**: wait state, signal provenance, and branch selection are durable modelled state, not scheduler side channels.
