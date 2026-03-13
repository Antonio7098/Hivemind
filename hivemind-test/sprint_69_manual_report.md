# Sprint 69 Manual Report

Date: 2026-03-13

## Happy-path workflow E2E

Validated on a fresh temp git repository fixture through the workflow-native CLI surface:

- created project and attached repo
- configured worker runtime
- created workflow and workflow run
- started and ticked the workflow
- observed workflow-owned runtime stream, worktree inspection, and workflow-filtered events
- completed the workflow through the runtime checkpoint tool bridge
- executed `workflow merge-prepare`, `workflow merge-approve`, and `workflow merge-execute`
- verified the merged file landed on `main`

Artifact:

- `hivemind-test/test-report/2026-03-13-sprint69-workflow-e2e.log`

## Error and edge-case coverage

Validated explicit failures for the new workflow-owned surfaces:

- merge prepare before completion
- merge approve before prepare
- invalid workflow step for worktree inspection
- invalid workflow run for runtime stream
- re-ticking a completed workflow

Artifact:

- `hivemind-test/test-report/2026-03-13-sprint69-workflow-edges.log`

## Real opencode runtime run

Validated a real workflow task using:

- adapter: `opencode`
- model: `opencode/nemotron-3-super-free`

Observed:

- workflow-backed runtime start and exit
- real provider JSON mirrored through runtime output
- checkpoint completion from model output
- workflow step success and workflow completion

Artifact:

- `hivemind-test/test-report/2026-03-13-sprint69-real-opencode-nemotron.log`
