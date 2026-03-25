# Frontend Integration: Points of Contact Checklist

## Projects

- [ ] **Create project** - `POST /api/projects/create` → `src/server/routes/projects.rs:11`
- [ ] **List projects** - `GET /api/projects` → `src/server/routes/queries/state.rs:26`
- [ ] **Inspect single project** - via `GET /api/projects` with filter → `src/app.rs:378` (`get_project`)
- [ ] **Update project** - `POST /api/projects/update` → `src/server/routes/projects.rs:17`
- [ ] **Delete project** - `POST /api/projects/delete` → `src/server/routes/projects.rs:25`
- [ ] **Attach repository to project** - `POST /api/projects/repos/attach` → `src/server/routes/projects.rs:64`
- [ ] **Detach repository from project** - `POST /api/projects/repos/detach` → `src/server/routes/projects.rs:74`

## Tasks

- [ ] **Create task** - `POST /api/tasks/create` → `src/server/routes/tasks.rs:10`
- [ ] **List tasks** - `GET /api/tasks` → `src/server/routes/queries/state.rs:27`
- [ ] **Update task title/description** - `POST /api/tasks/update` → `src/server/routes/tasks.rs:19`
- [ ] **Delete task** - `POST /api/tasks/delete` → `src/server/routes/tasks.rs:27`
- [ ] **Close task** - `POST /api/tasks/close` → `src/server/routes/tasks.rs:33`
- [ ] **Start task execution** - `POST /api/tasks/start` → `src/server/routes/tasks.rs:37` (returns `attempt_id`)
- [ ] **Complete task execution** - `POST /api/tasks/complete` → `src/server/routes/tasks.rs:43`
- [ ] **Retry task** - `POST /api/tasks/retry` → `src/server/routes/tasks.rs:47`
- [ ] **Abort task** - `POST /api/tasks/abort` → `src/server/routes/tasks.rs:55`
- [ ] **Set task run mode** - `POST /api/tasks/run-mode` → `src/server/routes/tasks.rs:78`

## Graphs

- [ ] **Create graph from tasks** - `POST /api/graphs/create` → `src/server/routes/graphs.rs:10`
- [ ] **List graphs** - `GET /api/graphs` → `src/server/routes/queries/state.rs:28`
- [ ] **Delete graph** - `POST /api/graphs/delete` → `src/server/routes/graphs.rs:27`
- [ ] **Add dependency between tasks** - `POST /api/graphs/dependencies/add` → `src/server/routes/graphs.rs:33`
- [ ] **Add check to task** - `POST /api/graphs/checks/add` → `src/server/routes/graphs.rs:42`
- [ ] **Validate graph** - `POST /api/graphs/validate` → `src/server/routes/graphs.rs:53`
- [ ] **Refresh graph snapshot** - `POST /api/governance/graph-snapshot/refresh` → `src/server/routes/governance.rs:68`

## Flows

- [ ] **Create flow from graph** - `POST /api/flows/create` → `src/server/routes/flows.rs:10`
- [ ] **List flows** - `GET /api/flows` → `src/server/routes/queries/state.rs:29`
- [ ] **Delete flow** - `POST /api/flows/delete` → `src/server/routes/flows.rs:14`
- [ ] **Start flow** - `POST /api/flows/start` → `src/server/routes/flows.rs:20`
- [ ] **Tick flow** (step execution) - `POST /api/flows/tick` → `src/server/routes/flows.rs:24`
- [ ] **Pause flow** - `POST /api/flows/pause` → `src/server/routes/flows.rs:32`
- [ ] **Resume flow** - `POST /api/flows/resume` → `src/server/routes/flows.rs:36`
- [ ] **Abort flow** - `POST /api/flows/abort` → `src/server/routes/flows.rs:40`
- [ ] **Set flow run mode** - `POST /api/flows/run-mode` → `src/server/routes/flows.rs:48`
- [ ] **Add flow dependency** - `POST /api/flows/dependencies/add` → `src/server/routes/flows.rs:55`
- [ ] **Replay flow** - `GET /api/flows/replay` → `src/server/routes/queries/other.rs` (replay_flow)

## Runtime Configuration

- [ ] **Set project runtime** - `POST /api/projects/runtime` → `src/server/routes/projects.rs:31`
- [ ] **Set flow runtime** - `POST /api/flows/runtime` → `src/server/routes/flows.rs:62`
- [ ] **Set task runtime** - `POST /api/tasks/runtime` → `src/server/routes/tasks.rs:59`
- [ ] **Set runtime defaults** - `POST /api/runtime/defaults` → `src/server/routes/projects.rs:47`
- [ ] **List runtimes** - `GET /api/runtimes` → `src/server/routes/queries/runtime.rs:11`
- [ ] **Check runtime health** - `GET /api/runtimes/health` → `src/server/routes/queries/runtime.rs:12`
- [ ] **Stream runtime events** - `GET /api/runtime-stream` → `src/server/routes/queries/mod.rs:63`

## Chat / Conversation

- [ ] **Invoke one-shot chat** - `POST /api/chat/invoke` → `src/server/routes/chat.rs:45`
- [ ] **Create persistent chat session** - `POST /api/chat/sessions/create` → `src/server/routes/chat.rs:49`
- [ ] **List chat sessions** - `GET /api/chat/sessions` → `src/server/routes/chat.rs:30`
- [ ] **Inspect chat session** - `GET /api/chat/sessions/inspect` → `src/server/routes/chat.rs:31`
- [ ] **Send message to session** - `POST /api/chat/sessions/send` → `src/server/routes/chat.rs:53`

## Constitution

- [x] **Show constitution** - `GET /api/governance/constitution` → `src/server/routes/governance.rs:6`
- [ ] **Check constitution compliance** - `POST /api/governance/constitution/check` → `src/server/routes/governance.rs:63`

## Governance Documents

- [x] **List project governance documents** - `GET /api/governance/documents` → `src/server/routes/governance.rs:11`
- [x] **Inspect governance document** - `GET /api/governance/documents/inspect` → `src/server/routes/governance.rs:16`

## Notepad

- [x] **Show project notepad** - `GET /api/governance/notepad` → `src/server/routes/governance.rs:24`
- [x] **Show global notepad** - `GET /api/governance/global/notepad` → `src/server/routes/governance.rs:29`

## Skills (Global Governance)

- [ ] **List global skills** - `GET /api/governance/global/skills` → `src/server/routes/governance.rs:32`
- [ ] **Inspect global skill** - `GET /api/governance/global/skills/inspect` → `src/server/routes/governance.rs:35`

## Templates (Global Governance)

- [ ] **List global templates** - `GET /api/governance/global/templates` → `src/server/routes/governance.rs:40`
- [ ] **Inspect global template** - `GET /api/governance/global/templates/inspect` → `src/server/routes/governance.rs:43`

## Governance Snapshots & Repair

- [ ] **Create governance snapshot** - CLI only → `src/cli/handlers/project/governance/snapshot.rs`
- [ ] **List governance snapshots** - CLI only → `src/cli/handlers/project/governance/snapshot.rs`
- [ ] **Restore governance snapshot** - CLI only → `src/cli/handlers/project/governance/snapshot.rs`
- [ ] **Detect governance issues** - CLI only → `src/cli/handlers/project/governance/repair.rs`
- [ ] **Apply governance repair** - CLI only → `src/cli/handlers/project/governance/repair.rs`

## Verification

- [ ] **Run verification on task** - `POST /api/verify/run` → `src/server/routes/operations.rs:21`
- [ ] **Override verification decision** - `POST /api/verify/override` → `src/server/routes/operations.rs:13`
- [ ] **Get verification results** - `GET /api/verify/results` → `src/server/routes/queries/other.rs`

## Merge

- [ ] **Prepare merge for flow** - `POST /api/merge/prepare` → `src/server/routes/operations.rs:25`
- [ ] **Approve merge** - `POST /api/merge/approve` → `src/server/routes/operations.rs:29`
- [ ] **Execute merge** - `POST /api/merge/execute` → `src/server/routes/operations.rs:33`
- [ ] **List merge states** - `GET /api/merges` → `src/server/routes/queries/state.rs:30`

## Checkpoints

- [ ] **List checkpoints for attempt** - `src/app.rs:894` (`list_checkpoints`)
- [ ] **Complete checkpoint** - `POST /api/checkpoints/complete` → `src/server/routes/operations.rs:45`

## Worktrees

- [ ] **List worktrees for flow** - `GET /api/worktrees` → `src/server/routes/queries/other.rs`
- [ ] **Inspect worktree** - `GET /api/worktrees/inspect` → `src/server/routes/queries/other.rs`
- [ ] **Cleanup worktrees** - `POST /api/worktrees/cleanup` → `src/server/routes/operations.rs:54`
- [ ] **Restore turn** - `POST /api/worktrees/restore-turn` → `src/server/routes/operations.rs:62`

## Events

- [x] **Query events** - `GET /api/events` → `src/server/routes/queries/events.rs:12`
- [x] **Inspect single event** - `GET /api/events/inspect` → `src/server/routes/queries/events.rs:20`
- [ ] **Replay flow from events** - `src/server/routes/queries/other.rs` (replay_flow)

## Attempts

- [ ] **List attempts** - `GET /api/attempts/inspect` → `src/server/routes/queries/attempts.rs`
- [ ] **Inspect attempt** - `GET /api/attempts/inspect` → `src/server/routes/queries/attempts.rs`
- [ ] **Get attempt diff** - `GET /api/attempts/diff` → `src/server/routes/queries/attempts.rs`

## UI State

- [ ] **Get full UI state snapshot** - `GET /api/state` → `src/server/routes/queries/state.rs:14`
- [ ] **Get version** - `GET /api/version` → `src/server/routes/queries/mod.rs:59`
- [ ] **Get API catalog** - `GET /api/catalog` → `src/server/routes/queries/mod.rs:62`
- [ ] **Health check** - `GET /health` → `src/server/routes/queries/mod.rs:54`

## Spec Trees (Sprint 71)

- [ ] **TODO: Workflow spec tree binding** - Sprint 71 spec binding UI not yet implemented. Requires spec tree API endpoints from backend. See `ops/roadmap/phase-5.md:71.1-71.7`
- [ ] **Inspect workflow spec** - `GET /api/workflows/spec` (not yet exposed)
- [ ] **Inspect task spec** - `GET /api/tasks/spec` (not yet exposed)

## Repository Explorer

- [ ] **TODO: Project repository explorer** - FileTree component (rename DocumentTree to FileTree for reuse). VS Code-style file explorer for project repos, renders code files with syntax highlighting. Reuses DocumentViewer pattern. See `src/components/common/FileTree.tsx` (to be created)

---

## CLI-Only Functionality (not exposed via HTTP API)

These require CLI invocation or need to be added to the API:

- **Constitution init/update** → `src/cli/handlers/project/governance/core.rs`
- **Project governance init/migrate/inspect/diagnose/replay** → `src/cli/handlers/project/governance/`
- **Governance document create/update/delete** → `src/cli/handlers/project/governance/document.rs`
- **Governance notepad create/update/delete** → `src/cli/handlers/project/governance/notepad.rs`
- **Global skill create/update/delete** → `src/cli/handlers/global/skills.rs`
- **Global system prompt operations** → `src/cli/handlers/global/dispatch/`
- **Global template create/delete/instantiate** → `src/cli/handlers/global/dispatch/template.rs`
- **Global notepad create/delete** → `src/cli/handlers/global/`
- **Task runtime clear** → `src/cli/handlers/task/`
- **Graph query** → `src/cli/handlers/graph/query.rs`
