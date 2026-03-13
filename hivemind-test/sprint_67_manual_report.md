# Sprint 67 Manual Report

- Date: 2026-03-13
- Fixture: `@hivemind-test` real repository workspace

## Smoke Path

- Created parent and child workflows through the workflow CLI surface.
- Launched parent workflow steps that spawned child runs with explicit copy-in inputs.
- Verified parent `workflow status` output showed child run summaries and parent lineage fields.
- Verified child completion mapped declared child context keys back into the parent output bag and parent context.
- Exercised nested timeout behavior with an OpenCode wrapper that delayed only the runtime `run` path; parent workflow completed with the child step marked `failed`, and child runtime events included observable timeout classification.
- Exercised a real OpenCode nested workflow using model `opencode/nemotron-3-super-free`; after completing the child checkpoint, both child and parent workflow runs reached `completed` and emitted `runtime_started`, `runtime_exited`, `checkpoint_completed`, and `workflow_run_completed`.

## Failure / Retry Notes

- Confirmed child lineage remains queryable through event filtering using `workflow_run_id`, `root_workflow_run_id`, and `parent_workflow_run_id`.
- Confirmed retry remains explicit at the step boundary; no hidden cross-run recovery state was introduced.
- Found and fixed an OpenCode adapter bug where runtime health probes (`--version` / `--help`) ignored configured timeouts and could block nested timeout smokes before execution started.

## Result

- Sprint 67 nested workflow smoke path completed successfully.
