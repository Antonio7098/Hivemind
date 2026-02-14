# Hivemind Core

`hivemind-core` is the crates.io distribution of the Hivemind CLI and orchestration engine.

- **Public site:** <https://hivemind-landing.netlify.app/>
- **Canonical docs + quickstart:** <https://hivemind-landing.netlify.app/docs/overview/quickstart>

- **Crate name:** `hivemind-core`
- **Binary name:** `hivemind`
- **Source:** <https://github.com/Antonio7098/Hivemind>

Hivemind is a **local-first, observable orchestration system** for agentic software development. It coordinates planning, execution, verification, and integration for autonomous agents working on real repositories.

## Install

```bash
cargo install hivemind-core

# Confirm installation
hivemind --version
```

`cargo install` places the `hivemind` binary in Cargo's bin directory (usually `~/.cargo/bin`). Add that directory to `PATH` if it is not already present.

To upgrade an existing installation:

```bash
cargo install hivemind-core --force
```

## Runtime prerequisites

- Rust toolchain (for `cargo install`)
- Git 2.40+
- At least one runtime adapter binary (e.g., `opencode`) on `PATH`

## First run check

```bash
hivemind -f json version
```

A successful response returns machine-readable metadata (commit SHA, build timestamp, supported adapters).

## Documentation

Canonical user documentation is hosted at <https://hivemind-landing.netlify.app/>. Start with the quickstart at <https://hivemind-landing.netlify.app/docs/overview/quickstart> for an end-to-end walkthrough.

Repository copies of the same content live under `docs/`:

- [Quickstart](docs/overview/quickstart.md)
- [Install Guide](docs/overview/install.md)
- [Vision & Principles](docs/overview/vision.md)

## License

MIT
