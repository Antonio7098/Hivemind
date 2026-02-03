# Hivemind

Hivemind is a **local-first, observable orchestration system for agentic software development**.

It is designed to make parallel AI agent work on real codebases **safe, legible, and governable** — without hiding complexity or surrendering human control.

Hivemind is not an AI that codes.  
It is a system that makes agentic work **real**.

---

## Why Hivemind Exists

AI coding agents are increasingly capable — but existing tools suffer from:

- Opaque execution
- Poor failure handling
- Unsafe parallelism
- Unclear attribution of changes
- Limited undo and recovery

When something goes wrong, developers are left guessing.

Hivemind treats agentic work as **engineering work**, deserving of:
- explicit planning
- deterministic structure
- observable execution
- auditable outcomes
- human authority at critical boundaries

---

## Core Principles

Hivemind is built on a small set of non-negotiable principles:

- **Observability is truth**
- **Structure enables scale**
- **Failures are first-class outcomes**
- **Humans decide what ships**
- **Runtimes are replaceable**
- **No magic. Everything has a reason and a trail**

These principles are enforced through architecture, not convention.

---

## What Hivemind Provides

- Projects spanning one or more repositories
- Tasks and simple todos
- Deterministic TaskGraphs (planning)
- Executable TaskFlows (execution)
- Scoped, parallel agent execution
- Explicit verification and retries
- Full diff, commit, and event history
- CLI-first interface (UI is a projection)

Everything runs **locally**.

---

## Documentation Map

Start here:

docs/overview/
├── vision.md # Why Hivemind exists
├── principles.md # Design doctrine
└── prd.md # Product guarantees

kotlin
Copy code

Then continue to:

docs/architecture/ # System shape & invariants
docs/design/ # Concrete mechanics & semantics

yaml
Copy code

---

## Project Status

Hivemind is under active development.

The architecture and design are intentionally documented **before** heavy implementation to avoid ambiguity, hidden assumptions, and unsafe shortcuts.

---

## Philosophy

> Agents execute.  
> Systems govern.  
> Humans decide.

If that resonates with you, you’re in the right place.
