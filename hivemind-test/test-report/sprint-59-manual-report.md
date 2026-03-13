# Sprint 59 Manual Report

Date: 2026-03-10
Owner: Antonio
Sprint: 59 (GraphCode Context Registry, Runtime Compaction, And Bounded Prompt Loss Accounting)
Status: IN PROGRESS (provider-backed graph runtime evidence captured; broader compaction/benchmark evidence still pending)

## Scope

This change set delivers the Sprint 59 code-path work for:

- authoritative runtime GraphCode artifact/session persistence in the native runtime state DB
- local path-based `ucp-api` / `ucp-graph` / `ucp-codegraph` imports from `../unified-content-protocol`
- GraphCode-registry-first native `graph_query` loading with stale-fingerprint marking and file-upgrade fallback
- local UCP GraphCode/graph navigator execution for bounded code graph traversal
- lane-aware prompt assembly metadata with overflow classification
- explicit record-time tool-output truncation and bounded GraphCode navigation summaries

## Automated Evidence Completed

- `cargo test native::tool_engine::tests:: -- --nocapture`
- `cargo test core::graph_query:: -- --nocapture`
- `cargo test agent_loop_surfaces_graph_query_results_into_the_next_prompt -- --nocapture`
- `cargo test native::runtime_hardening::storage::tests::runtime_state_store_applies_wal_and_migrations -- --nocapture`
- `cargo test native::runtime_hardening::storage::tests::runtime_state_store_persists_graphcode_registry_records -- --nocapture`
- `cargo test native::tests:: -- --nocapture`

## Provider-Backed Evidence Completed

- `bash hivemind-test/test_sprint_59_graph_runtime.sh`
- OpenRouter/native runtime completed a real local-repo taskflow with GraphCode-backed `graph_query`, `read_file`, `write_file`, `checkpoint_complete`, and `DONE`.
- Passing report artifact: `hivemind-test/test-report/2026-03-10-sprint59-graph-runtime-report.md`
- Key observed evidence from the passing run:
  - provider/model provenance: `openrouter|openrouter/openai/gpt-4o-mini`
  - flow state `completed`, task state `success`, attempt count `1`
  - graph queries observed `4` with python graph queries observed `3`
  - codegraph storage backend `ucp_graph_sqlite`
  - generic graph artifacts observed `1`
  - turn-budget failures observed `0`

## Remaining Manual Gaps

- a real larger-scope native run that triggers non-trivial context growth and visible compaction/truncation
- proof that GraphCode-backed context remains attributable and freshness-aware while the agent continues making real edits successfully across a longer implementation flow
- at least one benchmark-style GraphCode workflow (for example path explanation, likely-test ranking, or rank-before-hydrate)

## Notes

- The runtime registry is now authoritative for native GraphCode loading; `graph_snapshot.json` remains as a derivative/upgrade projection.
- The current implementation uses a bounded Hivemind request wrapper over locally imported UCP GraphCode/graph navigator APIs built from stored UCP snapshot artifacts.
- The first provider-backed reruns exposed two real issues that are now fixed on this branch: successful `graph_query` results were not visible as prompt tool results, and the Sprint 59 smoke used overly heavy `python_query` snippets that exceeded the 15s tool envelope.
- Remaining follow-up gaps are now about longer-run compaction evidence, benchmark/manual validation breadth, and moving larger payload storage farther out of the Hivemind DB.

## Artifacts

- `hivemind-test/test-report/sprint-59-manual-checklist.md`
- `hivemind-test/test-report/2026-03-10-sprint59-graph-runtime-report.md`
- `ops/reports/sprint-59-report-2026-03-09.md`

