# Hivemind

**A local-first, observable orchestration system for agentic software development.**

[![Documentation](https://docs.rs/hivemind-core/badge.svg)](https://docs.rs/hivemind-core)
[![Crates.io](https://img.shields.io/crates/v/hivemind-core.svg)](https://crates.io/crates/hivemind-core)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Hivemind coordinates planning, execution, verification, and integration for autonomous agents working on real codebases. It treats agentic work as **engineering work** — deserving of explicit planning, deterministic structure, observable execution, auditable outcomes, and human authority at critical boundaries.

## Why Hivemind?

AI coding agents are increasingly capable — but existing tools suffer from:

| Problem | Hivemind's Solution |
|---------|---------------------|
| Opaque execution | **Event-sourced architecture** — every action produces an auditable event |
| Poor failure handling | **Structured errors** with categories, codes, and recovery hints |
| Unsafe parallelism | **Scoped execution** with fine-grained access control |
| Unclear attribution | **Full diff/commit history** tied to tasks and attempts |
| Limited undo | **Deterministic state** — rebuild from events at any point |

## Core Philosophy

> **Agents execute. Systems govern. Humans decide.**

Hivemind is built on non-negotiable principles:

- **Observability is truth** — every change has a trail
- **Structure enables scale** — graphs, flows, and scopes provide guardrails
- **Failures are first-class outcomes** — retry, escalate, or abort explicitly
- **Humans decide what ships** — merge gates and review boundaries
- **Runtimes are replaceable** — adapters abstract execution backends

## Installation

```bash
cargo install hivemind-core

# Verify installation
hivemind --version
```

The binary is installed to `~/.cargo/bin`. Add it to your `PATH` if needed.

## Quick Start

```bash
# Create a project
hivemind project create my-app --repo /path/to/repo

# List projects
hivemind project list

# Create a task graph
hivemind graph create my-app --file graph.yaml

# Run a flow
hivemind flow run my-app --graph <graph-id>

# Check status
hivemind flow status <flow-id>

# Start the HTTP server for UI
hivemind serve --port 8787
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Hivemind                             │
├─────────────────────────────────────────────────────────────┤
│  CLI        │  Server     │  Library                        │
│  (clap)     │  (HTTP API) │  (this crate)                   │
├─────────────────────────────────────────────────────────────┤
│                    Core Domain Model                        │
│  Events → State → Actions                                   │
│  Graphs, Flows, Scopes, Verification                        │
├─────────────────────────────────────────────────────────────┤
│  Adapters                                                   │
│  OpenCode │ Claude Code │ Codex │ Kilo                      │
├─────────────────────────────────────────────────────────────┤
│  Storage                                                    │
│  SQLite Event Store (append-only, queryable)                │
└─────────────────────────────────────────────────────────────┘
```

## Key Concepts

### Event Sourcing

All state is derived from immutable events. This ensures:

- **Determinism**: Same events → same state, always
- **Idempotency**: Operations can be safely retried
- **Auditability**: Complete history of all changes
- **Time travel**: Query state at any point in history

### TaskGraphs and TaskFlows

- **TaskGraph**: Static, immutable DAG representing planned intent (what should happen)
- **TaskFlow**: Runtime instance with execution state (what is happening/happened)

Multiple flows can exist for the same graph, enabling retries, parallel attempts, and A/B execution strategies.

### Scoped Execution

Every task runs within a defined scope that specifies:

- Which repositories the agent can access
- What file patterns (include/exclude)
- What operations (read, write, delete)
- Whether network access is permitted

### Runtime Adapters

Hivemind supports multiple execution backends:

| Adapter | Binary | Notes |
|---------|--------|-------|
| OpenCode | `opencode` | Primary adapter, full feature support |
| Claude Code | `claude` | Anthropic's Claude CLI |
| Codex | `codex` | OpenAI's Codex CLI |
| Kilo | `kilo` | Custom adapter |

## Library Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
hivemind-core = "0.1"
```

### Example: Creating a Project

```rust
use hivemind::core::registry::Registry;

let registry = Registry::open()?;
let project = registry.create_project("my-project", "/path/to/repo")?;
println!("Created project: {}", project.id);
```

### Example: Defining a Task Graph

```rust
use hivemind::core::graph::{TaskGraph, GraphTask, SuccessCriteria};

let task = GraphTask::new(
    "Implement authentication",
    SuccessCriteria::new("Users can log in and log out")
);

let graph = TaskGraph::new("auth-feature")
    .with_task(task);
```

## Runtime Prerequisites

- Rust toolchain (for `cargo install`)
- Git 2.40+
- At least one runtime adapter binary on `PATH` (e.g., `opencode`)

## Documentation

| Resource | URL |
|----------|-----|
| **Canonical Docs** | <https://hivemind-landing.netlify.app/> |
| **Quickstart Guide** | <https://hivemind-landing.netlify.app/docs/overview/quickstart> |
| **API Reference** | <https://docs.rs/hivemind-core> |
| **Repository** | <https://github.com/Antonio7098/hivemind> |

## Project Status

Hivemind is under active development. The architecture and design are intentionally documented **before** heavy implementation to avoid ambiguity, hidden assumptions, and unsafe shortcuts.

## License

MIT
