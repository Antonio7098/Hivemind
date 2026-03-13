---
title: Runtime Stream UI Integration
description: Frontend guidance for projected runtime stream APIs and turn-ref recovery
order: 16
---

# Runtime Stream UI Integration

This guide describes the UI-facing runtime stream surfaces exposed by the server.

## Endpoints

### Snapshot API

```text
GET /api/runtime-stream?flow_id=<uuid>&attempt_id=<uuid>&limit=<n>
```

- Returns a bounded array of projected runtime stream items.
- `flow_id` and `attempt_id` are optional filters; provide one or both.
- Items are ordered by ascending event sequence.

### Live SSE API

```text
GET /api/runtime-stream/stream?flow_id=<uuid>&attempt_id=<uuid>
```

- Returns `text/event-stream`.
- Emits `event: runtime` frames.
- The connection starts with a keepalive comment (`: connected`).

## Envelope shape

Every SSE frame serializes a JSON envelope:

```json
{
  "cursor": 184,
  "item": {
    "event_id": "...",
    "sequence": 184,
    "timestamp": "2026-03-08T12:34:56Z",
    "flow_id": "...",
    "task_id": "...",
    "attempt_id": "...",
    "kind": "turn",
    "stream": "stdout",
    "title": "opencode turn 3",
    "text": "optional summary",
    "data": { "ordinal": 3, "git_ref": "refs/..." }
  }
}
```

## Common `item.kind` values

- `output_chunk`
- `narrative`
- `tool_call`
- `todo_snapshot`
- `command`
- `file_change`
- `session`
- `turn`
- `runtime_started`
- `runtime_exited`
- `checkpoint_declared`
- `checkpoint_completed`
- `checkpoint_commit_created`

The `data` object is intentionally kind-specific; UIs should branch on `kind` rather than depend on the full raw event taxonomy.

## Projection mapping

The runtime-stream API is a projection over lower-level event types. Current important mappings are:

- `command` ← `RuntimeCommandCompleted`
  - authoritative command completion for structured JSON runtimes
  - `data.command`, `data.exit_code`, `data.output`
- `tool_call` ← `RuntimeToolCallObserved`
  - normalized tool/MCP/collaboration/web-search activity
- `todo_snapshot` ← `RuntimeTodoSnapshotUpdated`
  - current todo list snapshot derived from normalized runtime output
- `narrative` ← `RuntimeNarrativeOutputObserved`
  - assistant/operator-facing status narration
- `session` ← `RuntimeSessionObserved`
  - provider session identity used for retry/session resume
- `turn` ← `RuntimeTurnCompleted`
  - per-turn metadata including `ordinal`, `provider_session_id`, `provider_turn_id`, `git_ref`, `commit_sha`, and optional `summary`
- `file_change` ← `RuntimeFilesystemObserved`
  - repository-level file deltas observed by Hivemind around runtime execution

`output_chunk` remains the raw normalized stdout/stderr stream. Raw provider JSON is preserved in attempt stderr/output capture with tags like `[opencode.json]` and `[codex.json]`, but those mirror lines are not exposed as their own dedicated runtime-stream `kind`.

## Suggested frontend pattern

1. Load a recent snapshot from `/api/runtime-stream`.
2. Record the latest `cursor`/`sequence` rendered.
3. Open `EventSource` against `/api/runtime-stream/stream` with the same filters.
4. Append only items whose `sequence` is newer than the last rendered item.

Minimal browser example:

```js
const source = new EventSource(`/api/runtime-stream/stream?attempt_id=${attemptId}`);
source.addEventListener("runtime", (event) => {
  const envelope = JSON.parse(event.data);
  renderRuntimeItem(envelope.item);
});
```

## Attempt inspection and restore UX

`GET /api/attempts/inspect?id=<attempt-id>` now exposes:

- `runtime_session`
- `turn_refs[]`

Each `turn_refs[]` entry contains:

- `ordinal`
- `adapter_name`
- `stream`
- `provider_session_id`
- `provider_turn_id`
- `git_ref`
- `commit_sha`
- `summary`

Recommended UI usage:

- show `runtime_session.session_id` as the provider resume handle
- show `turn_refs[]` as a recovery timeline / restore menu
- link `turn.kind === "turn"` runtime stream items back to the corresponding `turn_refs[]` ordinal

## Turn restore API

To restore a transient turn snapshot into the task worktree:

```text
POST /api/worktrees/restore-turn
```

Body:

```json
{
  "attempt_id": "<attempt-id>",
  "ordinal": 3,
  "confirm": true,
  "force": false
}
```

Notes:

- `confirm` is mandatory because restore is destructive to current worktree contents.
- If the owning flow is still running, `force` is required.
- Restore resets worktree contents to the stored turn snapshot but does **not** move the task branch HEAD.