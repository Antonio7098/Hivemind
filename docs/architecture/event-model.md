---
title: Event Model
description: Event sourcing and replay semantics
order: 4
---

# Hivemind — Event Model (events.md)

> **Core rule:** If it changes state, it emits an event.

This document defines the **event model** for Hivemind: what events exist, what they mean, and how they are used to derive state, drive execution, and power observability.

This is a **conceptual taxonomy**, not a wire format or schema.

---

## 1. Why an Event Model

Hivemind is an **event-native system**.

All meaningful behavior is expressed as events so that the system is:
- Observable
- Replayable
- Debuggable
- Pausable and resumable

Events are the **single source of truth**. All state is derived from them.

---

## 2. Event Principles

Every Hivemind event follows these principles:

1. **Append-only**  
   Events are never mutated or deleted.

2. **Causally ordered**  
   Ordering matters within a TaskFlow.

3. **Attributed**  
   Every event has an actor (agent, system, or human).

4. **Minimal but sufficient**  
   Events describe *what happened*, not interpretations.

5. **Replay-safe**  
   Replaying events must deterministically reconstruct state.

---

## 3. Event Scope & Correlation

Every event is correlated to one or more of the following:

- Project
- TaskGraph
- TaskFlow
- Task
- Attempt
- Repository

Correlation identifiers allow:
- Filtering
- Aggregation
- UI projection
- Debugging

For governance workflows, operators also filter by payload-level selectors (`artifact_id`, `template_id`, `rule_id`) without relying on hidden indexes.

---

## 4. Event Categories

Hivemind events are grouped by **semantic category**.

---

## 5. Project Events

These events describe changes to long-lived project context.

Examples:
- `ProjectCreated`
- `ProjectUpdated`
- `RepositoryAttachedToProject`
- `RepositoryDetachedFromProject`

Project events are **infrequent** and non-executional.

### 5.1 Governance / Context Artifact Events

Governance artifacts are first-class project/global state and emit explicit lifecycle events.

Examples:
- `GovernanceProjectStorageInitialized`
- `GovernanceStorageMigrated`
- `GovernanceArtifactUpserted`
- `GovernanceArtifactDeleted`
- `GovernanceAttachmentLifecycleUpdated`
- `GovernanceSnapshotCreated`
- `GovernanceSnapshotRestored`
- `GovernanceDriftDetected`
- `GovernanceRepairApplied`
- `GraphSnapshotStarted`
- `GraphSnapshotCompleted`
- `GraphSnapshotFailed`
- `GraphSnapshotDiffDetected`
- `ConstitutionInitialized`
- `ConstitutionUpdated`
- `ConstitutionValidated`
- `ConstitutionViolationDetected`
- `TemplateInstantiated`
- `AttemptContextOverridesApplied`
- `AttemptContextAssembled`
- `AttemptContextTruncated`
- `AttemptContextDelivered`

Properties:
- Global and project scopes are explicit in payloads (`scope: global|project`)
- Attachment lifecycle is task-scoped and auditable
- Graph snapshot lifecycle emits explicit start/completion/failure and diff telemetry with trigger attribution (`project_attach`, `checkpoint_complete`, `merge_completed`, `manual_refresh`)
- Snapshot completion payload records snapshot path, governance revision, UCP profile/engine metadata, and canonical fingerprint for replay-safe provenance
- Governance recovery snapshot events record snapshot identity, coverage size, and source event sequence used for bounded restore
- Drift/repair events expose recoverable vs unrecoverable issue counts and applied operation totals for operator audit trails
- Constitution lifecycle events include digest, schema/version metadata, confirmation flag, mutation intent, and actor attribution
- Constitution enforcement emits `ConstitutionViolationDetected` with gate (`manual_check`, `checkpoint_complete`, `merge_prepare`, `merge_approve`, `merge_execute`), rule ID/type/severity, and structured evidence/remediation hints
- Template instantiation records resolved artifact IDs for replay-safe context provenance
- Attempt context assembly emits immutable manifest, input hash, delivery hash, and override/truncation telemetry for replay-safe attempt provenance

---

## 6. TaskGraph Events (Planning)

These events describe intent and structure.

Examples:
- `TaskGraphCreated`
- `TaskAddedToGraph`
- `DependencyAdded`
- `ScopeAssigned`

Properties:
- Emitted during planning
- Immutable once execution starts
- Versioned

---

## 7. TaskFlow Lifecycle Events

These events describe TaskFlow-level execution control.

Examples:
- `TaskFlowStarted`
- `TaskFlowPaused`
- `TaskFlowResumed`
- `TaskFlowCompleted`
- `TaskFlowAborted`

TaskFlow lifecycle events gate scheduling but do not perform work.

---

## 8. Task Scheduling Events

These events describe when tasks become eligible to run.

Examples:
- `TaskReady`
- `TaskBlocked`
- `TaskUnblocked`

These events are derived from dependency resolution.

---

## 9. Task Execution Events

These events describe state transitions of individual tasks.

Examples:
- `TaskExecutionStarted`
- `TaskExecutionStateChanged`
- `TaskExecutionSucceeded`
- `TaskExecutionFailed`

These events correspond to the TaskExecution FSM.

---

## 10. Attempt Events

Attempts represent concrete worker executions.

Examples:
- `AttemptStarted`
- `AttemptCompleted`
- `AttemptCrashed`
- `RetryContextAssembled`
- `AttemptContextOverridesApplied`
- `AttemptContextAssembled`
- `AttemptContextTruncated`
- `AttemptContextDelivered`

Properties:
- Attempts are append-only
- Each attempt belongs to a single task
- Context inputs are explicit and hash-addressed
- Retry attempts reference prior attempt manifest hashes explicitly

---

## 11. Agent Events

These events describe agent-level behavior.

Examples:
- `AgentInvoked`
- `AgentOutputProduced`
- `AgentTerminated`

Agent events capture *interaction*, not internal reasoning.

---

## 12. Runtime Events

These events describe interactions with execution runtimes.

Examples:
- `RuntimeStarted`
- `RuntimeOutputChunk`
- `RuntimeInputProvided`
- `RuntimeInterrupted`
- `RuntimeFilesystemObserved`
- `RuntimeExited`
- `RuntimeError`

Runtime events are intentionally coarse-grained.

---

## 13. Scope & Safety Events

These events describe enforcement and violations.

Examples:
- `ScopeValidated`
- `ScopeConflictDetected`
- `TaskSchedulingDeferred`
- `ScopeViolationDetected`

Scope events are **safety-critical** and never ignored.

---

## 14. Filesystem & Diff Events

These events describe observable side effects.

Examples:
- `FileModified`
- `CheckpointCommitCreated`
- `DiffComputed`

Filesystem events are always attributable to:
- Task
- Attempt
- Scope

---

## 15. Verification Events

These events describe evaluation and judgment.

Examples:
- `VerificationStarted`
- `VerificationPassed`
- `VerificationFailed`
- `VerificationAborted`

Verification events are authoritative.

---

## 16. Retry & Control Events

These events describe execution control flow.

Examples:
- `RetryScheduled`
- `RetryLimitReached`
- `TaskEscalated`

These events prevent infinite loops and encode policy.

---

## 17. Merge Events

These events describe integration governance.

Examples:
- `MergePrepared`
- `MergeApproved`
- `MergeRejected`
- `MergeCompleted`

Merge events are distinct from execution success.

---

## 18. Human Intervention Events

These events describe explicit human actions.

Examples:
- `HumanPausedTaskFlow`
- `HumanResumedTaskFlow`
- `HumanAbortedTask`
- `HumanApprovedMerge`

Human events always override automation.

---

## 19. Derived State & Projections

State is **not stored directly**.

Instead:
- State is derived by replaying events
- UI views are projections
- Metrics are aggregations

No component may mutate state without emitting events.

---

## 20. Event Replay Guarantees

Hivemind guarantees that:

- Replaying events in order deterministically reconstructs **orchestration state** (TaskFlow position, task states, attempt history, decisions)
- Replaying does not re-trigger side effects
- Execution is idempotent with respect to events

**Important:** Event replay reconstructs *logical state*, not *execution artifacts*. Files, commits, and diffs are referenced by events but not contained in them. These artifacts must be preserved separately. Governance files are recoverable only when bounded governance snapshots are available and revision-compatible with current event authority.

This enables:
- Crash recovery
- Debugging
- Historical inspection

See `docs/design/event-replay.md` for detailed explanation of what can and cannot be reconstructed.

---

## 21. Execution Progress Signaling (Agent Status)

Agents may emit **execution progress signals**, but only in structured, observable form.

### Allowed Progress Signals

Agents may emit events such as:

- `AttemptStarted`
- `CheckpointReached`
- `ToolInvocationCompleted`
- `WaitingOnExternalProcess`
- `VerificationPending`

These signals represent **facts about execution**, not subjective assessments.

### Disallowed Status Updates

Agents must not emit:

- free-form progress narration
- speculative statements (e.g. “almost done”)
- unverifiable claims
- continuous chatter

### Design Rationale

Progress signaling exists to support:

- observability
- automation
- deadlock detection
- human confidence

It does not exist to simulate human conversation.

**Agents report facts. The system derives status.**

---

## 22. What Events Intentionally Avoid

Events do **not** contain:
- Model chain-of-thought
- Hidden runtime memory
- Unobservable state

Events describe *facts*, not speculation.

---

## 23. Summary

The event model is the **spine of Hivemind**.

By expressing all behavior as structured, replayable events, Hivemind achieves:
- Trust
- Transparency
- Control
- Evolvability

Everything else — state, UI, automation — is a consequence of this model.
