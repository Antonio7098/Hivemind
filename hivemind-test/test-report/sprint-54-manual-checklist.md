# Sprint 54 Manual Checklist

Date: 2026-02-28
Owner: Antonio
Sprint: 54 (Managed Network Policy And Host Approvals)

## 54.1 Managed Network Proxy

- [x] Validate local managed network proxy mode starts deterministic HTTP listener for native `run_command` execution.
- [x] Validate optional SOCKS5 listener contract exists and is policy-configurable.
- [x] Validate local admin control endpoint is created for managed proxy mode.
- [x] Validate safe bind defaults clamp non-loopback bind requests unless dangerous override is explicitly enabled.

## 54.2 Host/Domain Policy Engine

- [x] Validate host allowlist/denylist rules with deny-wins precedence.
- [x] Validate local/private address protection controls deny loopback/private targets by default.
- [x] Validate limited network mode enforces method restrictions with deterministic deny projection.

## 54.3 Network Approval Lifecycle

- [x] Validate per-attempt host/protocol approval flow supports `immediate` and `deferred` modes.
- [x] Validate approval outcomes and denials are explicit and attributable via `policy_tags`.
- [x] Validate deferred denial watcher terminates running command sessions when denial decisions land.

## 54.4 Operational Verification

- [x] Run `hivemind-test/test_worktree.sh` against fresh-clone release binary.
- [x] Run `hivemind-test/test_execution.sh` against fresh-clone release binary.
- [x] Inspect canonical SQLite runtime/filesystem event evidence under `/tmp/hivemind-test/.hm_home/.hivemind/db.sqlite`.

## Artifacts

- `hivemind-test/test-report/2026-02-28-sprint54-manual-bin.log`
- `hivemind-test/test-report/2026-02-28-sprint54-test_worktree.log`
- `hivemind-test/test-report/2026-02-28-sprint54-test_execution.log`
- `hivemind-test/test-report/2026-02-28-sprint54-event-inspection.log`
- `hivemind-test/test-report/2026-02-28-sprint54-filesystem-inspection.log`
- `hivemind-test/test-report/sprint-54-manual-report.md`
