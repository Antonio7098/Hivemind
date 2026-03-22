# Sprint 67 Manual Checklist

- Create two child workflows and one parent workflow in a real repository fixture.
- Add parent `workflow` steps with explicit `--child-workflow-id` bindings and child-context output mappings.
- Start the parent run and tick until both child runs complete.
- Inspect `hivemind workflow status <parent-run-id>` and confirm child run summaries appear.
- Inspect `hivemind events list --workflow-run <parent-run-id>` and confirm `root_workflow_run_id`, `parent_workflow_run_id`, and `step_id` lineage.
- Exercise a child failure path and verify parent behavior follows the configured terminal policy.
- Exercise a retry path through explicit step state changes and confirm a new child run is launched without hidden recovery state.
