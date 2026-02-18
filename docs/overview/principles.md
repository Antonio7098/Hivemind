---
title: Core Principles
description: Foundational principles and invariants
order: 2
---

# Hivemind Principles

> These principles are non-negotiable.
> They guide architecture, implementation, UX, and evolution.
> When trade-offs arise, these principles decide.

---

## 1. Observability Is Truth

If something happened, it must be observable.
If it is not observable, it is treated as if it did not happen.

* All meaningful behavior emits events
* All state is derived from events
* Canonical event/state persistence is transactional (`~/.hivemind/db.sqlite`), not ad hoc files
* All execution is inspectable and replayable
* There is no hidden system state

Observability is not a debugging feature.
It is the foundation of trust.

---

## 2. Fail Fast, Fail Loud, Fail Early

Failures are expected. Silent failures are not.

* Errors surface immediately
* Partial or ambiguous success is not accepted
* Unsafe states halt execution
* Escalation is explicit

Hivemind prefers early failure over delayed corruption.

---

## 3. Reliability Over Cleverness

Correctness matters more than novelty.

* Deterministic planning over heuristic execution
* Explicit rules over implicit behavior
* Bounded retries over infinite loops
* Safe defaults over permissive ones

A boring system that works is better than a clever system that surprises.

---

## 4. Explicit Error Taxonomy

Errors must be classifiable, attributable, and actionable.

* Errors are structured, not stringly-typed
* Failure modes are named and documented
* Verification failures differ from execution failures
* System errors differ from agent errors

Every error should answer:

> What failed? Why? What can be done next?

---

## 5. Structure and Process Are How Agents Scale

Unstructured agents do not scale.
Structure is not overhead — it is leverage.

* Tasks have boundaries
* Scopes define capability
* Execution follows state machines
* Verification is explicit

Agents operate inside systems.
Systems scale. Prompts do not.

---

## 6. SOLID Principles Are Doctrine

Hivemind treats software engineering fundamentals as **non-optional**.

* Single responsibility
* Explicit interfaces
* Replaceable components
* Clear ownership boundaries

Agentic systems do not excuse poor engineering.
They demand better engineering.

---

## 7. CLI-First Is Non-Negotiable

If it cannot be done via the CLI, it is not a real feature.

* All core functionality is exposed via CLI
* All CLI output is machine-readable
* All state transitions are explicit
* The UI is a projection, not a controller

This enables:

* autonomous coding agents
* deterministic testing
* headless execution
* future meta-orchestration

---

## 8. Absolute Observability, Everywhere

Not “good” observability.
**Absolute** observability.

* Structured logging by default
* Wide, explicit events
* No silent side effects
* No hidden retries
* No invisible merges

Debugging should feel like inspection, not archaeology.

---

## 9. Automated Checks Are Mandatory

Trust is earned continuously.

* Automated verification is the default
* Manual intervention is explicit
* Checks gate progression
* Regressions are detectable

Humans review outcomes, not chaos.

---

## 10. Failures Are First-Class Outcomes

Failure is a valid state.

* Failed tasks are preserved
* Partial progress is inspectable
* Retry history is visible
* No work disappears

A system that hides failure cannot be trusted.

---

## 11. Build Incrementally, Prove Foundations First

Complexity is earned.

* Start minimal
* Prove invariants
* Expand deliberately
* Never outpace observability

Hivemind grows **layer by layer**, not feature by feature.

---

## 12. Maximum Modularity, Minimum Lock-In

Hivemind must remain evolvable.

* Runtimes are replaceable
* Execution backends are swappable
* Architecture does not depend on specific models
* Abstractions are intentional, not accidental

Today’s implementation must not constrain tomorrow’s design.

---

## 13. Abstraction Without Loss of Control

Structure should exist even if users don’t see it.

* Planning agents may abstract complexity
* Internals remain explicit and enforceable
* Power users can always drop down a level

Hivemind hides complexity **without eliminating structure**.

---

## 14. Human Authority at Critical Boundaries

Agents execute. Humans decide.

* Verification gates success
* Merges are explicit
* Escalation is deliberate
* Nothing ships by accident

Speed is useful. Authority is essential.

---

## 15. No Magic

If the system cannot explain itself, it is wrong.

* No hidden context
* No implicit state
* No unexplained decisions

Everything has a reason.
Everything has a trail.

---
## Closing Statement

Hivemind is not built to impress.
It is built to **endure**.

These principles exist to ensure that as intelligence becomes cheaper and faster, **control, trust, and correctness do not disappear**.

When in doubt:

* choose explicitness
* choose observability
* choose structure
* choose safety

Everything else is secondary.
