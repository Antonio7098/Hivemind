# Sprint 36 Manual Checklist (Phase 3)

Date: 2026-02-17
Scope: Constitution Core

## Environment

- [x] Run in isolated HOME sandbox
- [x] Project/repository/governance initialized before constitution commands
- [x] Evidence captured in command log

## 36.1 Constitution Schema v1

- [x] Constitution Schema v1 file with `version`, `schema_version`, `compatibility`, `partitions`, and `rules` created and applied
- [x] Rule types validated manually (`forbidden_dependency`, `allowed_dependency`)
- [x] Compatibility/version fields validated (`constitution.v1`, `governance.v1`)

Evidence: `07-sprint36-manual.log`

## 36.2 Constitution Lifecycle Commands

- [x] `constitution init` fails without `--confirm`
- [x] `constitution init` succeeds with `--confirm`, `--actor`, `--intent`
- [x] `constitution show` returns canonical schema/partition/rule content
- [x] `constitution validate` returns `valid=true` and empty issues for valid constitution
- [x] `constitution update` succeeds with explicit confirmation and mutation metadata

Evidence: `07-sprint36-manual.log`

## 36.3 Constitution Event Model

- [x] `constitution_initialized` observed in event stream
- [x] `constitution_updated` observed in event stream
- [x] `constitution_validated` observed in event stream
- [x] Governance upsert revision increments for constitution mutations observed
- [x] Event payload includes actor + mutation intent + digest metadata

Evidence: `07-sprint36-manual.log`

## 36.4 Projection and Auditability

- [x] `project inspect` includes constitution digest/schema/version projection fields
- [x] Constitution mutation is attributable to explicit actor
- [x] Constitution mutation requires explicit confirmation (cannot be silent)

Evidence: `07-sprint36-manual.log`

## 36.5 Exit Criteria Mapping

- [x] Every active project in the manual run has one valid constitution
- [x] Constitution cannot be silently changed or bypassed
- [x] Constitution mutations are attributable, inspectable, and replayable
- [x] Manual validation is completed and documented
