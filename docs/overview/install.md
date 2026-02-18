---
title: Install Hivemind
description: Install the hivemind-core crate and prepare your environment
order: 2
---

# Install Hivemind

`hivemind-core` ships the `hivemind` CLI via crates.io. The canonical documentation lives at [hivemind-landing.netlify.app](https://hivemind-landing.netlify.app), with the authoritative quickstart at [hivemind-landing.netlify.app/docs/overview/quickstart](https://hivemind-landing.netlify.app/docs/overview/quickstart).

## Prerequisites

- Rust toolchain with `cargo` in `PATH`
- Git 2.40+
- A runtime adapter available for executing flows:
  - **OpenCode**: `npm install -g opencode` (recommended)
  - **Codex CLI**: `npm install -g @openai/codex`
  - **Claude Code**: `npm install -g @anthropic-ai/claude-code`
  - **Kilo**: See project documentation
- Optional: `~/.cargo/bin` added to your `PATH`

## Install via crates.io

```bash
cargo install hivemind-core

# confirm installation
hivemind --version
```

`cargo install` places binaries in `~/.cargo/bin` by default. Add the directory to your shell `PATH` if necessary:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## Verifying the CLI

```bash
hivemind -f json version
```

A successful response returns structured metadata containing the git SHA, build timestamp, and enabled adapters.

## Upgrading

```bash
cargo install hivemind-core --force
```

## Next steps

1. Follow the [Quickstart Guide](./quickstart.md) for an end-to-end run.
2. Review [Vision](./vision.md) and [Principles](./principles.md) to internalize the operating model.
3. Use [Governance Runbooks](./governance-runbooks.md) for migration, recovery, and policy-change operations.
4. Keep `hivemind-core` up to date with `cargo install --force` when new releases drop.
