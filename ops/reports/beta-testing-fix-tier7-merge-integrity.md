# Tier 7 Fix Note

Date: 2026-02-15  
Branch: `fix/tier6-9-beta-stabilization`

## Fixed Findings
- BUG-001-MRG001: blocked `merge prepare` attempts now emit flow-correlated `error_occurred` events (`flow_not_completed`).
- BUG-001-MRG004: abort-state inconsistency resolved via explicit task terminal transitions for active attempts.
- MRG dependency semantics clarity: CLI/docs now clearly state `graph add-dependency <from> <to>` means `<to>` depends on `<from>`.

## Validation
- `tests/integration.rs::cli_merge_prepare_blocked_emits_error_event`
- Repro matrix: blocked merge prepare emits `error_occurred` and merge lifecycle emits `merge_completed` for repeated flows.
