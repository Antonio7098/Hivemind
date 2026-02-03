# Hivemind Makefile
#
# Principle 7: CLI-first is non-negotiable.
# Principle 9: Automated checks are mandatory.

.PHONY: all build test lint fmt check validate clean doc help

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
	cargo test --all-features

# Run integration tests (marked as ignored by default)
test-integration:
	cargo test --all-features -- --ignored

# Run tests with coverage
coverage:
	cargo llvm-cov --all-features --lcov --output-path lcov.info
	cargo llvm-cov report --all-features

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

# === Phase workflow helpers ===

# Start a new phase (usage: make phase-start N=1 NAME=event-foundation)
phase-start:
	@if [ -z "$(N)" ] || [ -z "$(NAME)" ]; then \
		echo "Usage: make phase-start N=<number> NAME=<short-name>"; \
		exit 1; \
	fi
	git checkout main
	git pull origin main
	git checkout -b phase/$(N)-$(NAME)
	@echo "Branch phase/$(N)-$(NAME) created. Ready to begin Phase $(N)."

# Pre-PR validation (run before creating PR)
phase-validate: validate
	@echo ""
	@echo "=== Phase Validation Complete ==="
	@echo "Run 'make phase-pr' to create the pull request."

# Create PR for current phase branch
phase-pr:
	@BRANCH=$$(git rev-parse --abbrev-ref HEAD); \
	if [[ ! "$$BRANCH" =~ ^phase/ ]]; then \
		echo "Error: Not on a phase branch (current: $$BRANCH)"; \
		exit 1; \
	fi; \
	PHASE_NUM=$$(echo $$BRANCH | sed 's/phase\/\([0-9]*\)-.*/\1/'); \
	PHASE_NAME=$$(echo $$BRANCH | sed 's/phase\/[0-9]*-//'); \
	echo "Creating PR for Phase $$PHASE_NUM: $$PHASE_NAME"; \
	echo "Remember: Phase report (ops/reports/phase-$$PHASE_NUM-report.md) must be committed before merge!"; \
	gh pr create --title "Phase $$PHASE_NUM: $$PHASE_NAME" --base main

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
	@echo "Phase workflow:"
	@echo "  phase-start     Start new phase (N=1 NAME=event-foundation)"
	@echo "  phase-validate  Run pre-PR validation"
	@echo "  phase-pr        Create PR for current phase"
	@echo ""
	@echo "Changelog:"
	@echo "  changelog-add   Add entry interactively"
	@echo ""
	@echo "Other:"
	@echo "  clean           Clean build artifacts"
	@echo "  help            Show this help"
