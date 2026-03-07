# CodeGraph DB Schema

## Purpose

Define the DB-backed persistence model that replaces Hivemind's legacy file-based graph snapshot projection (`projects/<project-id>/graph_snapshot.json`) as the primary runtime substrate for CodeGraph.

## Core Decisions

- Canonical event authority remains the event store (`db.sqlite` + JSONL mirror).
- Canonical CodeGraph runtime state lives in the native runtime state DB (`runtime-state.sqlite` or equivalent configured path), not in governance JSON files.
- Hivemind imports CodeGraph extraction/query behavior from the local UCP integration; Hivemind owns persistence, freshness tracking, session state, and migration only.
- Hivemind must **not** implement a new CodeGraph engine from scratch; the local UCP CodeGraph implementation is the sole extraction/query backend and Hivemind only wraps it with DB-backed runtime state.
- Any remaining file projection is transitional/derivative and must not be treated as the source of truth.

## Scope

This schema stores:

- graph snapshot registry metadata
- per-repository CodeGraph payloads and stats
- active project/workspace bindings to the current snapshot
- navigation session state for prompt assembly and resume
- dirty-path invalidation and refresh job lifecycle

This schema does **not** reimplement CodeGraph extraction, validation, or traversal semantics.

## Storage Location

Use the existing native runtime state SQLite DB and migration mechanism described in `docs/design/runtime-wrapper.md` and implemented under `src/native/runtime_hardening/storage/`.

## Tables

### `codegraph_snapshots`

One logical CodeGraph snapshot per project/workspace refresh result.

- `snapshot_id TEXT PRIMARY KEY`
- `project_id TEXT NOT NULL`
- `workspace_id TEXT`
- `snapshot_schema_version TEXT NOT NULL` — Hivemind persistence schema version, e.g. `codegraph_snapshot.v1`
- `ucp_profile_version TEXT NOT NULL`
- `ucp_engine_version TEXT NOT NULL`
- `canonical_fingerprint TEXT NOT NULL`
- `extractor_config_json TEXT NOT NULL`
- `repo_state_hash TEXT` — optional combined repo/worktree identity
- `source_kind TEXT NOT NULL` — `fresh_build|incremental_refresh|legacy_import`
- `trigger TEXT NOT NULL`
- `freshness_state TEXT NOT NULL` — `fresh|stale|refreshing|superseded|failed`
- `previous_snapshot_id TEXT`
- `created_at_ms INTEGER NOT NULL`
- `activated_at_ms INTEGER`
- `superseded_at_ms INTEGER`
- `failed_at_ms INTEGER`
- `failure_json TEXT`

Indexes:

- `(project_id, created_at_ms DESC)`
- `(project_id, canonical_fingerprint)`
- `(freshness_state)`

### `codegraph_snapshot_repositories`

One row per repository included in a snapshot.

- `snapshot_id TEXT NOT NULL`
- `repo_ordinal INTEGER NOT NULL`
- `repo_name TEXT NOT NULL`
- `repo_path TEXT NOT NULL`
- `commit_hash TEXT NOT NULL`
- `repo_fingerprint TEXT`
- `stats_json TEXT NOT NULL`
- `document_json TEXT NOT NULL` — authoritative persisted portable CodeGraph document for this repo
- `structure_projection TEXT` — optional prompt-friendly derived projection
- `created_at_ms INTEGER NOT NULL`
- `PRIMARY KEY (snapshot_id, repo_ordinal)`

Indexes:

- `(snapshot_id)`
- `(repo_path)`

### `codegraph_project_state`

Current authoritative binding between a project/workspace and its active CodeGraph state.

- `project_id TEXT PRIMARY KEY`
- `workspace_id TEXT`
- `active_snapshot_id TEXT`
- `active_refresh_job_id TEXT`
- `active_session_id TEXT`
- `last_refresh_trigger TEXT`
- `last_refresh_at_ms INTEGER`
- `last_mutation_at_ms INTEGER`
- `last_error_json TEXT`
- `updated_at_ms INTEGER NOT NULL`

This table replaces implicit "read whatever `graph_snapshot.json` exists on disk" behavior.

### `codegraph_navigation_sessions`

Prompt-facing working state for CodeGraph-backed navigation.

- `session_id TEXT PRIMARY KEY`
- `project_id TEXT NOT NULL`
- `workspace_id TEXT`
- `snapshot_id TEXT NOT NULL`
- `agent_mode TEXT NOT NULL` — `planner|freeflow|task_executor`
- `status TEXT NOT NULL` — `active|stale|closed|superseded`
- `focus_nodes_json TEXT NOT NULL`
- `pinned_nodes_json TEXT NOT NULL`
- `recent_traversals_json TEXT NOT NULL`
- `hydrated_excerpts_json TEXT NOT NULL`
- `session_summary_json TEXT`
- `freshness_state TEXT NOT NULL`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`
- `closed_at_ms INTEGER`

Indexes:

- `(project_id, updated_at_ms DESC)`
- `(snapshot_id)`
- `(status)`

### `codegraph_refresh_jobs`

Tracks rebuild/refresh lifecycle and explicit attribution.

- `refresh_job_id TEXT PRIMARY KEY`
- `project_id TEXT NOT NULL`
- `workspace_id TEXT`
- `base_snapshot_id TEXT`
- `result_snapshot_id TEXT`
- `trigger TEXT NOT NULL`
- `requested_by TEXT NOT NULL` — tool/runtime path or system actor
- `refresh_mode TEXT NOT NULL` — `incremental|full|legacy_import`
- `status TEXT NOT NULL` — `queued|running|succeeded|failed|cancelled`
- `full_rebuild_reason TEXT`
- `requested_at_ms INTEGER NOT NULL`
- `started_at_ms INTEGER`
- `completed_at_ms INTEGER`
- `error_json TEXT`

Indexes:

- `(project_id, requested_at_ms DESC)`
- `(status)`

### `codegraph_refresh_job_paths`

Per-path invalidation input for a refresh job.

- `refresh_job_id TEXT NOT NULL`
- `path_ordinal INTEGER NOT NULL`
- `repo_path TEXT NOT NULL`
- `change_kind TEXT NOT NULL` — `create|modify|delete|rename|unknown`
- `export_surface_changed INTEGER NOT NULL DEFAULT 0`
- `PRIMARY KEY (refresh_job_id, path_ordinal)`

Indexes:

- `(refresh_job_id)`
- `(repo_path)`

## Authority Rules

- Prompt assembly reads CodeGraph state through `codegraph_project_state` + `codegraph_navigation_sessions`.
- Navigation queries resolve against `codegraph_snapshot_repositories.document_json` for the bound active snapshot.
- Refresh status reads from `codegraph_refresh_jobs` and never infers status from missing/present files.
- Legacy `graph_snapshot.json` is, at most, an exported derivative for compatibility.

## Migration From Legacy Graph Snapshot Projection

Migration phases:

1. Add schema migrations to the native runtime state DB.
2. On first CodeGraph access for a project, if DB state is absent but `graph_snapshot.json` exists:
   - read and validate legacy artifact
   - import it into `codegraph_snapshots` + `codegraph_snapshot_repositories`
   - create `codegraph_project_state` binding with `source_kind=legacy_import`
   - emit explicit migration event/log
3. Switch runtime reads to DB-backed snapshot/session lookup.
4. Keep file projection only as optional compatibility output during transition.
5. After migration window, remove runtime dependence on file projection entirely.

Migration must fail loud if the legacy artifact is malformed, stale, or incompatible.

## Initial Migration Versions

Recommended first additions to `schema_migrations` in native runtime state DB:

- `3` — create CodeGraph snapshot + repository tables
- `4` — create project state + navigation sessions tables
- `5` — create refresh jobs + invalidation path tables

## Implementation Touchpoints

- Replace/wrap `src/core/registry/graph/snapshot/refresh.rs` so refresh writes DB-backed snapshot state first.
- Replace legacy readers that assume governance JSON is authoritative.
- Bind native harness prompt assembly and resume to `codegraph_project_state` and `codegraph_navigation_sessions`.
- Keep UCP build/query calls local and imported; do not fork extractor logic into Hivemind.

## Non-Goals

- fully relationalizing nodes/edges inside SQLite on day one
- replacing UCP traversal/query semantics with Hivemind-owned SQL queries
- making prompt memory authoritative for CodeGraph session recovery

Start with snapshot-level JSON persistence plus explicit metadata/session tables; normalize further only if profiling proves it necessary.