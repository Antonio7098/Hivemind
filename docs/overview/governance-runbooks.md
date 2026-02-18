---
title: Governance Runbooks
description: Operator procedures for governance migration, recovery, and constitution policy updates
order: 5
---

# Governance Runbooks

This document defines practical operator runbooks for Phase 3 governance workflows.

## 1) Migration Runbook (Legacy Repo-Local -> Canonical Governance Layout)

Use when a project still has repo-local `.hivemind/` governance files.

1. Inspect current governance state.
   ```bash
   hivemind -f json project governance inspect <project>
   ```
2. Execute migration.
   ```bash
   hivemind -f json project governance migrate <project>
   ```
3. Re-inspect and verify:
   - `initialized: true`
   - expected canonical artifact paths exist under `~/.hivemind/projects/<project-id>/`
4. Validate event trail:
   ```bash
   hivemind -f json events list --project <project> --artifact-id constitution.yaml --limit 200
   ```
   Confirm `governance_storage_migrated` and `governance_artifact_upserted` are present.

## 2) Drift/Missing Artifact Recovery Runbook

Use when governance context fails due stale snapshot, missing references, or missing artifacts.

1. Diagnose current health and replay parity.
   ```bash
   hivemind -f json project governance diagnose <project>
   hivemind -f json project governance replay <project> --verify
   ```
2. Capture/inspect recovery snapshots.
   ```bash
   hivemind -f json project governance snapshot create <project> --interval-minutes 30
   hivemind -f json project governance snapshot list <project> --limit 5
   ```
3. If diagnostics show `graph_snapshot_stale` or `graph_snapshot_missing`, refresh snapshot.
   ```bash
   hivemind graph snapshot refresh <project>
   ```
4. Build deterministic repair plan.
   ```bash
   hivemind -f json project governance repair detect <project>
   hivemind -f json project governance repair preview <project> --snapshot-id <snapshot-id>
   ```
5. Apply repair plan explicitly when all remaining issues are recoverable.
   ```bash
   hivemind -f json project governance repair apply <project> --snapshot-id <snapshot-id> --confirm
   ```
6. If diagnostics still show template reference issues (`template_document_missing`, `template_skill_missing`, `template_system_prompt_missing`):
   - recreate missing artifacts or update the template
   - re-run template instantiation
   ```bash
   hivemind global template instantiate <project> <template-id>
   ```
7. Re-run diagnostics until `healthy: true`.
8. Re-run policy validation before merge boundaries:
   ```bash
   hivemind -f json constitution check --project <project>
   ```

## 3) Constitution Policy Change Runbook

Use when adding/updating constitution rules.

1. Export/edit constitution content (YAML).
2. Validate candidate policy.
   ```bash
   hivemind -f json constitution validate <project>
   ```
3. Apply mutation with explicit human confirmation + audit metadata.
   ```bash
   hivemind constitution update <project> --from-file <policy.yaml> --confirm --actor <name> --intent "policy update reason"
   ```
4. Execute explicit policy check.
   ```bash
   hivemind -f json constitution check --project <project>
   ```
5. Review violations and event trail:
   ```bash
   hivemind -f json events list --project <project> --rule-id <rule-id> --limit 200
   ```

## 4) Operational Notes

- Treat diagnostics output as the authoritative operator checklist.
- Prefer deterministic fixes (`snapshot`, `repair preview`, `repair apply`) before manual file edits.
- Keep rollback steps explicit in incident notes (what changed, who approved, what was revalidated).
