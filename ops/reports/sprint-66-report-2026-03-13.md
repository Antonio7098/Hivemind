# Sprint 66 Report: Workflow Context, Step Inputs, And Append-Only Output Bag

## 1. Sprint Metadata

- **Sprint**: 66
- **Title**: Workflow Context, Step Inputs, And Append-Only Output Bag
- **Branch**: `sprint/66-workflow-context-output-bag`
- **Date**: 2026-03-13
- **Owner**: Antonio

## 2. Objectives

- Add a typed workflow-scoped context model with explicit initialization and deterministic snapshot hashing.
- Add deterministic step input resolution and an append-only workflow output bag for branch fan-in.
- Inject workflow-derived hashes into the immutable attempt manifest while preserving the Sprint 65 execution bridge.
- Complete automated validation, docs, roadmap updates, and manual CLI smoke evidence.

## 3. Delivered Changes

- Added typed workflow values, workflow context state/snapshots, step input bindings, reducer/selector contracts, output bag entries, and step-context snapshots.
- Added workflow event payloads for context initialization, context snapshot capture, step input resolution, and output-bag append.
- Added replay/projection support for the new workflow data-plane events.
- Extended attempt-context manifest support to `attempt_context.v3` with workflow-derived manifest metadata.
- Extended workflow CLI/API authoring and run-create input handling for context inputs, step bindings, output bindings, and context patches.
- Added registry logic for workflow context initialization, deterministic step input resolution, output-bag append, context snapshot capture, workflow-derived attempt-context injection, and join-step native execution.
- Added integration coverage for join fan-in execution and workflow-derived attempt manifests.
- Added manual smoke artifacts for positive and negative Sprint 66 workflow data-plane paths.
- Added a shared external-runtime directive bridge so non-native runtimes can satisfy built-in checkpoint directives from emitted `ACT:tool:checkpoint_complete:{...}` lines.
- Added a shared external-runtime no-progress watchdog and fail-loud classification path for silent remote attempts.
- Forced synthetic workflow join steps onto the native runtime so join fan-in remains workflow-native even when branch steps use external runtimes.
- Fixed deterministic reducer ordering so fan-in follows declared `producer_step_ids` rather than output append timing.

## 4. Exit Criteria Results

| # | Criterion | Status | Verification |
|---|-----------|--------|--------------|
| 1 | Workflow context is evented, inspectable, and replay-safe | PASS | new workflow events, replay/apply support, `workflow status`, workflow-focused tests |
| 2 | Parallel branches write only through append-only bag semantics | PASS | output-bag append path, reducer-based join consumption, integration/manual join smoke |
| 3 | Downstream step inputs are deterministic and hashable | PASS | `WorkflowStepContextSnapshot`, attempt manifest workflow section, integration/manual hash inspection |
| 4 | Automated and manual smoke validation are completed and documented | PASS | `cargo check`, `cargo test workflow_`, workflow integration slice, Sprint 66 manual artifacts |

**Overall: 4/4 criteria passed**

## 5. Validation

Executed during Sprint 66 finalization:

- `CARGO_TARGET_DIR=/tmp/hivemind-target cargo check`
- `CARGO_TARGET_DIR=/tmp/hivemind-target cargo test workflow_ --quiet`
- `CARGO_TARGET_DIR=/tmp/hivemind-target cargo test --test integration workflow_tick`
- `CARGO_TARGET_DIR=/tmp/hivemind-target cargo test --test integration cli_flow_tick_auto_completes_checkpoint_from_external_runtime_directive --quiet`
- `CARGO_TARGET_DIR=/tmp/hivemind-target cargo test reconcile_workflow_bridge_for_flow_completes_join_after_branch_recovery --quiet`

Results:

- `cargo check`: PASS
- `cargo test workflow_ --quiet`: PASS
- `cargo test --test integration workflow_tick`: PASS
- `cargo test --test integration cli_flow_tick_auto_completes_checkpoint_from_external_runtime_directive --quiet`: PASS
- `cargo test reconcile_workflow_bridge_for_flow_completes_join_after_branch_recovery --quiet`: PASS

## 6. Manual Validation (`@hivemind-test`)

Artifacts:

- `hivemind-test/sprint_66_manual_checklist.md`
- `hivemind-test/sprint_66_manual_report.md`

Manual checks confirmed:

- a real repository fixture can execute a parallel-branch workflow with join fan-in
- workflow context revisions and hashes are visible through `workflow status`
- output bag contents and join-produced outputs are inspectable through the CLI
- workflow-backed attempt manifests include the new `workflow` section and hash references
- duplicate single-producer and schema mismatch cases fail explicitly through the real CLI
- real `opencode/nemotron-3-super-free` attempts receive Sprint 66 workflow step context and can auto-complete workflow checkpoints through the shared non-native directive bridge
- silent external attempts fail loudly with `no_observable_progress_timeout` instead of hanging without observable progress

## 7. Documentation Updates

Updated or added:

- `ops/roadmap/phase-5.md`
- `docs/overview/principles.md`
- `docs/architecture/workflow-foundation.md`
- `docs/design/workflow-context-data-plane.md`
- `ops/process/sprint-execution.md`
- `ops/reports/sprint-66-report-2026-03-13.md`
- `hivemind-test/sprint_66_manual_checklist.md`
- `hivemind-test/sprint_66_manual_report.md`

## 8. Principle Checkpoint

- **Observability is truth**: workflow context, step inputs, and output-bag mutations are all evented and inspectable.
- **Fail loud**: reducer conflicts and schema mismatches fail explicitly instead of silently choosing a branch winner.
- **Fail loud**: silent non-native runtimes now terminate with an explicit watchdog error instead of consuming minutes without observable progress.
- **Reliability over cleverness**: the data plane is layered onto the existing bridge with explicit hashes and no hidden mutable memory.
- **Reliability over cleverness**: external-runtime checkpoint handling now lives at the orchestration layer instead of relying on adapter-specific shell tricks.
- **Automated checks mandatory**: workflow-focused unit and integration validation plus manual smoke evidence were completed before closeout.
- **No magic**: workflow context, step context, and attempt context are documented as separate explicit layers with visible hash boundaries.
