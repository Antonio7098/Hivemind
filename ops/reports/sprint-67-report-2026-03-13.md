# Sprint 67 Report: Nested Workflows, Inheritance, And Lineage

## 1. Sprint Metadata

- **Sprint**: 67
- **Title**: Nested Workflows, Inheritance, And Lineage
- **Branch**: `sprint/67-nested-workflows-lineage`
- **Date**: 2026-03-13
- **Owner**: Antonio

## 2. Objectives

- Add `workflow` steps that launch child workflow runs with explicit parent-to-child input bindings.
- Make nested parent/child lineage directly inspectable from CLI, API, and event views.
- Preserve explicit failure and retry behavior across child workflow boundaries without hidden recovery state.
- Close Sprint 67 with automated regression coverage and manual CLI smoke evidence.

## 3. Delivered Changes

- Added a workflow-run inspect payload with a recursive `child_runs` tree and explicit `workflow_name` / `parent_step_name` lineage metadata.
- Moved `workflow status` JSON and `/api/workflow-runs/inspect` onto the same registry-built inspect contract so CLI table output, CLI JSON, and API inspection no longer diverge.
- Updated CLI event filtering so `events list --workflow-run <root-run-id>` resolves root workflow runs to `root_workflow_run_id` lineage filtering and includes nested child events by default.
- Added regression coverage for nested CLI status inspection and API workflow-run inspection.
- Refreshed Sprint 67 docs, roadmap state, manual report text, and changelog metadata to reflect the final inspectability contract.

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Child workflows can be launched and observed deterministically | PASS | existing nested-workflow execution path plus `workflow_run_inspect_endpoint_includes_nested_lineage_tree` |
| 2 | Parent/child input-output mappings are explicit and replay-safe | PASS | existing Sprint 67 workflow registry tests and workflow-focused regression suite |
| 3 | Nested lineage is inspectable from CLI/API/events without guesswork | PASS | new CLI status regression, API inspect regression, and root-run event filter validation |
| 4 | Automated and manual smoke validation are completed and documented | PASS | targeted Rust validation plus `hivemind-test/test-report/2026-03-13-sprint67-nested-lineage.log` and updated Sprint 67 manual report |

**Overall: 4/4 criteria passed**

## 5. Validation

Executed during Sprint 67 closeout:

- `cargo fmt --all`
- `cargo test workflow_ --quiet`
- `cargo test workflow_run_inspect_endpoint_includes_nested_lineage_tree --quiet`
- `cargo test --test integration cli_workflow_status_json_includes_nested_child_tree --quiet`
- `cargo test --test integration cli_workflow_tick_rejects_unsupported_step_kinds --quiet`

Results:

- `cargo fmt --all`: PASS
- `cargo test workflow_ --quiet`: PASS
- `cargo test workflow_run_inspect_endpoint_includes_nested_lineage_tree --quiet`: PASS
- `cargo test --test integration cli_workflow_status_json_includes_nested_child_tree --quiet`: PASS
- `cargo test --test integration cli_workflow_tick_rejects_unsupported_step_kinds --quiet`: PASS

## 6. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/sprint_67_manual_checklist.md`
- `hivemind-test/sprint_67_manual_report.md`
- `hivemind-test/test-report/2026-03-13-sprint67-nested-lineage.log`

Manual checks confirmed:

- a real repository fixture can launch a parent workflow that spawns child workflow runs through the CLI
- `workflow status` JSON exposes a recursive `child_runs` tree with workflow and parent-step names
- `events list --workflow-run <root-run-id>` now includes child workflow events via root-lineage filtering
- nested lineage remains attributable through `workflow_run_id`, `root_workflow_run_id`, and `parent_workflow_run_id`

## 7. Documentation Updates

Updated or added:

- `ops/roadmap/phase-5.md`
- `docs/architecture/workflow-foundation.md`
- `docs/design/nested-workflow-lineage.md`
- `hivemind-test/sprint_67_manual_report.md`
- `ops/reports/sprint-67-report-2026-03-13.md`
- `changelog.json`

## 8. Principle Checkpoint

- **Observability is truth**: nested workflow inspection now exposes an explicit tree instead of relying on table-only synthesis or raw event reconstruction.
- **Fail loud**: root-run event inspection now follows explicit lineage filtering, removing a silent inspection gap for nested child events.
- **Reliability over cleverness**: one registry-built inspect payload now drives CLI and API output, reducing drift between surfaces.
- **CLI-first**: the full nested lineage view is available from `workflow status` and `events list` without depending on UI-only projections.
- **Automated checks mandatory**: workflow-focused Rust validation and a real-repo CLI smoke were completed before closeout.
- **No magic**: parent/child names, root ids, parent ids, and parent step names are rendered directly in the inspect payload.
