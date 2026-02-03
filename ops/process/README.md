# Hivemind Development Process

> **Principle 5:** Structure and process are how agents scale.
> Structure is not overhead — it is leverage.

This directory contains the mandatory processes for Hivemind development.

## Documents

| Document | Purpose |
|----------|---------|
| [phase-execution.md](phase-execution.md) | **Mandatory workflow** for executing each phase |
| [phase-report-template.md](phase-report-template.md) | Template for phase completion reports |

## Quick Start

### Starting a New Phase

```bash
# Create phase branch
make phase-start N=1 NAME=event-foundation

# Or manually:
git checkout main && git pull
git checkout -b phase/01-event-foundation
```

### During Development

1. Implement phase checklist items from `ops/ROADMAP.md`
2. Write tests alongside code
3. Commit frequently with meaningful messages
4. Update `ops/ROADMAP.md` as items complete

### Before PR

```bash
# Full validation
make validate

# Or individually:
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps
```

### Creating PR

```bash
make phase-pr

# Or manually:
gh pr create --title "Phase N: Title" --base main
```

### After Merge

1. Generate phase report from template
2. Save to `ops/reports/phase-N-report.md`
3. Update changelog: `make changelog-add`

## Validation Commands

| Command | Purpose |
|---------|---------|
| `make fmt` | Format code |
| `make fmt-check` | Check formatting (CI) |
| `make lint` | Clippy with -D warnings |
| `make test` | Run unit tests |
| `make test-integration` | Run integration tests |
| `make coverage` | Test with coverage |
| `make doc` | Generate documentation |
| `make validate` | Full validation suite |

## Phase Lifecycle

```
PLANNING → DEVELOPMENT → VALIDATION → DOCUMENTATION → REVIEW → MERGE → REPORT
```

See [phase-execution.md](phase-execution.md) for complete details.
