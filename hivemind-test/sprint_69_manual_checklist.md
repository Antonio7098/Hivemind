# Sprint 69 Manual Checklist

- [x] Create a fresh temp git repository fixture and attach it to a Hivemind project.
- [x] Create a workflow, add a task step, create a workflow run, and start it through the CLI.
- [x] Exercise workflow-owned inspection surfaces:
  - `workflow worktree-list`
  - `workflow worktree-inspect`
  - `workflow runtime-stream`
- [x] Confirm API projections accept `workflow_run_id` for runtime stream and worktree inspection paths.
- [x] Complete a full workflow-native verification and merge prepare/approve/execute smoke against a real successful task execution path.
- [x] Validate workflow-owned error and edge cases:
  - merge prepare before completion fails loudly
  - merge approve before prepare fails loudly
  - invalid workflow run selectors fail loudly
  - invalid workflow step worktree inspection fails loudly
  - re-ticking a completed workflow fails with `workflow_run_not_running`
- [x] Run a real `opencode` workflow task using `opencode/nemotron-3-super-free` and capture runtime/event evidence.
