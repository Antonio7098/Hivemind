# Hivemind — Architecture

This directory defines the **architectural shape and invariants** of Hivemind.

These documents answer:

> *What must always be true about the system, regardless of implementation?*

They describe **boundaries, responsibilities, state models, and guarantees** — not code-level mechanics.

---

## What Belongs Here

Architecture documents define:

- Core concepts and abstractions
- State ownership and lifecycle
- Execution and verification semantics
- System invariants
- Authority boundaries
- What the system guarantees (and what it does not)

They intentionally avoid:
- OS-specific details
- Runtime implementation tricks
- Phase-specific limitations
- Command-level behavior

---

## Document Overview

Key documents in this directory include:

- `architecture.md`  
  High-level system overview and invariants

- `hivemind_state_model.md`  
  What state exists, how it evolves, and what is persisted

- `hivemind_event_model.md`  
  Event taxonomy and observability model

- `hivemind_task_flow.md`  
  TaskGraph vs TaskFlow, execution semantics

- `hivemind_scope_model.md`  
  Scope contracts and conflict rules

- `hivemind_commit_branch_model.md`  
  Execution vs integration commits, branch strategy

- `hivemind_runtime_adapters.md`  
  Runtime abstraction boundaries

---

## Stability Expectations

Architecture documents are:
- **Relatively stable**
- Changed deliberately
- Updated only when core guarantees evolve

If you find yourself wanting to add:
- implementation detail
- OS mechanics
- “how we do it right now”

You are probably looking for `docs/design/`.

---

## Relationship to Other Docs

- Product guarantees are defined in `docs/overview/prd.md`
- Design and mechanics live in `docs/design/`
- Architecture constrains design, not the other way around

---

## Contribution Guideline

Changes to architecture documents should:
- Preserve stated principles
- Avoid over-specification
- Be justified by clear invariants

Architecture is the system’s **constitution**.
Treat it accordingly.
