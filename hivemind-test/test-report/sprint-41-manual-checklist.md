# Sprint 41 Manual Checklist (Phase 3)

Date: 2026-02-18
Scope: Governance Hardening and Production Readiness

## Environment

- [x] Manual run executed with isolated `HOME`
- [x] Governance artifacts initialized and validated on a real repository
- [x] Evidence captured under `hivemind-test/test-report/`

## 41.1 Testing and Reliability

- [x] Governance lifecycle and enforcement gates validated manually (`project governance`, `constitution`, `graph snapshot`)
- [x] Replay integrity validated after governance/context activity via `events replay <flow> --verify`
- [x] Artifact operations exercised while concurrent flow ticks were active (template instantiation + document updates)

Evidence: `23-sprint41-manual.log`

## 41.2 Observability and Operations

- [x] Event filters validated for `--artifact-id`, `--template-id`, and `--rule-id`
- [x] `project governance diagnose` reports missing references (`template_document_missing`)
- [x] `project governance diagnose` reports stale snapshots (`graph_snapshot_stale`) and recovery to healthy state after refresh

Evidence: `23-sprint41-manual.log`

## 41.3 Documentation and Adoption

- [x] Governance CLI/help/docs behavior exercised against live commands
- [x] Quickstart governance scenario commands validated in manual run
- [x] Runbook workflows validated: migration/recovery/policy-change command surfaces available and deterministic

Evidence: `23-sprint41-manual.log`, `24-sprint41-ci-worktree.log`, `25-sprint41-ci-execution.log`, `26-sprint41-ci-events.log`

## 41.4 Full Manual Phase 3 E2E (`@hivemind-test`)

- [x] Fresh-clone `hivemind-test/test_worktree.sh` smoke run passes
- [x] Fresh-clone `hivemind-test/test_execution.sh` smoke run passes
- [x] Event store inspection confirms runtime/task/governance event visibility

Evidence: `24-sprint41-ci-worktree.log`, `25-sprint41-ci-execution.log`, `26-sprint41-ci-events.log`

## 41.5 Exit Criteria Mapping

- [x] Governance/context features meet principle checkpoints with no hidden state
- [x] Operators can inspect, explain, and recover governance state end-to-end
- [x] Phase 3 capabilities are stable for broad project use
- [x] Manual validation in `@hivemind-test` is completed and documented
