---
title: CLI Semantics
description: Command-line operational semantics
order: 1
---

# Hivemind — CLI Operational Semantics

> **Principle 7:** CLI-first is non-negotiable.
> **Principle 15:** No magic. Everything has a reason.

This document specifies the **operational semantics** of Hivemind CLI commands: not just what you can do, but what happens when you do it. Each command has defined preconditions, effects, and failure modes.

The CLI is the authoritative interface. These semantics are the contract.

---

## 1. Semantic Structure

### 1.1 Command Semantics Format

Each command is specified with:
- **Synopsis:** Command signature
- **Preconditions:** What must be true before execution
- **Effects:** What changes when command succeeds
- **Events:** What events are emitted
- **Failures:** What can go wrong and how it's reported
- **Idempotence:** Behavior on repeated execution

### 1.2 Notation

```
[optional]
<required>
...       # repeatable
|         # alternatives
```

---

## 2. Project Commands

### 2.1 project create

**Synopsis:**
```
hivemind project create <name> [--description <text>]
```

**Preconditions:**
- No project with `<name>` exists in registry

**Effects:**
- New project created with unique ID
- Project added to registry
- Project directory structure created (if applicable)

**Events:**
```
ProjectCreated:
  project_id: <generated>
  name: <name>
  description: <text>
```

**Failures:**
- `project_exists`: Project with name exists
- `invalid_project_name`: Name is empty

**Idempotence:** Not idempotent. Second call fails.

---

### 2.2 project list

**Synopsis:**
```
hivemind [-f json|table|yaml] project list
```

**Preconditions:** None

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `REGISTRY_READ_ERROR`: Cannot read project registry

**Idempotence:** Idempotent. Always returns current state.

---

### 2.3 project attach-repo

**Synopsis:**
```
hivemind project attach-repo <project> <repo-path> [--name <name>] [--access ro|rw]
```

**Preconditions:**
- Project `<project>` exists
- `<repo-path>` is a valid git repository
- No repo with same name already attached

**Effects:**
- Repository reference added to project
- Access mode recorded (default: rw)
- Graph snapshot refresh is triggered for the project (`trigger=project_attach`)

**Events:**
```
RepositoryAttachedToProject:
  project_id: <project_id>
  repo_name: <name>
  repo_path: <repo-path>
  access_mode: <ro|rw>

GraphSnapshotStarted:
  project_id: <project_id>
  trigger: project_attach
  repository_count: <n>

GraphSnapshotCompleted|GraphSnapshotFailed:
  project_id: <project_id>
  trigger: project_attach
  ...
```

**Failures:**
- `project_not_found`: Project doesn't exist
- `repo_path_not_found`: Path does not exist
- `invalid_repository_path`: Empty path
- `not_a_git_repo`: Path is not a git repo
- `repo_already_attached`: Repo path already attached
- `repo_name_already_attached`: Repo name already used

**Idempotence:** Not idempotent. Second call fails.

---

### 2.4 project runtime-set

**Synopsis:**
```
hivemind project runtime-set <project> [--role worker|validator] [--adapter <name>] [--binary-path <path>] [--model <model>] [--arg <arg>...] [--env KEY=VALUE...] [--timeout-ms <ms>] [--max-parallel-tasks <n>]
```

**Preconditions:**
- Project `<project>` exists

**Effects:**
- Project runtime default for the selected role is set (or replaced)
- Worker defaults influence task execution scheduling/runtime selection
- Validator defaults influence verification/runtime selection for validator role

**Events:**
```
ProjectRuntimeConfigured:
  project_id: <project_id>
  adapter_name: <name>
  binary_path: <path>
  model: <model> | null
  args: [<args...>]
  env: { <key>: <value>, ... }
  timeout_ms: <ms>
  max_parallel_tasks: <n>

ProjectRuntimeRoleConfigured:
  project_id: <project_id>
  role: WORKER|VALIDATOR
  adapter_name: <name>
  binary_path: <path>
  model: <model> | null
  args: [<args...>]
  env: { <key>: <value>, ... }
  timeout_ms: <ms>
  max_parallel_tasks: <n>
```

**Failures:**
- `project_not_found`
- `invalid_runtime_adapter`
- `invalid_env`: invalid env formatting
- `invalid_max_parallel_tasks`: `--max-parallel-tasks` must be >= 1

**Idempotence:** Idempotent if config is unchanged. Otherwise emits a new configuration event.

---

### 2.5 project inspect

**Synopsis:**
```
hivemind [-f json|table|yaml] project inspect <project>
```

**Preconditions:**
- Project exists

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `project_not_found`

**Idempotence:** Idempotent.

---

### 2.6 project update

**Synopsis:**
```
hivemind project update <project> [--name <name>] [--description <text>]
```

**Preconditions:**
- Project exists

**Effects:**
- Project metadata updated

**Events:**
```
ProjectUpdated:
  project_id: <project_id>
  name: <name> | null
  description: <text> | null
```

**Failures:**
- `project_not_found`
- `invalid_project_name`: Name is empty
- `project_name_conflict`: Name already taken

**Idempotence:** Idempotent if no changes. Otherwise emits a new update event.

---

### 2.7 project detach-repo

**Synopsis:**
```
hivemind project detach-repo <project> <repo-name>
```

**Preconditions:**
- Project exists
- Repository with `<repo-name>` is attached

**Effects:**
- Repository reference removed from project

**Events:**
```
RepositoryDetached:
  project_id: <project_id>
  name: <repo-name>
```

**Failures:**
- `project_not_found`
- `repo_not_found`

**Idempotence:** Not idempotent. Second call fails.

---

### 2.8 project governance init

**Synopsis:**
```
hivemind [-f json|table|yaml] project governance init <project>
```

**Preconditions:**
- Project exists

**Effects:**
- Creates/ensures canonical governance storage under `~/.hivemind/projects/<project-id>/` and `~/.hivemind/global/`
- Initializes deterministic governance projection state for the project

**Events:**
```
GovernanceProjectStorageInitialized:
  project_id: <project-id>
  schema_version: governance.v1
  projection_version: 1
  root_path: ~/.hivemind/projects/<project-id>

GovernanceArtifactUpserted:
  project_id: <project-id>|null
  scope: project|global
  artifact_kind: constitution|documents|notepad|graph_snapshot|skills|system_prompts|templates
  artifact_key: <artifact-key>
  path: <absolute-path>
  revision: <n>
  schema_version: governance.v1
  projection_version: 1
```

**Failures:**
- `project_not_found`
- `governance_storage_create_failed`
- `governance_path_conflict`

**Idempotence:** Idempotent. Re-running preserves existing files/directories and only emits missing projection events.

---

### 2.9 project governance migrate

**Synopsis:**
```
hivemind [-f json|table|yaml] project governance migrate <project>
```

**Preconditions:**
- Project exists

**Effects:**
- Migrates legacy repo-local governance artifacts from `<repo>/.hivemind/...` into canonical global/project governance storage
- Preserves existing non-empty canonical artifacts unless they match scaffold defaults
- Ensures governance layout and projection state are initialized

**Events:**
```
GovernanceStorageMigrated:
  project_id: <project-id>
  from_layout: repo_local_hivemind_v1
  to_layout: global_governance_v1
  migrated_paths: [<absolute-path>...]
  rollback_hint: <text>
  schema_version: governance.v1
  projection_version: 1
```

**Failures:**
- `project_not_found`
- `governance_migration_failed`
- `governance_storage_create_failed`

**Idempotence:** Idempotent for already-migrated content.

---

### 2.10 project governance inspect

**Synopsis:**
```
hivemind [-f json|table|yaml] project governance inspect <project>
```

**Preconditions:**
- Project exists

**Effects:** None (read-only)

**Output:**
- Governance root path, schema/projection metadata, export/import boundary note
- Canonical artifact paths and projection revision status
- Legacy candidate paths still present in repo-local layout
- Migration history summaries for the project

**Events:** None

**Failures:**
- `project_not_found`

**Idempotence:** Idempotent.

---

### 2.10.1 constitution

**Synopsis:**
```
hivemind [-f json|table|yaml] constitution init <project> [--content <yaml> | --from-file <path>] --confirm [--actor <name>] [--intent <text>]
hivemind [-f json|table|yaml] constitution show <project>
hivemind [-f json|table|yaml] constitution validate <project>
hivemind [-f json|table|yaml] constitution check --project <project>
hivemind [-f json|table|yaml] constitution update <project> (--content <yaml> | --from-file <path>) --confirm [--actor <name>] [--intent <text>]
```

**Preconditions:**
- Project exists
- Canonical constitution path is `~/.hivemind/projects/<project-id>/constitution.yaml`
- `init` and `update` require explicit `--confirm`
- `update` requires constitution to be initialized first
- `check` requires constitution to be initialized for explicit/manual validation
- Projects with attached repositories must have a current graph snapshot artifact

**Effects:**
- Defines and validates Constitution Schema v1 (`version`, `schema_version`, `compatibility`, `partitions[]`, `rules[]`)
- Enforces strict rule semantics:
  - `forbidden_dependency` / `allowed_dependency` require known partition IDs for `from` + `to`
  - `coverage_requirement` requires known `target` partition and `threshold` in `0..=100`
- Evaluates `validate(graph_snapshot, constitution)` with deterministic, severity-aware outcomes:
  - `hard`: blocks checkpoint/merge progression
  - `advisory`: reported, non-blocking
  - `informational`: logged, non-blocking
- Maintains per-project constitution digest/version projection fields on project state
- Preserves mutation audit metadata (`actor`, `mutation_intent`, confirmation flag)

**Events:**
```
ConstitutionInitialized:
  project_id: <project-id>
  path: <constitution-path>
  schema_version: constitution.v1
  constitution_version: 1
  digest: <content-digest>
  revision: <n>
  actor: <actor>
  mutation_intent: <text>
  confirmed: true

ConstitutionUpdated:
  project_id: <project-id>
  path: <constitution-path>
  schema_version: constitution.v1
  constitution_version: 1
  previous_digest: <digest>
  digest: <digest>
  revision: <n>
  actor: <actor>
  mutation_intent: <text>
  confirmed: true

ConstitutionValidated:
  project_id: <project-id>
  path: <constitution-path>
  schema_version: constitution.v1
  constitution_version: 1
  digest: <digest>
  valid: true|false
  issues: [<code:field:message>...]
  validated_by: <actor>

ConstitutionViolationDetected:
  project_id: <project-id>
  flow_id: <flow-id>|null
  task_id: <task-id>|null
  attempt_id: <attempt-id>|null
  gate: manual_check|checkpoint_complete|merge_prepare|merge_approve|merge_execute
  rule_id: <rule-id>
  rule_type: forbidden_dependency|allowed_dependency|coverage_requirement
  severity: hard|advisory|informational
  message: <human-readable>
  evidence: [<strings>...]
  remediation_hint: <hint>|null
  blocked: true|false
```

`init` and `update` also emit `GovernanceArtifactUpserted` for `artifact_kind: constitution`.

**Failures:**
- `constitution_confirmation_required`
- `constitution_schema_invalid`
- `constitution_validation_failed`
- `constitution_not_initialized`
- `constitution_not_found`
- `constitution_input_read_failed`
- `constitution_content_missing`
- `constitution_hard_violation`
- `graph_snapshot_missing`
- `graph_snapshot_stale`
- `graph_snapshot_integrity_invalid`

**Idempotence:**
- `show`: idempotent
- `validate`: idempotent relative to file content (always emits a validation event)
- `check`: idempotent relative to graph snapshot + constitution state (always emits violation events when present)
- `init`: non-idempotent after initial constitution lifecycle event (must use `update` for subsequent mutations)
- `update`: idempotent when content digest is unchanged, but still explicit and confirmed

---

### 2.11 project governance document

**Synopsis:**
```
hivemind [-f json|table|yaml] project governance document create <project> <document-id> --title <text> --owner <text> [--tag <tag>...] --content <text>
hivemind [-f json|table|yaml] project governance document list <project>
hivemind [-f json|table|yaml] project governance document inspect <project> <document-id>
hivemind [-f json|table|yaml] project governance document update <project> <document-id> [--title <text>] [--owner <text>] [--tag <tag>...] [--content <text>]
hivemind [-f json|table|yaml] project governance document delete <project> <document-id>
```

**Preconditions:**
- Project exists
- `document-id` uses governance identifier format

**Effects:**
- `create` and `update` write canonical artifact JSON under `~/.hivemind/projects/<project-id>/documents/`
- Document metadata (`title`, `owner`, `tags`, `updated_at`) is maintained
- Revision history is immutable (new revisions are appended)

**Events:**
```
GovernanceArtifactUpserted:
  project_id: <project-id>
  scope: project
  artifact_kind: document
  artifact_key: <document-id>
  path: <absolute-path>
  revision: <n>
  schema_version: governance.v1
  projection_version: 1

GovernanceArtifactDeleted:
  project_id: <project-id>
  scope: project
  artifact_kind: document
  artifact_key: <document-id>
  path: <absolute-path>
  schema_version: governance.v1
  projection_version: 1
```

**Failures:**
- `project_not_found`
- `invalid_governance_identifier`
- `document_not_found`
- `governance_artifact_schema_invalid`

**Idempotence:**
- `list`/`inspect`: idempotent
- `create`: non-idempotent (conflicts when artifact already exists)
- `update`: idempotent when no effective change
- `delete`: non-idempotent (fails if document is absent)

---

### 2.12 project governance attachment

**Synopsis:**
```
hivemind [-f json|table|yaml] project governance attachment include <project> <task-id> <document-id>
hivemind [-f json|table|yaml] project governance attachment exclude <project> <task-id> <document-id>
```

**Preconditions:**
- Project and task exist
- Referenced document exists for `include`

**Effects:**
- Sets explicit per-task attachment lifecycle for project documents
- Included/excluded sets are consumed at attempt start to resolve per-attempt document context

**Events:**
```
GovernanceAttachmentLifecycleUpdated:
  project_id: <project-id>
  task_id: <task-id>
  artifact_kind: document
  artifact_key: <document-id>
  attached: true|false
  schema_version: governance.v1
  projection_version: 1
```

**Derived execution telemetry (during `flow tick`):**
- `AttemptContextOverridesApplied` captures template documents, explicit includes/excludes, and final resolved set used by the attempt.

**Failures:**
- `project_not_found`
- `task_not_found`
- `document_not_found`

**Idempotence:** Idempotent when attachment state is unchanged.

---

### 2.13 project governance notepad

**Synopsis:**
```
hivemind [-f json|table|yaml] project governance notepad create <project> --content <text>
hivemind [-f json|table|yaml] project governance notepad show <project>
hivemind [-f json|table|yaml] project governance notepad update <project> --content <text>
hivemind [-f json|table|yaml] project governance notepad delete <project>
```

**Preconditions:**
- Project exists

**Effects:**
- Maintains project-level notepad at `~/.hivemind/projects/<project-id>/notepad.md`
- Notepad is explicitly **non-executional** and **non-validating**
- Notepad content is never attached to runtime input by default

**Events:**
- `create`/`update`: `GovernanceArtifactUpserted` (`artifact_kind: notepad`)
- `delete`: `GovernanceArtifactDeleted` (`artifact_kind: notepad`)
- `show`: no events

**Failures:**
- `project_not_found`
- `governance_artifact_read_failed`

**Idempotence:**
- `show`: idempotent
- `create`/`update`: idempotent when content unchanged
- `delete`: idempotent if already absent

---

### 2.14 global skill

**Synopsis:**
```
hivemind [-f json|table|yaml] global skill create <skill-id> --name <text> --content <text> [--tag <tag>...]
hivemind [-f json|table|yaml] global skill list
hivemind [-f json|table|yaml] global skill inspect <skill-id>
hivemind [-f json|table|yaml] global skill update <skill-id> [--name <text>] [--content <text>] [--tag <tag>...]
hivemind [-f json|table|yaml] global skill delete <skill-id>
```

**Effects:**
- Maintains globally reusable skill artifacts under `~/.hivemind/global/skills/`
- Enforces schema/identifier validation with structured failures

**Events:**
- Mutations emit governance upsert/delete events with `scope: global` and `artifact_kind: skill`

**Failures:**
- `invalid_governance_identifier`
- `governance_artifact_schema_invalid`
- `skill_not_found`

---

### 2.15 global system-prompt

**Synopsis:**
```
hivemind [-f json|table|yaml] global system-prompt create <prompt-id> --content <text>
hivemind [-f json|table|yaml] global system-prompt list
hivemind [-f json|table|yaml] global system-prompt inspect <prompt-id>
hivemind [-f json|table|yaml] global system-prompt update <prompt-id> --content <text>
hivemind [-f json|table|yaml] global system-prompt delete <prompt-id>
```

**Effects:**
- Maintains globally reusable system prompt artifacts under `~/.hivemind/global/system_prompts/`

**Events:**
- Mutations emit governance upsert/delete events with `scope: global` and `artifact_kind: system_prompt`

**Failures:**
- `invalid_governance_identifier`
- `governance_artifact_schema_invalid`
- `system_prompt_not_found`

---

### 2.16 global template

**Synopsis:**
```
hivemind [-f json|table|yaml] global template create <template-id> --system-prompt-id <prompt-id> [--skill-id <skill-id>...] [--document-id <document-id>...] [--description <text>]
hivemind [-f json|table|yaml] global template list
hivemind [-f json|table|yaml] global template inspect <template-id>
hivemind [-f json|table|yaml] global template update <template-id> [--system-prompt-id <prompt-id>] [--skill-id <skill-id>...] [--document-id <document-id>...] [--description <text>]
hivemind [-f json|table|yaml] global template delete <template-id>
hivemind [-f json|table|yaml] global template instantiate <project> <template-id>
```

**Effects:**
- Stores template references in `~/.hivemind/global/templates/`
- Validates referenced system prompt and skill IDs
- `instantiate` resolves all references against project/global registries and emits immutable resolution event

**Events:**
```
TemplateInstantiated:
  project_id: <project-id>
  template_id: <template-id>
  system_prompt_id: <prompt-id>
  skill_ids: [<skill-id>...]
  document_ids: [<document-id>...]
  schema_version: governance.v1
  projection_version: 1
```

Template create/update/delete operations also emit governance upsert/delete events (`scope: global`, `artifact_kind: template`).

**Failures:**
- `template_not_found`
- `system_prompt_not_found`
- `skill_not_found`
- `document_not_found`
- `governance_artifact_schema_invalid`

---

### 2.17 global notepad

**Synopsis:**
```
hivemind [-f json|table|yaml] global notepad create --content <text>
hivemind [-f json|table|yaml] global notepad show
hivemind [-f json|table|yaml] global notepad update --content <text>
hivemind [-f json|table|yaml] global notepad delete
```

**Effects:**
- Maintains global notepad at `~/.hivemind/global/notepad.md`
- Contract: notepad is always `non_executional=true` and `non_validating=true`
- Empty scaffold notepad is treated as absent (`exists=false`) for read semantics

**Events:**
- `create`/`update`: `GovernanceArtifactUpserted` (`scope: global`, `artifact_kind: notepad`)
- `delete`: `GovernanceArtifactDeleted` (`scope: global`, `artifact_kind: notepad`)

**Failures:**
- `governance_artifact_read_failed`
- `governance_artifact_write_failed`

**Idempotence:**
- `show`: idempotent
- `create`/`update`: idempotent when content unchanged
- `delete`: idempotent if already absent

---

## 3. Task Commands

### 3.1 task create

**Synopsis:**
```
hivemind task create <project> <title> [--description <text>] [--scope <scope-def>]
```

**Preconditions:**
- Project exists

**Effects:**
- Task created with unique ID
- Task added to project's task list
- Task state: OPEN (not in any TaskFlow)

**Events:**
```
TaskCreated:
  task_id: <generated>
  project_id: <project_id>
  title: <title>
```

**Failures:**
- `project_not_found`: Project doesn't exist
- `invalid_task_title`: Title is empty
- `invalid_scope`: Scope definition is malformed

**Idempotence:** Not idempotent. Creates new task each time.

---

### 3.2 task list

**Synopsis:**
```
hivemind [-f json|table|yaml] task list <project> [--state <state>]
```

**Preconditions:**
- Project exists

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `PROJECT_NOT_FOUND`: Project doesn't exist

**Idempotence:** Idempotent.

---

### 3.3 task close

**Synopsis:**
```
hivemind task close <task-id> [--reason <text>]
```

**Preconditions:**
- Task exists
- Task is not in an active TaskFlow

**Effects:**
- Task state changes to CLOSED
- Task removed from active consideration

**Events:**
```
TaskClosed:
  task_id: <task-id>
  reason: <text>
```

**Failures:**
- `task_not_found`: Task doesn't exist
- `task_in_active_flow`: Task is part of running TaskFlow

**Idempotence:** Idempotent. Closing closed task is no-op.

---

### 3.4 task inspect

**Synopsis:**
```
hivemind [-f json|table|yaml] task inspect <task-id>
```

**Preconditions:**
- Task ID is a valid UUID
- Task exists

**Effects:** None (read-only)

**Events:** None

**Failures:**
- `invalid_task_id`: Task ID is not a valid UUID
- `task_not_found`: Task doesn't exist

**Idempotence:** Idempotent.

---

### 3.5 task update

**Synopsis:**
```
hivemind task update <task-id> [--title <text>] [--description <text>]
```

**Preconditions:**
- Task exists

**Effects:**
- Task metadata updated

**Events:**
```
TaskUpdated:
  task_id: <task-id>
  title: <text> | null
  description: <text> | null
```

**Failures:**
- `invalid_task_id`: Task ID is not a valid UUID
- `task_not_found`: Task doesn't exist
- `invalid_task_title`: Title is empty

**Idempotence:** Idempotent if no changes. Otherwise emits a new update event.

---

### 3.6 task runtime-set

**Synopsis:**
```
hivemind task runtime-set <task-id> [--role worker|validator] [--clear] [--adapter <name>] [--binary-path <path>] [--model <model>] [--arg <arg>...] [--env KEY=VALUE...] [--timeout-ms <ms>]
```

**Preconditions:**
- Task `<task-id>` exists

**Effects:**
- Sets a task-level runtime override for the selected role
- `--clear` removes the task-level override and falls back to flow/project/global defaults

**Events:**
```
TaskRuntimeConfigured:
  task_id: <task_id>
  adapter_name: <name>
  binary_path: <path>
  model: <model> | null
  args: [<args...>]
  env: { <key>: <value>, ... }
  timeout_ms: <ms>

TaskRuntimeRoleConfigured:
  task_id: <task_id>
  role: WORKER|VALIDATOR
  adapter_name: <name>
  binary_path: <path>
  model: <model> | null
  args: [<args...>]
  env: { <key>: <value>, ... }
  timeout_ms: <ms>

TaskRuntimeCleared:
  task_id: <task_id>

TaskRuntimeRoleCleared:
  task_id: <task_id>
  role: WORKER|VALIDATOR
```

**Failures:**
- `task_not_found`
- `invalid_runtime_adapter`
- `invalid_env`

**Idempotence:** `--clear` is idempotent when no override is present.

---

### 3.7 task set-run-mode

**Synopsis:**
```
hivemind task set-run-mode <task-id> <manual|auto>
```

**Preconditions:**
- Task exists

**Effects:**
- Sets task execution mode:
  - `auto`: task is eligible for scheduler dispatch when dependencies are met
  - `manual`: task remains `READY` until explicitly ticked or switched back to `auto`

**Events:**
```
TaskRunModeSet:
  task_id: <task-id>
  mode: MANUAL|AUTO
```

**Failures:**
- `task_not_found`

**Idempotence:** Idempotent when mode is unchanged.

---

## 4. TaskGraph Commands

### 4.1 graph create

**Synopsis:**
```
hivemind graph create <project> <name> [--from-tasks <task-ids...>]
```

**Preconditions:**
- Project exists
- All specified tasks exist and are open

**Effects:**
- TaskGraph created with unique ID
- Tasks added as nodes (if specified)
- Graph is mutable until used

**Events:**
```
TaskGraphCreated:
  graph_id: <generated>
  project_id: <project_id>
  name: <name>
```

**Failures:**
- `PROJECT_NOT_FOUND`
- `TASK_NOT_FOUND`: One or more tasks don't exist

**Idempotence:** Not idempotent. Creates new graph.

---

### 4.2 graph add-dependency

**Synopsis:**
```
hivemind graph add-dependency <graph-id> <from-task> <to-task>
```

**Preconditions:**
- Graph exists and is mutable
- Both tasks are in the graph
- Adding dependency doesn't create cycle

**Effects:**
- Dependency edge added to graph
- `<to-task>` depends on `<from-task>` (`<from-task>` must complete before `<to-task>` can start)

**Events:**
```
DependencyAdded:
  graph_id: <graph-id>
  from_task: <from-task>
  to_task: <to-task>
```

**Failures:**
- `GRAPH_NOT_FOUND`
- `GRAPH_IMMUTABLE`: Graph already used in TaskFlow
- `TASK_NOT_IN_GRAPH`
- `CYCLE_DETECTED`: Dependency would create cycle

**Idempotence:** Idempotent. Adding existing dependency is no-op.

---

### 4.3 graph validate

**Synopsis:**
```
hivemind graph validate <graph-id>
```

**Preconditions:**
- Graph exists

**Effects:** None (validation only)

**Output:**
- Validation result (valid/invalid)
- List of issues if invalid

**Events:** None

**Failures:**
- `GRAPH_NOT_FOUND`

**Idempotence:** Idempotent.

---

### 4.4 graph snapshot refresh

**Synopsis:**
```
hivemind [-f json|table|yaml] graph snapshot refresh <project>
```

**Preconditions:**
- Project exists
- Project has at least one attached repository
- Attached repositories resolve a valid HEAD commit

**Effects:**
- Rebuilds static snapshot artifact at `~/.hivemind/projects/<project-id>/graph_snapshot.json`
- Uses UCP codegraph extraction as authoritative backend (no local duplicate parser)
- Persists UCP profile/version metadata, canonical fingerprint, repository provenance, and static structure+blocks projection
- Emits diff telemetry when canonical fingerprint changes

**Events:**
```
GraphSnapshotStarted:
  project_id: <project-id>
  trigger: manual_refresh
  repository_count: <n>

GraphSnapshotDiffDetected:
  project_id: <project-id>
  trigger: manual_refresh|project_attach|checkpoint_complete|merge_completed
  previous_fingerprint: <optional>
  canonical_fingerprint: <digest>

GraphSnapshotCompleted:
  project_id: <project-id>
  trigger: manual_refresh|project_attach|checkpoint_complete|merge_completed
  path: ~/.hivemind/projects/<project-id>/graph_snapshot.json
  revision: <n>
  repository_count: <n>
  ucp_engine_version: <version>
  profile_version: codegraph.v1
  canonical_fingerprint: <digest>

GraphSnapshotFailed:
  project_id: <project-id>
  trigger: manual_refresh|project_attach|checkpoint_complete|merge_completed
  reason: <message>
  hint: <optional>
```

**Failures:**
- `project_not_found`
- `project_has_no_repo`
- `ucp_codegraph_build_failed`
- `graph_snapshot_profile_invalid`
- `graph_snapshot_scope_unsupported`
- `graph_snapshot_fingerprint_mismatch`
- `governance_artifact_write_failed`

**Idempotence:** Content-idempotent; revision increments are event-driven and expected.

---

## 5. TaskFlow Commands

### 5.1 flow create

**Synopsis:**
```
hivemind flow create <graph-id> [--name <name>]
```

**Preconditions:**
- Graph exists and is valid
- Graph is not already used by another active flow

**Effects:**
- TaskFlow created with unique ID
- Graph becomes immutable
- Flow state: CREATED (not started)
- All tasks in flow: PENDING

**Events:**
```
TaskFlowCreated:
  flow_id: <generated>
  graph_id: <graph-id>
  name: <name>
```

**Failures:**
- `GRAPH_NOT_FOUND`
- `GRAPH_INVALID`: Graph fails validation
- `GRAPH_IN_USE`: Graph already used by active flow

**Idempotence:** Not idempotent. Creates new flow.

---

### 5.2 flow start

**Synopsis:**
```
hivemind flow start <flow-id>
```

**Preconditions:**
- Flow exists
- Flow state is CREATED or PAUSED
- All declared upstream flow dependencies are completed

**Effects:**
- Flow state changes to RUNNING
- Scheduler begins releasing ready tasks
- Tasks with no dependencies become READY

**Events:**
```
TaskFlowStarted:
  flow_id: <flow-id>
  timestamp: <now>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_ALREADY_RUNNING`: Flow is already running
- `FLOW_COMPLETED`: Flow has already completed
- `FLOW_ABORTED`: Flow was aborted
- `flow_dependencies_unmet`: One or more required upstream flows are not completed

**Idempotence:** Idempotent if flow is PAUSED. Not idempotent if CREATED.

---

### 5.3 flow pause

**Synopsis:**
```
hivemind flow pause <flow-id> [--wait]
```

**Preconditions:**
- Flow exists
- Flow state is RUNNING

**Effects:**
- Flow state changes to PAUSED
- No new tasks scheduled
- Running tasks continue to completion (unless --wait)
- With --wait: blocks until running tasks complete

**Events:**
```
TaskFlowPaused:
  flow_id: <flow-id>
  running_tasks: [<task-ids>]
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_RUNNING`: Flow is not in RUNNING state

**Idempotence:** Idempotent. Pausing paused flow is no-op.

---

### 5.4 flow resume

**Synopsis:**
```
hivemind flow resume <flow-id>
```

**Preconditions:**
- Flow exists
- Flow state is PAUSED

**Effects:**
- Flow state changes to RUNNING
- Scheduler resumes releasing ready tasks

**Events:**
```
TaskFlowResumed:
  flow_id: <flow-id>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_PAUSED`: Flow is not paused

**Idempotence:** Not idempotent on non-paused flow.

---

### 5.5 flow abort

**Synopsis:**
```
hivemind flow abort <flow-id> [--force] [--reason <text>]
```

**Preconditions:**
- Flow exists
- Flow is not already COMPLETED or ABORTED

**Effects:**
- Flow state changes to ABORTED
- Any RUNNING/VERIFYING task attempts are marked FAILED with reason `flow_aborted`
- No further tasks scheduled
- Artifacts preserved

**Events:**
```
TaskExecutionStateChanged:
  flow_id: <flow-id>
  task_id: <task-id>
  attempt_id: <attempt-id|null>
  from: RUNNING|VERIFYING
  to: FAILED

TaskExecutionFailed:
  flow_id: <flow-id>
  task_id: <task-id>
  attempt_id: <attempt-id|null>
  reason: flow_aborted

TaskFlowAborted:
  flow_id: <flow-id>
  reason: <text>
  forced: <boolean>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_ALREADY_TERMINAL`: Flow is completed or aborted

**Idempotence:** Idempotent. Aborting aborted flow is no-op.

---

### 5.6 flow restart

**Synopsis:**
```
hivemind flow restart <flow-id> [--name <name>] [--start]
```

**Preconditions:**
- Flow exists
- Flow state is ABORTED

**Effects:**
- Creates a new flow from the same graph
- Copies flow dependencies and flow-level runtime defaults
- Copies run mode (`manual|auto`) from the source flow
- If `--start` is set, immediately starts the restarted flow

**Events:**
- Emits the same event sequence as creating/configuring a new flow:
  - `TaskFlowCreated`
  - `TaskFlowDependencyAdded` (if dependencies existed)
  - `TaskFlowRuntimeConfigured` (if flow runtime defaults existed)
  - `TaskFlowRunModeSet` (if source run mode was `auto`)
  - `TaskFlowStarted` (if started via `--start` or auto-start)

**Failures:**
- `FLOW_NOT_FOUND`
- `flow_not_aborted`: Source flow is not aborted
- `graph_invalid`: Source graph can no longer validate

**Idempotence:** Not idempotent. Each invocation creates a new flow.

---

### 5.7 flow status

**Synopsis:**
```
hivemind flow status <flow-id>
```

**Preconditions:**
- Flow exists

**Effects:** None (read-only)

**Output:**
- Flow state
- Task states summary
- Progress metrics
- Current activity

**Events:** None

**Failures:**
- `FLOW_NOT_FOUND`

**Idempotence:** Idempotent.

---

### 5.8 flow tick

**Synopsis:**
```
hivemind flow tick <flow-id> [--interactive] [--max-parallel <n>]
```

`--interactive` is deprecated. The flag now returns `interactive_mode_deprecated`.

**Preconditions:**
- Flow exists
- Flow state is RUNNING
- A runtime can be resolved for runnable tasks using precedence:
  - task role override
  - flow role default
  - project role default
  - global role default

**Effects:**
- Transitions any dependency-satisfied `PENDING` tasks to `READY`
- Executes up to the effective concurrency limit of task attempts using the configured runtime adapter
- Effective limit is derived from `--max-parallel`, project runtime `max_parallel_tasks`, and optional global cap `HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL`
- Evaluates scope compatibility for each dispatch candidate within the tick
- Hard conflicts are deferred and serialized
- Soft conflicts are allowed with warning telemetry
- Emits runtime lifecycle events correlated by attempt ID

If `--interactive` is provided:

- The command fails with `interactive_mode_deprecated`
- No attempt is started

**Events:**
```
TaskReady:
  flow_id: <flow-id>
  task_id: <task-id>

ScopeConflictDetected:
  flow_id: <flow-id>
  task_id: <task-id>
  conflicting_task_id: <task-id>
  severity: soft_conflict | hard_conflict
  action: warn_parallel | serialized
  reason: <text>

TaskSchedulingDeferred:
  flow_id: <flow-id>
  task_id: <task-id>
  reason: <text>

TaskExecutionStateChanged:
  task_id: <task-id>
  from: READY
  to: RUNNING

RuntimeStarted:
  attempt_id: <attempt-id>
  task_id: <task-id>
  adapter_name: <adapter-name>
  prompt: <runtime-prompt>
  flags: [<runtime-args-and-flags...>]

AttemptContextAssembled:
  flow_id: <flow-id>
  task_id: <task-id>
  attempt_id: <attempt-id>
  attempt_number: <n>
  manifest_hash: <hash>
  inputs_hash: <hash>
  context_hash: <hash>
  context_size_bytes: <bytes>
  truncated_sections: [<section>...]
  manifest_json: <manifest-json>

AttemptContextTruncated:
  flow_id: <flow-id>
  task_id: <task-id>
  attempt_id: <attempt-id>
  budget_bytes: <bytes>
  original_size_bytes: <bytes>
  truncated_size_bytes: <bytes>
  sections: [<section>...]
  policy: ordered_section_then_total_budget

AttemptContextDelivered:
  flow_id: <flow-id>
  task_id: <task-id>
  attempt_id: <attempt-id>
  manifest_hash: <hash>
  inputs_hash: <hash>
  context_hash: <hash>
  delivery_target: runtime_execution_input
  prior_attempt_ids: [<attempt-id>...]
  prior_manifest_hashes: [<hash>...]

RuntimeOutputChunk:
  attempt_id: <attempt-id>
  stream: stdout|stderr
  content: <text>

RuntimeInputProvided:
  attempt_id: <attempt-id>
  content: <text>

RuntimeInterrupted:
  attempt_id: <attempt-id>

RuntimeFilesystemObserved:
  attempt_id: <attempt-id>
  files_created: [<paths...>]
  files_modified: [<paths...>]
  files_deleted: [<paths...>]

RuntimeExited:
  attempt_id: <attempt-id>
  exit_code: <code>

TaskExecutionStateChanged:
  task_id: <task-id>
  from: RUNNING
  to: VERIFYING
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_RUNNING`
- `RUNTIME_NOT_CONFIGURED`
- `WORKTREE_NOT_FOUND`
- `UNSUPPORTED_RUNTIME`
- `interactive_mode_deprecated`: `--interactive` is no longer supported
- `invalid_max_parallel`: `--max-parallel` must be >= 1
- `invalid_global_parallel_limit`: `HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL` is invalid

**Idempotence:** Not idempotent. Each tick may schedule and/or execute work.

---

### 5.9 flow set-run-mode

**Synopsis:**
```
hivemind flow set-run-mode <flow-id> <manual|auto>
```

**Preconditions:**
- Flow exists

**Effects:**
- Sets flow scheduling mode:
  - `manual`: scheduling advances only via explicit `flow start/resume/tick`
  - `auto`: flow may automatically progress when runnable
- If flow is `CREATED`, mode is `auto`, and dependencies are satisfied, flow may start immediately
- If flow is `RUNNING` and mode changes to `auto`, scheduler attempts immediate progress

**Events:**
```
TaskFlowRunModeSet:
  flow_id: <flow-id>
  mode: MANUAL|AUTO
```

**Failures:**
- `FLOW_NOT_FOUND`

**Idempotence:** Idempotent when mode is unchanged.

---

### 5.10 flow add-dependency

**Synopsis:**
```
hivemind flow add-dependency <flow-id> <depends-on-flow-id>
```

**Preconditions:**
- Both flows exist
- Both flows belong to the same project
- Target flow is in `CREATED` state
- Dependency does not create a cycle

**Effects:**
- Declares that `<flow-id>` cannot start until `<depends-on-flow-id>` reaches `COMPLETED`

**Events:**
```
TaskFlowDependencyAdded:
  flow_id: <flow-id>
  depends_on_flow_id: <depends-on-flow-id>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `flow_dependency_locked`
- `flow_dependency_cross_project`
- `flow_dependency_self`
- `flow_dependency_cycle`

**Idempotence:** Idempotent when dependency already exists.

---

### 5.11 flow runtime-set

**Synopsis:**
```
hivemind flow runtime-set <flow-id> [--role worker|validator] [--clear] [--adapter <name>] [--binary-path <path>] [--model <model>] [--arg <arg>...] [--env KEY=VALUE...] [--timeout-ms <ms>] [--max-parallel-tasks <n>]
```

**Preconditions:**
- Flow exists

**Effects:**
- Sets or clears a flow-level runtime default for the selected role
- Flow-level runtime defaults take precedence over project/global defaults and are lower precedence than task overrides

**Events:**
```
TaskFlowRuntimeConfigured:
  flow_id: <flow-id>
  role: WORKER|VALIDATOR
  adapter_name: <name>
  binary_path: <path>
  model: <model> | null
  args: [<args...>]
  env: { <key>: <value>, ... }
  timeout_ms: <ms>
  max_parallel_tasks: <n>

TaskFlowRuntimeCleared:
  flow_id: <flow-id>
  role: WORKER|VALIDATOR
```

**Failures:**
- `FLOW_NOT_FOUND`
- `invalid_runtime_adapter`
- `invalid_env`
- `invalid_max_parallel_tasks`

**Idempotence:** `--clear` is idempotent when no role default exists.

---

## 6. Task Execution Commands

### 6.1 task start

**Synopsis:**
```
hivemind task start <task-id>
```

**Preconditions:**
- Task exists and is part of a TaskFlow
- Flow is RUNNING
- Task execution state is READY or RETRY
- Worktree exists for the task

**Effects:**
- Task execution state transitions to RUNNING
- A new attempt is created for the task
- A filesystem baseline is captured for that attempt
- Baseline artifact is persisted under `.hivemind/artifacts/baselines/`

**Events:**
```
TaskExecutionStateChanged:
  task_id: <task-id>
  from: READY|RETRY
  to: RUNNING

AttemptStarted:
  task_id: <task-id>
  attempt_id: <generated>

BaselineCaptured:
  task_id: <task-id>
  attempt_id: <attempt-id>
  baseline_id: <generated>
  git_head: <sha>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`: Task is not part of any TaskFlow
- `FLOW_NOT_RUNNING`: Flow is not in RUNNING state
- `TASK_NOT_READY`: Task is not in READY or RETRY state
- `WORKTREE_NOT_FOUND`: Worktree not found for task
- `BASELINE_CAPTURE_FAILED`: Baseline capture failed

**Idempotence:** Not idempotent.

---

### 6.2 task complete

**Synopsis:**
```
hivemind task complete <task-id>
```

**Preconditions:**
- Task exists and is part of a TaskFlow
- Task execution state is RUNNING
- The current attempt has an associated baseline

**Effects:**
- Task execution state transitions to VERIFYING
- Changes are detected and unified diffs are computed against the baseline
- Diff artifact is persisted under `.hivemind/artifacts/diffs/`
- A best-effort checkpoint commit may be created in the task worktree

**Events:**
```
TaskExecutionStateChanged:
  task_id: <task-id>
  from: RUNNING
  to: VERIFYING

FileModified:
  task_id: <task-id>
  attempt_id: <attempt-id>
  path: <path>
  change_type: CREATED|MODIFIED|DELETED

DiffComputed:
  task_id: <task-id>
  attempt_id: <attempt-id>
  diff_id: <generated>
  baseline_id: <baseline-id>

CheckpointCommitCreated:
  task_id: <task-id>
  attempt_id: <attempt-id>
  commit_sha: <sha>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`
- `TASK_NOT_RUNNING`: Task is not in RUNNING state
- `BASELINE_NOT_FOUND`: Baseline artifact missing for attempt
- `DIFF_COMPUTE_FAILED`: Diff computation failed

**Idempotence:** Not idempotent.

---

### 6.3 task retry

**Synopsis:**
```
hivemind task retry <task-id> [--reset-count]
```

**Preconditions:**
- Task exists and is in a TaskFlow
- Task state is FAILED or RETRY
- Retry limit not exceeded (unless --reset-count)

**Effects:**
- Task state changes to PENDING (if dependencies met) or appropriate state
- New attempt will be scheduled
- With --reset-count: retry counter reset to 0

**Events:**
```
TaskRetryRequested:
  task_id: <task-id>
  reset_count: <boolean>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`: Task is not part of any TaskFlow
- `TASK_NOT_RETRIABLE`: Task is not in retriable state
- `RETRY_LIMIT_EXCEEDED`: And --reset-count not specified

**Idempotence:** Not idempotent. Each call queues a retry.

---

### 6.4 task abort

**Synopsis:**
```
hivemind task abort <task-id> [--reason <text>]
```

**Preconditions:**
- Task exists and is in a TaskFlow
- Task is not already SUCCESS or FAILED

**Effects:**
- Task state changes to FAILED
- Running attempt terminated
- Downstream tasks remain PENDING (blocked)

**Events:**
```
TaskAborted:
  task_id: <task-id>
  reason: <text>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`
- `TASK_ALREADY_TERMINAL`: Task is success or failed

**Idempotence:** Idempotent. Aborting failed task is no-op.

---

### 6.5 attempt list

**Synopsis:**
```
hivemind attempt list [--flow <flow-id>] [--task <task-id>] [--limit <n>]
```

**Preconditions:** None

**Effects:** None (read-only)

**Output:**
- Attempt metadata (attempt ID, flow ID, task ID, attempt number, checkpoint completion status)
- Optional filtering by flow/task

**Events:** None

**Failures:**
- `invalid_flow_id`
- `invalid_task_id`

**Idempotence:** Idempotent.

---

### 6.6 attempt inspect

**Synopsis:**
```
hivemind attempt inspect <attempt-id> [--context] [--diff] [--output]
```

**Preconditions:**
- Attempt exists

**Effects:** None (read-only)

**Output:**
- Attempt metadata
- With `--context`: assembled context object:
  - `retry`: retry context text when retry path exists
  - `manifest`: immutable attempt context manifest (ordered inputs, resolved artifacts, budget/truncation metadata, retry links)
  - `manifest_hash`: digest of stored manifest
  - `inputs_hash`: deterministic digest of resolved context inputs
  - `delivered_context_hash`: digest of runtime-delivered context payload
- With --diff: changes made
- With --output: runtime output

**Notes:**
- The CLI primarily expects `<attempt-id>`. For backwards compatibility, `<task-id>` may be accepted and will return the latest attempt information for that task within its flow.
- With `--diff`, the CLI prints the stored unified diff artifact (if one has been computed).

**Events:** None

**Failures:**
- `ATTEMPT_NOT_FOUND`

**Idempotence:** Idempotent.

---

### 6.7 checkpoint list

**Synopsis:**
```
hivemind checkpoint list <attempt-id>
```

**Preconditions:**
- Attempt exists

**Effects:** None (read-only)

**Output:**
- Declared checkpoints for the attempt
- Current checkpoint state (`declared`, `active`, `completed`)
- Associated checkpoint commit hash (if completed)

**Events:** None

**Failures:**
- `invalid_attempt_id`
- `attempt_not_found`

**Idempotence:** Idempotent.

---

### 6.8 worktree list

**Synopsis:**
```
hivemind worktree list <flow-id>
```

**Preconditions:**
- Flow exists
- Flow belongs to a project with exactly one attached repository

**Effects:** None (read-only)

**Output:**
- Worktree status list for each task in the flow

**Events:** None

**Failures:**
- `FLOW_NOT_FOUND`
- `PROJECT_HAS_NO_REPO`: No repository attached
- `MULTIPLE_REPOS_UNSUPPORTED`: More than one repository attached

**Idempotence:** Idempotent.

---

### 6.9 worktree inspect

**Synopsis:**
```
hivemind worktree inspect <task-id>
```

**Preconditions:**
- Task exists and is part of a flow
- Flow belongs to a project with exactly one attached repository

**Effects:** None (read-only)

**Output:**
- Worktree status for the task, including:
  - expected worktree path
  - whether the path is a valid git worktree
  - current branch and HEAD (if available)

**Events:** None

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_IN_FLOW`
- `PROJECT_HAS_NO_REPO`: No repository attached
- `MULTIPLE_REPOS_UNSUPPORTED`: More than one repository attached

**Idempotence:** Idempotent.

---

### 6.10 worktree cleanup

**Synopsis:**
```
hivemind worktree cleanup <flow-id> [--force] [--dry-run]
```

**Preconditions:**
- Flow exists
- Flow belongs to a project with exactly one attached repository
- If flow state is RUNNING, `--force` is required

**Effects:**
- Removes all task worktrees for the flow under `~/hivemind/worktrees/<flow-id>/...`
- With `--dry-run`, returns success without removing worktrees

**Output:**
- Number of worktrees cleaned (or that would be cleaned in `--dry-run`)

**Events:**
```
WorktreeCleanupPerformed:
  flow_id: <flow-id>
  cleaned_worktrees: <count>
  forced: <boolean>
  dry_run: <boolean>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `PROJECT_HAS_NO_REPO`: No repository attached
- `MULTIPLE_REPOS_UNSUPPORTED`: More than one repository attached
- `flow_running_cleanup_requires_force`: running flow cleanup requires `--force`
- `GIT_WORKTREE_FAILED`: Worktree remove fails

**Idempotence:** Idempotent (no-op if worktrees are absent).

---

## 7. Verification Commands

### 7.1 verify override

**Synopsis:**
```
hivemind verify override <task-id> <pass|fail> --reason <text>
```

**Preconditions:**
- Task exists and is in VERIFYING state
- User has override authority

**Effects:**
- Verification outcome overridden
- Task transitions based on override (SUCCESS or FAILED)
- Override recorded with attribution

**Events:**
```
HumanOverride:
  task_id: <task-id>
  override_type: VERIFICATION_OVERRIDE
  decision: <pass|fail>
  reason: <text>
  user: <current-user>
```

**Failures:**
- `TASK_NOT_FOUND`
- `TASK_NOT_VERIFYING`: Task is not in verification state
- `OVERRIDE_NOT_PERMITTED`: Policy forbids override

**Idempotence:** Not idempotent. Only valid during VERIFYING state.

---

## 8. Merge Commands

### 8.1 merge prepare

**Synopsis:**
```
hivemind merge prepare <flow-id> [--target <branch>]
```

**Preconditions:**
- Flow exists
- Flow state is COMPLETED (success)
- No pending merge preparation
- Constitution hard-rule gate passes when a project constitution is initialized

**Effects:**
- Integration commits computed
- Conflict check performed
- Merge preview generated

**Events:**
```
MergePrepared:
  flow_id: <flow-id>
  repos: [<repo-statuses>]
  conflicts: [<conflicts>]

ErrorOccurred:
  error.code: flow_not_completed
  correlation.flow_id: <flow-id>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `FLOW_NOT_COMPLETED`: Flow hasn't completed successfully
- `MERGE_ALREADY_PREPARED`: Preparation exists
- `constitution_hard_violation`

**Idempotence:** Idempotent if no conflicts. Re-preparation refreshes.

---

### 8.2 merge approve

**Synopsis:**
```
hivemind merge approve <flow-id>
```

**Preconditions:**
- Flow exists
- Merge is prepared
- No unresolved conflicts
- Constitution hard-rule gate passes when a project constitution is initialized

**Effects:**
- Merge marked as approved
- Ready for execution

**Events:**
```
MergeApproved:
  flow_id: <flow-id>
  user: <current-user>
```

**Failures:**
- `FLOW_NOT_FOUND`
- `MERGE_NOT_PREPARED`: No merge preparation
- `UNRESOLVED_CONFLICTS`: Conflicts exist
- `constitution_hard_violation`

**Idempotence:** Idempotent. Approving approved merge is no-op.

---

### 8.3 merge execute

**Synopsis:**
```
hivemind merge execute <flow-id> [--mode local|pr] [--monitor-ci] [--auto-merge] [--pull-after]
```

**Preconditions:**
- Flow exists
- Merge is approved
- Constitution hard-rule gate passes when a project constitution is initialized

**Effects:**
- `--mode local` (default):
  - acquires integration lock
  - fast-forwards target from `integration/<flow>/prepare`
  - cleans integration/exec branches
  - emits `MergeCompleted`
- `--mode pr`:
  - pushes `integration/<flow>/prepare` to origin
  - creates or reuses a pull request via `gh`
  - optional CI monitoring (`--monitor-ci`)
  - optional auto squash merge (`--auto-merge`)
  - optional pull target branch after merge (`--pull-after`)
  - emits `MergeCompleted` when merge completion is confirmed

**Events:**
```
MergeCompleted:
  flow_id: <flow-id>
  commits: [<commit-refs>]
```

**Failures:**
- `FLOW_NOT_FOUND`
- `MERGE_NOT_APPROVED`: Merge not approved
- `MERGE_CONFLICT`: Conflict occurred during merge
- `constitution_hard_violation`
- `PUSH_FAILED`: Could not push to remote
- `pr_merge_multi_repo_unsupported`
- `pr_merge_no_origin`
- `gh_not_available` / `gh_command_failed`
- `ci_monitor_failed`

**Idempotence:** Not idempotent. Cannot merge twice.

---

## 8.4 Runtime Commands

### runtime list

**Synopsis:**
```
hivemind runtime list
```

**Effects:**
- Lists built-in runtime adapters and local binary availability

**Failures:** None (read-only)

### runtime health

**Synopsis:**
```
hivemind runtime health [--project <project>] [--flow <flow-id>] [--task <task-id>] [--role worker|validator]
```

**Effects:**
- Runs adapter health check for the selected runtime target
- If no target is provided, reports aggregate default-binary availability
- Reports the effective runtime target selected by role-aware precedence

### runtime defaults-set

**Synopsis:**
```
hivemind runtime defaults-set [--role worker|validator] [--adapter <name>] [--binary-path <path>] [--model <model>] [--arg <arg>...] [--env KEY=VALUE...] [--timeout-ms <ms>] [--max-parallel-tasks <n>]
```

**Effects:**
- Sets global runtime defaults for the selected role
- Serves as the final fallback in runtime resolution

**Failures:**
- `runtime_not_configured` (when checking project/task with no configured runtime)
- `task_not_found`
- `flow_not_found`
- `project_not_found`
- `invalid_runtime_adapter`
- `invalid_env`
- `invalid_max_parallel_tasks`

**Exit behavior:** Returns non-zero when the target runtime is unhealthy.

---

## 9. Event Commands

### 9.1 events list

**Synopsis:**
```
hivemind events list [--project <project>] [--graph <graph-id>] [--flow <flow-id>] [--task <task-id>] [--attempt <attempt-id>] [--since <rfc3339>] [--until <rfc3339>] [--limit <n>]
```

**Preconditions:** None

**Effects:** None (read-only)

**Output:**
- Historical event records
- Optional correlation and time-window filtering

**Events:** None (reads events, doesn't create)

**Failures:**
- `invalid_*_id`: Invalid correlation ID
- `invalid_timestamp`: Invalid RFC3339 timestamp
- `invalid_time_range`: `--since` is later than `--until`

**Idempotence:** Idempotent.

---

### 9.2 events inspect

**Synopsis:**
```
hivemind events inspect <event-id>
```

**Preconditions:** Event exists

**Effects:** None (read-only)

**Output:** Full event payload and metadata

**Failures:**
- `invalid_event_id`
- `event_not_found`

**Idempotence:** Idempotent.

---

### 9.3 events stream

**Synopsis:**
```
hivemind events stream [--flow <flow-id>] [--task <task-id>] [--project <project>] [--graph <graph-id>] [--attempt <attempt-id>] [--since <rfc3339>] [--until <rfc3339>] [--limit <n>]
```

**Preconditions:** None

**Effects:** None (read-only, streaming)

**Output:**
- Real-time event stream (historical matching events first, then new matching events)
- Filtered by correlation IDs and/or time window

**Events:** None (reads events, doesn't create)

**Failures:**
- `invalid_*_id`: Invalid correlation ID
- `invalid_timestamp`: Invalid RFC3339 timestamp
- `invalid_time_range`: `--since` is later than `--until`

**Idempotence:** N/A (streaming command)

---

### 9.4 events replay

**Synopsis:**
```
hivemind events replay <flow-id> [--verify]
```

**Preconditions:**
- Flow exists (or existed)

**Effects:**
- With --verify: compares replayed state to stored state
- Without --verify: outputs reconstructed state

**Output:**
- Reconstructed orchestration state
- Discrepancies (with --verify)

**Events:** None

**Failures:**
- `FLOW_NOT_FOUND`
- `EVENT_CORRUPTION`: Events are corrupt
- `STATE_MISMATCH`: Replayed state differs (with --verify)

**Idempotence:** Idempotent.

---

## 10. Output Contracts

### 10.1 Standard Output Format

All commands support:
```
-f, --format json    # Machine-readable JSON
-f, --format table   # Human-readable output
-f, --format yaml    # YAML output
```

Default: table

### 10.2 Error Output Format

Errors are always structured:
```json
{
  "success": false,
  "error": {
    "category": "user",
    "code": "task_not_found",
    "message": "Task abc123 not found",
    "origin": "registry:get_task",
    "hint": "Try again"
  }
}
```

### 10.3 Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Command error (invalid args, precondition failed) |
| 2 | Resource not found |
| 3 | Conflict or invalid state |
| 4 | Permission denied |
| 10+ | System errors |

---

## 11. Invariants

CLI operational semantics guarantee:

- Every command has defined preconditions
- Every command has defined effects
- Effects only occur if preconditions met
- Failures are reported with structured errors
- Idempotence behavior is documented
- Events are emitted for state-changing operations

Violating these invariants is a SystemError.

---

## 12. Summary

This document defines **what happens** when CLI commands execute:

- **Preconditions:** What must be true
- **Effects:** What changes
- **Events:** What is recorded
- **Failures:** What can go wrong

The CLI is not just an interface. It is a contract.

> If the CLI says it happened, it happened. If it says it failed, it failed.
> There is no ambiguity.
