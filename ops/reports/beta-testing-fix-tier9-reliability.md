# Tier 9 Fix Note

Date: 2026-02-15  
Branch: `fix/tier6-9-beta-stabilization`

## Fixed Findings
- REL-002 dependency anomaly investigation completed: scheduler ordering is correct; issue traced to ambiguous dependency argument semantics.
- Documentation and CLI argument docs now explicitly define `graph add-dependency <from> <to>` as `<to>` depends on `<from>`.
- Added 3-task chain regression to lock root-task readiness behavior at flow start.

## Validation
- `tests/integration.rs::cli_dependency_chain_only_root_task_starts_ready`
- Repro matrix confirms `events list --flow --since --until` support and deterministic dependency ordering.
