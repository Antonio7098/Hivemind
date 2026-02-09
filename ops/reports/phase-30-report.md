# Phase 30: UI Foundation (Optional)

## 1. Phase Metadata

- **Phase**: 30
- **Title**: UI Foundation (Optional)
- **Branch**: `phase/30-ui-foundation`
- **Dates**: 2026-02-09 – 2026-02-09
- **Owner**: Antonio

## 2. Objectives

- Provide a thin, local HTTP server that exposes hivemind’s CLI/event-sourced state for UI projection.
- Hook the existing React frontend to that server without violating CLI-first or “UI is a projection” principles.
- Ensure all validation gates (`make validate`, frontend lint/build) pass.

## 3. Deliverables

- New `hivemind serve` CLI command that starts a local HTTP server:
  - `/health`
  - `/api/version`
  - `/api/state?events_limit=N` (projects, tasks, graphs, flows, merge_states, events)
  - `/api/projects`
  - `/api/tasks`
  - `/api/graphs`
  - `/api/flows`
  - `/api/merges`
  - `/api/events?limit=N`
- `src/server.rs` module that builds `UiState` from `Registry::state()` + `list_events`.
- Frontend store wiring to fetch `/api/state` on an interval and reflect real registry-derived state.
- Lint-clean theme context refactor (`ThemeContext.tsx` + `themeContext.ts` + `themeTypes.ts` + `useTheme.ts`).

## 4. Exit Criteria Results

From `ops/ROADMAP.md` Phase 30:

- [x] UI reads via CLI/events only (projection over `Registry` + events.jsonl).
- [x] UI does not modify state directly (HTTP API is read-only; no mutation endpoints).
- [x] UI reflects CLI state accurately (verified by hitting CLI to create flows/tasks and observing UI update).
- [x] No UI-only features (all visible state corresponds to CLI-accessible entities).

## 5. Metrics

- **Lines added**: ~70 in `src/server.rs`, small wiring changes in CLI and frontend.
- **Lines removed**: legacy mock-only landing content file removed, minor changes elsewhere.
- **Tests**:
  - `make validate` (fmt-check, clippy, tests, doc) – PASS
  - `npm run lint` – PASS
  - `npm run build` – PASS
- **New tests**:
  - `server::tests::api_version_ok`
  - `server::tests::api_state_ok_empty`
  - `server::tests::api_unknown_endpoint_404`

## 6. Principle Compliance

1. **Observability is truth**: UI state is computed from `Registry::state()` + `list_events`; no hidden state.
2. **Fail fast, fail loud**: HTTP errors wrap `HivemindError` via `CliResponse::<()>::error`.
3. **Reliability over cleverness**: Server is a small, explicit wrapper around the existing registry; no hidden caches.
4. **Explicit error taxonomy**: Reuses `HivemindError` codes (`endpoint_not_found`, `server_bind_failed`, etc.).
5. **Structure scales**: New `server` module is isolated and exported via `lib.rs`.
6. **SOLID**: HTTP layer delegates all domain logic to existing `Registry` and event store.
7. **CLI-first**: There is still no “backend” API distinct from CLI/state; this is a projection server only.
8. **Absolute observability**: `/api/events` returns structured events with correlation ids.
9. **Automated checks**: `make validate` is mandatory and passes.
10. **Failures are first-class**: HTTP 4xx/5xx structured via `CliResponse::error`.
11. **Build incrementally**: Adds only the minimal API needed for current frontend views.
12. **Maximum modularity**: Frontend base URL is configurable; server is optional (`hivemind serve`).
13. **Abstraction without loss**: HTTP schema closely matches Rust core types + event payloads.
14. **Human authority**: No new automation that bypasses human control; UI remains read-only.
15. **No magic**: Endpoints and transformations are explicit and documented in code.

## 7. Challenges

- Aligning a new HTTP surface area with strict CLI-first principles.
- Getting clippy clean with strict lints for the new module (match arm consolidation, borrow rules).

## 8. Learnings

- A registry-centric “projection server” is a good fit for UI needs without introducing a parallel API.
- Small helper tests around HTTP handlers help keep behavior stable as the registry evolves.

## 9. Next Phase Readiness

- UI can now present graphs/flows/events derived from real event-sourced state.
- Sets the foundation for future UI improvements (e.g. live event streaming, richer dashboards) while staying read-only.
