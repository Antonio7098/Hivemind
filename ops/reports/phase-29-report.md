# Phase 29 Report - Event Streaming and Observability

## Scope
Phase 29 goals from `ops/ROADMAP.md`:
- Real-time event streaming command with practical filtering
- Historical event query command with inspect support
- Correlation and time-range filtering for observability

## Implementation Summary
- Extended event filter model (`src/storage/event_store.rs`):
  - `since` and `until` RFC3339 timestamp bounds
  - timestamp-aware filtering in `EventFilter::matches`
- Extended CLI event query/stream args (`src/cli/commands.rs`):
  - `events list`: `--project`, `--graph`, `--flow`, `--task`, `--attempt`, `--since`, `--until`, `--limit`
  - `events stream`: `--project`, `--graph`, `--flow`, `--task`, `--attempt`, `--since`, `--until`, `--limit`
- Unified filter construction and validation in CLI (`src/main.rs`):
  - strict UUID parsing per correlation filter
  - strict RFC3339 parsing for `--since` / `--until`
  - explicit invalid range error when `since > until`
- Preserved existing `events inspect <event-id>` and `events replay` semantics.

## Validation
Executed:
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`

Result: all pass.

## Added/Updated Tests
- `src/storage/event_store.rs`
  - `in_memory_store_filter_by_time_range`
- `tests/integration.rs`
  - extended `cli_events_stream_with_filters` to validate:
    - `events list --flow --since --until`
    - invalid timestamp handling (`invalid_timestamp`)
    - invalid range handling (`invalid_time_range`)

## Manual @hivemind-test Validation

Manual run used a fresh clone workspace and `hivemind-test` style sandbox repo under temp HOME.

Artifacts:
- root: `/tmp/tmp.pqAF9LVsk8`
- logs: `/tmp/tmp.pqAF9LVsk8/phase29-manual`

Captured start/end:
- start: `2026-02-14T21:31:38+00:00`
- end: `2026-02-14T21:31:40+00:00`
- duration: `2s`

Key checks:
- `events list --flow <id> --since ... --until ...` returns matching historical flow events.
- `events stream --flow <id> --since ...` returns matching stream output.
- invalid timestamp returns structured error `invalid_timestamp`.
- reversed time range returns structured error `invalid_time_range`.

## Principle Checkpoint
- Observability is truth: event queries now support explicit temporal windows and full correlation dimensions.
- CLI-first: all new observability surfaces are CLI-native and machine-readable.
- No magic: validation errors are explicit, typed, and actionable.

