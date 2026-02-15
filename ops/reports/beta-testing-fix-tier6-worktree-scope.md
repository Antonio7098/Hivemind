# Tier 6 Fix Note

Date: 2026-02-15  
Branch: `fix/tier6-9-beta-stabilization`

## Fixed Findings
- ART-003-01: detect out-of-worktree writes (`/tmp`) as scope violations.
- ART-003-02: detect parent-repo git drift as scope violations.
- ART-004-01: `attempt inspect --context` now returns retry context.
- ART-005-01: `worktree cleanup` now requires `--force` for running flows and supports `--dry-run`.
- ART-005-02: `worktree_cleanup_performed` event now emitted.

## Regression Coverage
- `tests/integration.rs`: scope violation detection, worktree preservation, context inspection, cleanup force/event coverage.

## Validation
- `cargo test --all-features --test integration -- --test-threads=1`
- Tier matrix reproductions for tier 6 in `/tmp/hm_tier6_9_validation.txt`.
