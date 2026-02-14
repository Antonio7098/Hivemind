---
title: Hivemind Documentation
description: A local-first orchestration system for AI agents that work on real codebases
order: 0
---

# Welcome to Hivemind

Hivemind is a **local-first orchestration system** for AI agents working on real codebases.

It is built for teams that need to scale agent autonomy without losing control.

**Everything observable.**
**Everything deterministic.**
**Everything reversible.**

## What You Get

- Deterministic task planning (`graph`) and execution (`flow`)
- Scoped, parallel agents with explicit permissions
- Event-native execution history (`events`) for inspection and replay
- Explicit verification and merge gates (`verify`, `merge`)
- Full CLI automation with machine-readable output

## Quick Start

```bash
# From the Hivemind repository
cargo build --release

# Confirm CLI is available
./target/release/hivemind version

# Create a project
./target/release/hivemind project create "demo" --description "My first Hivemind project"
```

For an end-to-end setup (project, tasks, graph, flow, merge), continue with [Quickstart Guide](./overview/quickstart.md).

## Documentation Structure

- `overview/` — product vision, guarantees, principles, and onboarding
- `architecture/` — system model and core invariants
- `design/` — operational semantics for advanced usage
