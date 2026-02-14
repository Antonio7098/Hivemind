# Phase 28 Report - Additional Runtime Adapters

## Scope
Implemented Phase 28 in `ops/ROADMAP.md`:
- Codex adapter
- Claude Code adapter
- Kilo adapter
- Runtime selection improvements:
  - per-project default runtime (existing, extended validation)
  - per-task runtime override (`task runtime-set`)
  - `hivemind runtime list`
  - `hivemind runtime health`

## Implementation Summary
- Added adapters:
  - `src/adapters/codex.rs`
  - `src/adapters/claude_code.rs`
  - `src/adapters/kilo.rs`
- Extended adapter registry metadata in `src/adapters/mod.rs`.
- Extended runtime/event model:
  - `TaskRuntimeConfigured`
  - `TaskRuntimeCleared`
  - `Task.runtime_override`
- Added CLI commands:
  - `hivemind runtime list`
  - `hivemind runtime health [--project <project>] [--task <task-id>]`
  - `hivemind task runtime-set <task-id> ...` and `--clear`
- Updated flow execution to resolve effective runtime per task:
  - task override if present
  - otherwise project runtime
- Preserved runtime lifecycle/events and runtime output projection pipeline.
- Hardened runtime projection parsing for real runtime output:
  - strip ANSI escape sequences and control artifacts before marker parsing
  - detect `Command:` / `Tool:` markers even when prefixed by runtime UI glyphs
  - added unit test: `projects_markers_with_ansi_prefixes`

## Validation
Executed:
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test --doc`

Result: all pass.

## Manual Runtime Test (fresh clone + real runtimes)

Fresh clone run directory:
- `/tmp/tmp.n8y5hrscgD/hivemind`

Logs:
- `/tmp/tmp.n8y5hrscgD/logs/runtime_list.json`
- `/tmp/tmp.n8y5hrscgD/logs/runtime_health_all.json`
- `/tmp/tmp.n8y5hrscgD/logs/runtime_health_project.json`
- `/tmp/tmp.n8y5hrscgD/logs/flow_tick.txt`
- `/tmp/tmp.n8y5hrscgD/logs/runtime_output_chunk_events.txt`
- `/tmp/tmp.n8y5hrscgD/logs/runtime_started_events.txt`
- `/tmp/tmp.n8y5hrscgD/logs/runtime_exited_events.txt`

Observed:
- `runtime list` shows: `opencode`, `codex`, `claude-code`, `kilo`.
- `runtime health` succeeded for defaults and project-targeted Kilo config.
- Real Kilo execution produced runtime lifecycle events:
  - `runtime_started` (adapter: `kilo`)
  - `runtime_output_chunk` (stderr captured)
  - `runtime_exited` (exit_code `0`)
- Task remained running due checkpoint completion gate (`checkpoints_incomplete`) which is expected by existing flow semantics.

Notes:
- Kilo model alias `opencode/kimi-k2.5-free` was not accepted by Kilo in this environment; a Kilo-native free model is required (for example entries reported by `kilo models`).
- A follow-up long-running Kilo attempt was stopped to avoid unnecessary runtime credit usage.
- `runtime_output_chunk` capture was confirmed from real Kilo runs (stderr/stdout chunks persisted in events). Projection marker extraction is now robust against ANSI-prefixed lines.

## Principle Checkpoint
- Observability preserved: runtime output and lifecycle events remain event-sourced.
- CLI-first preserved: runtime discovery/health and per-task override are CLI-native.
- Modularity improved: runtime adapters are replaceable and selected without changing taskflow semantics.
