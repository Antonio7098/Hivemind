---
title: Multi-Repo Integration
description: Working with multiple repositories
order: 4
---

# Hivemind — Multi-Repo Integration

> **Principle 2:** Fail fast, fail loud, fail early.
> **Principle 10:** Failures are first-class outcomes.

This document specifies how Hivemind handles **multi-repository TaskFlows**: execution, atomicity, and failure modes when tasks span multiple repositories.

Multi-repo execution is inherently complex. This document is honest about that complexity and defines explicit rules for handling it.

---

## 1. Multi-Repo Execution Model

### 1.1 Definition

A **multi-repo TaskFlow** is a TaskFlow where:
- The project contains multiple repositories
- One or more tasks modify multiple repositories
- OR different tasks modify different repositories

### 1.2 Fundamental Challenge

Git repositories are independent:
- Each repo has its own history
- Cross-repo atomicity does not exist in git
- A change in repo A cannot be transactionally linked to repo B

Hivemind must define explicit rules for handling this reality.

---

## 2. Execution Semantics

### 2.1 Per-Repo Isolation

Each repository gets:
- Its own execution branch per task
- Its own worktree per task
- Its own commit history

Cross-repo changes are coordinated by Hivemind, not by git.

### 2.2 Task Scope Across Repos

A single task may have scope in multiple repositories:

```yaml
task:
  scope:
    repos:
      - name: backend
        access: read-write
        paths: ["src/**", "tests/**"]
      - name: frontend
        access: read-write
        paths: ["src/api/**"]
```

### 2.3 Execution Order

Within a multi-repo task:
1. Worktrees created for all repos
2. Agent executes with access to all worktrees
3. Changes observed in all repos
4. Scope verified per repo
5. Verification considers all repos

### 2.4 Runtime Surface (Sprint 27)

For each attempt, Hivemind provides multi-repo paths to the runtime via:
- Primary working directory: first attached repository task worktree
- Context block listing all repo worktree paths by repo name
- Environment variables:
  - `HIVEMIND_PRIMARY_WORKTREE`
  - `HIVEMIND_ALL_WORKTREES` (`name=path;name=path`)
  - `HIVEMIND_REPO_<REPO_NAME>_WORKTREE` (sanitized uppercase key per repo)

---

## 3. Atomicity Model

### 3.1 The Atomicity Problem

Consider:
- Task T1 modifies both backend and frontend repos
- T1 succeeds
- Merge to backend succeeds
- Merge to frontend fails (conflict)

**Result:** Backend has changes that assume frontend changes exist, but frontend doesn't have them.

### 3.2 Hivemind's Atomicity Rule

> **Multi-repo merge is all-or-nothing at the TaskFlow level, not the task level.**

This means:
- Individual tasks may succeed independently
- Merge does not happen until TaskFlow completes
- If any repo fails to merge, no repos merge

### 3.3 Atomicity Levels

Hivemind supports configurable atomicity:

**Level 1: TaskFlow Atomic (Default)**
- No merges until entire TaskFlow succeeds
- All repos merge together or none merge
- Safest option

**Level 2: Task Atomic**
- Each task's changes merge independently
- Cross-task dependencies at user's risk
- Faster but riskier

**Level 3: Repo Atomic**
- Each repo merges independently
- No cross-repo coordination
- Not recommended for coupled repos

---

## 4. Merge Protocol

### 4.1 TaskFlow Atomic Merge (Default)

```
TaskFlow Completes Successfully
    ↓
Merge Preparation (all repos)
    ↓
Conflict Check (all repos)
    ↓
[If any conflict] → Merge Blocked
    ↓
Human Approval
    ↓
Merge Execution (all repos, sequential)
    ↓
[If any merge fails] → Rollback attempted, report failure
    ↓
Merge Complete
```

### 4.2 Merge Preparation

For each repo:
1. Collect all successful task branches
2. Compute integration commit(s)
3. Test merge against target branch
4. Report conflicts if any

### 4.3 Conflict Detection

Before any merge executes:
```
for repo in project.repos:
    conflicts = git.merge_test(repo, integration_branch, target_branch)
    if conflicts:
        report_conflict(repo, conflicts)
        block_all_merges()
```

### 4.4 Merge Execution

If all repos pass conflict check:
```
merged_repos = []
try:
    for repo in project.repos:
        git.merge(repo, integration_branch, target_branch)
        merged_repos.append(repo)
except MergeError as e:
    for merged_repo in merged_repos:
        git.revert_merge(merged_repo)  # Best effort
    report_partial_failure(e, merged_repos)
```

### 4.5 Rollback Limitations

**Honest Statement:** Perfect rollback is not always possible.

If repo A merges but repo B fails:
- Attempt to revert repo A merge
- If repo A has new commits since merge, revert may conflict
- Report situation to human

This is a fundamental limitation of distributed version control.

---

## 5. Failure Modes

### 5.1 Pre-Merge Failures

**Task Failure:**
- One task in the TaskFlow fails
- TaskFlow enters FAILED state
- No repos merge
- Clean state preserved

**Verification Failure:**
- Task changes rejected by verification
- Retry or fail per policy
- No merge until success

### 5.2 Merge-Time Failures

**Conflict in One Repo:**
- Detected during merge preparation
- All repos blocked from merge
- Human resolves conflict
- Retry merge preparation

**Merge Execution Failure:**
- Rare: conflict check passed but merge failed
- Partial state possible
- Best-effort rollback
- Human intervention required

### 5.3 Post-Merge Failures

**Integration Test Failure:**
- Changes merged but integration tests fail
- This is **out of scope** for Hivemind atomicity
- Standard git revert workflow applies

---

## 6. Partial Success States

### 6.1 Definition

A **partial success state** occurs when:
- Some repos have merged changes
- Other repos do not
- Cross-repo consistency is broken

### 6.2 Detection

Hivemind tracks merge state per repo:

```
MergeState:
  taskflow_id: string
  repos:
    - name: backend
      status: MERGED
      commit: abc123
    - name: frontend
      status: FAILED
      error: "Merge conflict in src/api/client.ts"
```

### 6.3 Reporting

Partial success emits:
```
MultiRepoMergePartialFailure:
  taskflow_id: string
  merged_repos: [backend]
  failed_repos: [frontend]
  requires_human_intervention: true
```

### 6.4 Resolution

Human must:
1. Decide whether to revert merged repos
2. Resolve conflicts in failed repos
3. Complete merge manually
4. Update Hivemind state

---

## 7. Cross-Repo Dependencies

### 7.1 Implicit Dependencies

Sometimes repo changes are implicitly coupled:
- Backend API change requires frontend update
- Library change requires consumer update

Hivemind cannot detect implicit dependencies automatically.

### 7.2 Explicit Dependency Declaration

Users may declare cross-repo task dependencies:

```yaml
taskflow:
  tasks:
    - id: update-api
      repos: [backend]

    - id: update-client
      repos: [frontend]
      depends_on: [update-api]  # Explicit ordering
```

### 7.3 Dependency Enforcement

Cross-repo dependencies are enforced:
- update-client cannot start until update-api succeeds
- If update-api fails, update-client remains PENDING

---

## 8. Worktree Strategy

### 8.1 Multi-Repo Worktree Layout

```
.hivemind/worktrees/
  └── taskflow-42/
      └── task-update-api/
          ├── backend/     ← backend repo worktree
          └── frontend/    ← frontend repo worktree
```

### 8.2 Agent Access

Agent receives paths to all repo worktrees in scope:

```
HIVEMIND_WORKTREE_BACKEND=/path/to/backend
HIVEMIND_WORKTREE_FRONTEND=/path/to/frontend
```

### 8.3 Path Translation

Agent instructions use logical repo names:
```
Modify backend:src/api/handler.py
Modify frontend:src/api/client.ts
```

Hivemind translates to actual paths.

---

## 9. Scope Enforcement

### 9.1 Per-Repo Scope

Scope is evaluated per repository:
- backend scope violations fail the task
- frontend scope violations fail the task
- Violations in either repo are fatal

### 9.2 Cross-Repo Scope Conflicts

If task T1 and T2 both modify the same repo:
- Standard scope conflict rules apply
- Soft/hard conflict determined by path overlap
- Isolation via separate worktrees if needed

---

## 10. Events

### 10.1 Multi-Repo Events

```
MultiRepoTaskStarted:
  task_id: string
  repos: [string]

MultiRepoCheckpointCreated:
  task_id: string
  checkpoints:
    - repo: backend
      commit: abc123
    - repo: frontend
      commit: def456

MultiRepoMergeStarted:
  taskflow_id: string
  repos: [string]

MultiRepoMergeCompleted:
  taskflow_id: string
  results:
    - repo: backend
      status: SUCCESS
      commit: merged123
    - repo: frontend
      status: SUCCESS
      commit: merged456

MultiRepoMergePartialFailure:
  taskflow_id: string
  merged_repos: [string]
  failed_repos: [string]
```

---

## 11. Configuration

### 11.1 Project-Level Config

```yaml
project:
  multi_repo:
    atomicity_level: taskflow_atomic  # or task_atomic, repo_atomic

    merge_strategy:
      conflict_check: pre_merge  # Check all before merging any
      rollback_on_failure: true

    repos:
      - name: backend
        path: /path/to/backend
        target_branch: main

      - name: frontend
        path: /path/to/frontend
        target_branch: main
```

### 11.2 TaskFlow-Level Override

```yaml
taskflow:
  multi_repo:
    atomicity_level: task_atomic  # Override for this flow
```

---

## 12. Invariants

Multi-repo integration guarantees:

- Each repo gets independent execution branches
- Scope is enforced per repo
- Default atomicity is TaskFlow-level (all or nothing)
- Partial success states are detected and reported
- Human intervention is required for merge conflicts
- Rollback is best-effort, not guaranteed

Violating these invariants is a SystemError.

---

## 13. Limitations

### 13.1 What Hivemind Cannot Do

- Guarantee atomic commits across repos (git limitation)
- Automatically resolve merge conflicts
- Detect implicit cross-repo dependencies
- Guarantee perfect rollback

### 13.2 Why This Is Acceptable

Per Principle 2: "Failures surface immediately"
- Partial states are detected and reported
- Human is always in the loop for merge

Per Principle 10: "Failures are first-class outcomes"
- Multi-repo merge failure is an expected outcome
- Failure handling is explicit, not hidden

---

## 14. Summary

Multi-repo integration in Hivemind:

- **Executes** tasks across repos with isolated worktrees
- **Coordinates** via TaskFlow-level atomicity by default
- **Detects** conflicts before merge execution
- **Reports** partial success states explicitly
- **Requires** human intervention for conflicts

> Cross-repo atomicity is a coordination problem, not a git problem.
> Hivemind coordinates. Git stores. Humans decide.
