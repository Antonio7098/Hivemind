# Sprint 69 Report: Workflow-Native Runtime, Merge, Worktree, And Public Surface Cutover

## 1. Sprint Metadata

- **Sprint**: 69
- **Title**: Workflow-Native Runtime, Merge, Worktree, And Public Surface Cutover
- **Date**: 2026-03-13
- **Owner**: Antonio

## 2. Delivered Changes

- Added workflow-run-owned registry wrappers for runtime stream, worktree list/inspect/cleanup, and merge prepare/approve/execute so operators can drive those surfaces with `workflow_run_id`.
- Extended worktree inspection payloads with workflow lineage (`workflow_id`, `workflow_run_id`, `step_id`, `step_run_id`) while preserving legacy `flow_id`/`task_id` compatibility.
- Promoted new `workflow` CLI subcommands for runtime stream, worktree inspection/cleanup, and merge execution.
- Updated API query and operation routes so runtime stream, worktrees, and merge endpoints accept `workflow_run_id` directly.
- Updated UI state assembly to expose `workflows` and `workflow_runs` as first-class projected state alongside legacy flows.
- Updated workflow architecture, quickstart, and CLI semantics docs to describe workflows as the primary execution surface and `flow/*` as transitional compatibility.

## 3. Automated Validation

Executed:

- `cargo check --all-features`
- `cargo fmt --all`
- `cargo fmt --all --check`
- `cargo test --all-features workflow_run_id -- --nocapture`

Results:

- compile: PASS
- formatting: PASS
- targeted workflow-owned API regression slice: PASS

## 4. Manual Validation

Artifacts:

- `hivemind-test/sprint_69_manual_checklist.md`
- `hivemind-test/sprint_69_manual_report.md`
- `hivemind-test/test-report/2026-03-13-sprint69-workflow-e2e.log`
- `hivemind-test/test-report/2026-03-13-sprint69-workflow-edges.log`
- `hivemind-test/test-report/2026-03-13-sprint69-real-opencode-nemotron.log`

Manual smoke completed:

- full workflow-owned happy path: create/start/tick/runtime/checkpoint/workflow completion/merge prepare/approve/execute
- workflow-owned CLI inspection for worktrees and runtime stream
- API inspection via `workflow_run_id` selectors
- negative and edge-case coverage for workflow-owned merge/worktree/runtime selectors
- real `opencode` workflow execution using `opencode/nemotron-3-super-free`

## 5. Exit Criteria Status

| Criterion | Status |
|-----------|--------|
| Workflow is the primary execution surface for new runs | PASS |
| Runtime, worktree, and merge subsystems operate under workflow-native identity | PASS |
| Public CLI/API/docs are aligned with the workflow-first model | PASS |
| Automated and manual smoke validation are completed and documented | PASS |

## 6. Documentation Updated

- `docs/architecture/workflow-foundation.md`
- `docs/overview/quickstart.md`
- `docs/design/cli-semantics.md`
- `ops/roadmap/phase-5.md`
- `hivemind-test/sprint_69_manual_checklist.md`
- `hivemind-test/sprint_69_manual_report.md`

## 7. Principle Checkpoint

- **Observability is truth**: workflow-owned runtime/worktree/merge surfaces still resolve through explicit correlation and projected state.
- **Fail loud**: workflow-owned merge and worktree APIs reject missing owners and invalid workflow selectors explicitly.
- **Reliability over cleverness**: the synthetic flow remains an internal bridge; public ownership is cut over without hidden migration.
- **CLI-first**: workflow-native commands now exist for runtime stream, worktree, and merge operations.
- **No magic**: legacy `flow/*` remains documented as compatibility behavior rather than being silently removed.
