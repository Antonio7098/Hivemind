# Hivemind Development Process

> **Principle 5:** Structure and process are how agents scale.
> Structure is not overhead — it is leverage.

This directory contains the mandatory processes for Hivemind development.

## Documents

| Document | Purpose |
|----------|---------|
| [sprint-execution.md](sprint-execution.md) | **Mandatory workflow** for executing each sprint |
| [sprint-report-template.md](sprint-report-template.md) | Template for sprint completion reports |

## Quick Start

### Starting a New Sprint

```bash
# Create sprint branch
make sprint-start N=1 NAME=event-foundation

# Or manually:
git checkout main && git pull
git checkout -b sprint/01-event-foundation
```

### During Development

1. Implement sprint checklist items from `ops/ROADMAP.md`
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
make sprint-pr

# Or manually:
gh pr create --title "Sprint N: Title" --base main
```

### After Merge

1. Generate sprint report from template
2. Save to `ops/reports/sprint-N-report.md`
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

## Sprint Lifecycle

```
PLANNING → DEVELOPMENT → VALIDATION → DOCUMENTATION → REVIEW → MERGE → REPORT
```

See [sprint-execution.md](sprint-execution.md) for complete details.
