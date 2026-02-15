# Hivemind — Design & Operational Semantics

This directory contains **concrete design decisions and operational mechanics**.

These documents answer:

> *Given the architecture, how does this actually work?*

They are intentionally more detailed, more practical, and more likely to evolve than the architecture documents.

---

## What Belongs Here

Design documents cover:

- CLI operational semantics
- Error taxonomy and handling
- Runtime wrapper mechanics
- Scope enforcement strategies
- Verification authority and flow
- Retry context assembly
- Event replay semantics
- Multi-repo integration behavior

These documents are allowed to:
- Be sprint-aware (Sprint 1 vs Sprint 2)
- Discuss limitations honestly
- Reference OS or runtime constraints
- Evolve as implementation progresses

---

## What Does NOT Belong Here

Design docs should not:
- Redefine architectural invariants
- Contradict principles
- Imply guarantees the architecture does not make

If a design decision violates an architectural invariant, the architecture must be updated first.

---

## Directory Structure

Typical contents include:

- `hivemind_cli_operational_semantics.md`
- `error-model.md`
- `runtime-wrapper-mechanics.md`
- `scope-enforcement.md`
- `verification-authority.md`
- `retry-context.md`
- `event-replay-semantics.md`
- `multi-repo-integration.md`

---

## Stability Expectations

Design documents are:
- Expected to evolve
- Updated alongside implementation
- Safe to revise as tradeoffs change

They should remain:
- Explicit
- Honest
- Testable against real behavior

---

## Relationship to Architecture

- Architecture defines **what must be true**
- Design defines **how it is achieved today**

Design documents may reference architecture freely.  
Architecture documents should not depend on design details.

---

## Contribution Guideline

When adding or updating a design document:
- Be explicit about limits
- Prefer detection over hidden failure
- Avoid “magic” behavior
- Document failure modes

Design is where principles meet reality.
