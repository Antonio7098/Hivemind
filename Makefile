# Hivemind Makefile
#
# Principle 7: CLI-first is non-negotiable.
# Principle 9: Automated checks are mandatory.

.PHONY: all build test test-nextest lint fmt check validate clean doc help

NEXTEST_AVAILABLE := $(shell cargo nextest --version >/dev/null 2>&1 && echo 1)
ifeq ($(NEXTEST_AVAILABLE),1)
TEST_CMD := cargo nextest run --all-features
else
TEST_CMD := cargo test --all-features
endif

# Default target
all: validate build

# Build the project
build:
	cargo build --all-features

# Build release
release:
	cargo build --release --all-features

# Run all tests
test:
	$(TEST_CMD)

test-nextest:
	cargo nextest run --all-features

# Run integration tests (marked as ignored by default)
test-integration:
	cargo test --all-features -- --ignored

# Run tests with coverage
coverage:
	cargo llvm-cov --all-features --lcov --output-path lcov.info
	cargo llvm-cov report

# Format code
fmt:
	cargo fmt --all

# Check formatting (CI mode)
fmt-check:
	cargo fmt --all --check

# Lint with clippy (strict mode)
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Check (compile without codegen)
check:
	cargo check --all-features

# Generate documentation
doc:
	cargo doc --no-deps --document-private-items

# Full validation suite (run before PR)
validate: fmt-check lint test doc
	@echo "All validation checks passed!"

# Clean build artifacts
clean:
	cargo clean
	rm -f lcov.info

# Run the CLI
run:
	cargo run --

# Install locally
install:
	cargo install --path .

# === Changelog helpers ===

# Add changelog entry interactively
changelog-add:
	python scripts/changelog_add.py --interactive

# === Sprint workflow helpers ===

# Start a new sprint (usage: make sprint-start N=1 NAME=event-foundation)
sprint-start:
	@if [ -z "$(N)" ] || [ -z "$(NAME)" ]; then \
		echo "Usage: make sprint-start N=<number> NAME=<short-name>"; \
		exit 1; \
	fi
	git checkout main
	git pull origin main
	git checkout -b sprint/$(N)-$(NAME)
	@echo "Branch sprint/$(N)-$(NAME) created. Ready to begin Sprint $(N)."

# Pre-PR validation (run before creating PR)
sprint-validate: validate
	@echo ""
	@echo "=== Sprint Validation Complete ==="
	@echo "Run 'make sprint-pr' to create the pull request."

# Create PR for current sprint branch
sprint-pr:
	@BRANCH=$$(git rev-parse --abbrev-ref HEAD); \
	case "$$BRANCH" in sprint/*) ;; *) \
		echo "Error: Not on a sprint branch (current: $$BRANCH)"; \
		exit 1; \
	esac; \
	SPRINT_NUM=$$(echo $$BRANCH | sed 's/sprint\/\([0-9]*\)-.*/\1/'); \
	SPRINT_NAME=$$(echo $$BRANCH | sed 's/sprint\/[0-9]*-//'); \
	echo "Creating PR for Sprint $$SPRINT_NUM: $$SPRINT_NAME"; \
	echo "Remember: Sprint report (ops/reports/sprint-$$SPRINT_NUM-report.md) must be committed before merge!"; \
	gh pr create --title "Sprint $$SPRINT_NUM: $$SPRINT_NAME" --base main --body "Sprint report: ops/reports/sprint-$$SPRINT_NUM-report.md"

# === Help ===

help:
	@echo "Hivemind Makefile"
	@echo ""
	@echo "Build targets:"
	@echo "  build           Build debug binary"
	@echo "  release         Build release binary"
	@echo "  install         Install to ~/.cargo/bin"
	@echo ""
	@echo "Test targets:"
	@echo "  test            Run unit tests"
	@echo "  test-integration Run integration tests"
	@echo "  coverage        Run tests with coverage"
	@echo ""
	@echo "Quality targets:"
	@echo "  fmt             Format code"
	@echo "  fmt-check       Check formatting"
	@echo "  lint            Run clippy (strict)"
	@echo "  validate        Full validation suite"
	@echo ""
	@echo "Documentation:"
	@echo "  doc             Generate documentation"
	@echo ""
	@echo "Sprint workflow:"
	@echo "  sprint-start     Start new sprint (N=1 NAME=event-foundation)"
	@echo "  sprint-validate  Run pre-PR validation"
	@echo "  sprint-pr        Create PR for current sprint"
	@echo ""
	@echo "Changelog:"
	@echo "  changelog-add   Add entry interactively"
	@echo ""
	@echo "Other:"
	@echo "  clean           Clean build artifacts"
	@echo "  help            Show this help"
