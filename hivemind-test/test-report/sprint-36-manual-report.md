# Sprint 36 Manual Test Report

Date: 2026-02-17
Scope: Phase 3 / Sprint 36 — Constitution Core

## Summary

Manual validation confirms Sprint 36 constitution behavior is operational and auditable:

- constitution lifecycle commands (`init`, `show`, `validate`, `update`) are CLI-available
- mutation commands enforce explicit confirmation (`--confirm`)
- constitution schema validation enforces typed rule semantics and compatibility fields
- lifecycle and validation events are emitted and queryable
- project projection stores constitution digest/schema/version metadata

## Evidence Artifacts

- `07-sprint36-manual.log`
- `sprint-36-manual-checklist.md`

## Validations Performed

1. Governance bootstrap and confirmation gate validation
   - ran `project governance init`
   - verified `constitution init` fails without `--confirm`

2. Constitution initialization and inspection
   - ran `constitution init --from-file ... --confirm --actor manual-tester --intent ...`
   - ran `constitution show`
   - verified schema version, partitions, and rules in output

3. Constitution validation and update
   - ran `constitution validate` before and after mutation
   - ran `constitution update --from-file ... --confirm --actor ... --intent ...`
   - verified updated digest and governance revision increment

4. Auditability and projection checks
   - ran `project inspect` and verified constitution projection fields:
     - `constitution_digest`
     - `constitution_schema_version`
     - `constitution_version`
     - `constitution_updated_at`
   - ran `events stream --project ...` and verified:
     - `constitution_initialized`
     - `constitution_updated`
     - `constitution_validated`

## Exit Criteria Check

- [x] Every active project has one valid constitution
- [x] Constitution cannot be silently changed or bypassed
- [x] Constitution mutations are attributable, inspectable, and replayable
- [x] Manual validation in `hivemind-test` is completed and documented
