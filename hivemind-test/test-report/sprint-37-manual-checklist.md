# Sprint 37 Manual Checklist (Phase 3)

Date: 2026-02-17
Scope: UCP Graph Integration and Snapshot Projection

## Environment

- [x] Manual run executed with isolated `HOME`
- [x] Repositories initialized with deterministic commits for provenance checks
- [x] Evidence captured in command logs under `hivemind-test/test-report/`

## 37.1 UCP Integration Contract

- [x] Hivemind consumes UCP graph extraction via pinned `ucp-api` dependency (UCP git revision)
- [x] Snapshot output records `ucp_engine_version` and `profile_version`
- [x] Snapshot output records per-repo `canonical_fingerprint` and `logical_key`-compatible profile content
- [x] Accepted node/edge scope enforcement observed through successful profile-compliant snapshot creation

Evidence: `08-sprint37-manual.log`

## 37.2 Snapshot Projection Model

- [x] `graph_snapshot.json` materialized under project governance path
- [x] Snapshot envelope includes schema/version metadata, provenance, summary, and aggregate fingerprint
- [x] Static codegraph representation (`Document structure` + `Blocks`) persisted for LLM context projection

Evidence: `08-sprint37-manual.log`

## 37.3 Lifecycle and Observability

- [x] Snapshot refresh triggered on project attach (`trigger=project_attach`)
- [x] Snapshot refresh triggered on checkpoint completion (`trigger=checkpoint_complete`)
- [x] Snapshot refresh triggered on merge completion (`trigger=merge_completed`)
- [x] Manual refresh command works (`hivemind graph snapshot refresh`)
- [x] Snapshot lifecycle events (`started`, `completed`, `failed/diff` when applicable) are visible in event stream

Evidence: `08-sprint37-manual.log`, `10-sprint37-execution.log`

## 37.4 Integrity and Staleness Gates

- [x] Constitution validation fails when repo HEAD diverges from snapshot provenance
- [x] Failure includes actionable recovery hint (`Run: hivemind graph snapshot refresh <project>`)
- [x] Refresh + revalidate path succeeds after stale snapshot recovery

Evidence: `08-sprint37-manual.log`

## 37.5 Compatibility and Non-Regression

- [x] Existing worktree lifecycle smoke remains operational
- [x] Existing execution/checkpoint flow remains operational
- [x] Non-constitution project flow executes without new mandatory constitution coupling

Evidence: `09-sprint37-worktree.log`, `10-sprint37-execution.log`, `13-sprint37-ci-worktree.log`, `14-sprint37-ci-execution.log`, `08-sprint37-manual.log`

## 37.6 Exit Criteria Mapping

- [x] Hivemind uses UCP graph engine as authoritative extraction backend
- [x] Hivemind accepts only profile-compliant UCP graph artifacts
- [x] Snapshot lifecycle triggers and event telemetry are explicit and observable
- [x] Constitution engine receives stable, reproducible graph input
- [x] Hivemind-owned versioning/eventing/failure semantics are enforced
- [x] Manual validation in `@hivemind-test` is completed and documented
