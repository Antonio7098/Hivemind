# Sprint Execution Protocol

> **Principle 11:** Build incrementally, prove foundations first.
> **Principle 15:** No magic. Everything has a reason. Everything has a trail.

This document defines the **mandatory workflow** for executing each sprint of Hivemind development. Every sprint follows this protocol. No exceptions.

The protocol ensures that:
- Each sprint is isolated and traceable (one branch per sprint)
- Quality gates are enforced before progression
- Documentation stays synchronized with code
- History is auditable and reproducible

---

## Sprint Lifecycle

```
PLANNING → DEVELOPMENT → VALIDATION → DOCUMENTATION → REPORT → REVIEW → MERGE → CI MONITORING
```

Each stage has defined inputs, outputs, and exit criteria.

> **CRITICAL:** Sprint report is generated **BEFORE merge**, not after. Merge happens only when report is complete.

---

## 1. Planning Stage

### 1.1 Branch Creation

Create a dedicated branch for the sprint:

```bash
git checkout main
git pull origin main
git checkout -b sprint/<sprint-number>-<short-name>
```

Examples:
- `sprint/01-event-foundation`
- `sprint/14-opencode-adapter`
- `sprint/23-single-repo-e2e`

### 1.2 Scope Review

Before writing code:

> **CRITICAL:** Thoroughly review **all** relevant documentation in `docs/architecture/` and `docs/design/`. Do not miss anything — missing docs lead to incomplete implementations.

1. Read the sprint definition in `ops/ROADMAP.md`
2. Identify all checklist items for the sprint
3. **Review related architecture docs** in `docs/architecture/` — check for patterns, interfaces, and constraints
4. **Review related design docs** in `docs/design/` — understand operational semantics, error models, and decisions
5. **Cross-reference docs** — ensure architecture and design docs are consistent
6. Identify dependencies on prior sprints
7. Verify prior sprint exit criteria are met

### 1.3 Codex Study Stage

Codex patterns are mission-critical for Phase 4. Before development:

1. **Audit `@codex-main/codex-rs`** (i.e., `~/programming/Hivemind/codex-main/codex-rs`):
   - Identify runtime harnessing, tool schemas, streaming protocols, and permission patterns that align with our native runtime ambitions.
   - Note how tests exercise agent loops/tool validations so we know what we must cover.
2. **Review `codex-main/.npmrc`** for workspace/dependency policies and record actionable settings.
3. **Run or inspect critical tests** in `@codex-main/codex-rs` (focus on harness/tool/permission tests) and summarize the test surface we should match or exceed in Phase 4.
4. Document findings, reused code snippets, and testing expectations before moving to development.

### 1.4 Planning Outputs

- [Branch created and pushed  
- [Sprint scope understood  
- [Dependencies verified  
- [Codex audit documented, including harness/testing expectations  

### 1.3 Planning Outputs

- [Branch created and pushed  
- [Sprint scope understood  
- [Dependencies verified  
- [codex-main `.npmrc` reviewed and actionable patterns captured  
- [Concrete codex-main code patterns identified/copied where relevant  

---

## 2. Development Stage

### 2.1 Development Rules

1. **One sprint per branch** — no mixing sprint work
2. **Commit frequently** — atomic, meaningful commits
3. **Commit message format:**
   ```
   <type>(<scope>): <description>

   [optional body]

   Sprint: <sprint-number>
   ```
   Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

4. **No `// TODO` without issue** — create tracking issue or fix now
5. **Tests alongside code** — no code without tests

### 2.2 Checklist Tracking

As you complete items, update `ops/ROADMAP.md`:
- Change `- [ ]` to `- [x]` for completed items
- Commit the roadmap update with the feature

### 2.3 Development Outputs

- [All sprint checklist items implemented  
- [All new code has tests  
- [Commits are atomic and well-messaged  

---

## 3. Validation Stage

### 3.1 Mandatory Checks

Run **all** validation commands. Fail fast if any exits non-zero.

```bash
# Format check
cargo fmt --all --check

# Lint (strict mode - warnings are errors)
cargo clippy --all-targets --all-features -- -D warnings

# Unit tests
cargo test --all-features

# Integration tests (if applicable)
cargo test --all-features -- --ignored

# Coverage (ensure new code is covered)
cargo llvm-cov --all-features --lcov --output-path lcov.info
# Or: cargo tarpaulin --out Lcov

# Doc tests
cargo test --doc
```

Or use the Makefile:
```bash
make validate
```

### 3.2 Exit Criteria Verification

For each exit criterion in `ops/ROADMAP.md`:

1. Write a verification test or manual verification script
2. Document how to verify in the sprint report
3. All exit criteria must pass

### 3.3 Principle Checkpoint

Verify against all 15 principles:

Review each principle and document compliance in sprint report:
1. Observability is truth
2. Fail fast, fail loud
3. Reliability over cleverness
4. Explicit error taxonomy
5. Structure scales
6. SOLID principles
7. CLI-first
8. Absolute observability
9. Automated checks mandatory
10. Failures are first-class
11. Build incrementally
12. Maximum modularity
13. Abstraction without loss
14. Human authority
15. No magic

### 3.4 Validation Outputs

- cargo fmt --check` passes
- cargo clippy -D warnings` passes
- All tests pass
- Coverage meets threshold (target: 80%+)
- All exit criteria verified
- Principle checkpoint completed

---

## 4. Documentation Stage

### 4.1 Code Documentation

1. All public items have doc comments
2. Module-level documentation explains purpose
3. Examples in doc comments where helpful

Verify:
```bash
cargo doc --no-deps --document-private-items
# Check for warnings
```

### 4.2 Architecture/Design Docs

If the sprint changes behavior documented in `docs/`:

1. Update affected architecture docs
2. Update affected design docs
3. Ensure docs match implementation

### 4.3 Changelog Update

Add entry to `changelog.json`:

```bash
# Use the changelog helper
make changelog-add

# Or manually edit changelog.json
```

### 4.4 README Updates

If the sprint adds user-facing features:
- Update relevant sections in README.md
- Add/update usage examples

### 4.5 Documentation Outputs

- All public items documented
- cargo doc` produces no warnings
- Architecture/design docs synchronized
- Changelog entry added
- README updated (if applicable)

---

### Manual end-to-end test project run (required)

Use the bundled `hivemind-test/` project to exercise real execution surfaces, runtime output capture, filesystem observation, verification transitions, and the merge workflow.

This procedure must run against a **fresh clone in a temp directory** so the main repo stays clean and no manual cleanup is required.

1) Create a fresh temp clone

```bash
TMP_ROOT=$(mktemp -d)
trap 'rm -rf "$TMP_ROOT"' EXIT

git clone . "$TMP_ROOT/hivemind"
cd "$TMP_ROOT/hivemind"
```

2) Build the binary you will test

```bash
cargo build --release
```

3) Run the smoke tests and capture stdout/stderr

```bash
set -euo pipefail

./hivemind-test/test_worktree.sh 2>&1 | tee /tmp/hivemind-test_worktree.log
./hivemind-test/test_execution.sh 2>&1 | tee /tmp/hivemind-test_execution.log
```

4) Inspect the event log for correctness

The test scripts set `HOME` to a sandbox temp directory. Inspect its event store:

```bash
test -f /tmp/hivemind-test/.hm_home/.hivemind/events.jsonl

# High-signal checks (note: `rg` exits non-zero when there are no matches)
rg '"type":"runtime_' /tmp/hivemind-test/.hm_home/.hivemind/events.jsonl | head || true
rg '"type":"task_execution_state_changed"' /tmp/hivemind-test/.hm_home/.hivemind/events.jsonl | head || true
rg '"type":"merge_' /tmp/hivemind-test/.hm_home/.hivemind/events.jsonl | head || true
```

5) Inspect worktrees and filesystem changes

```bash
# Confirm the runtime observed filesystem changes
rg '"type":"runtime_filesystem_observed"' /tmp/hivemind-test/.hm_home/.hivemind/events.jsonl | head || true

# Optionally, inspect the diff artifact for the attempt
# (find the attempt id from events.jsonl, then run: hivemind attempt inspect <attempt-id> --diff --output)
```

6) If anything looks wrong, stop and debug before merging

Minimum debugging artifacts to include in the PR discussion:

- `/tmp/hivemind-test_worktree.log`
- `/tmp/hivemind-test_execution.log`
- the relevant excerpt from `hivemind-test/.hm_home/.hivemind/events.jsonl`

---

## 5. Report Stage (BEFORE Merge)

### 5.1 Generate Sprint Report

Create report at `ops/reports/sprint-<N>-report.md` using the template in `ops/process/sprint-report-template.md`.

The report must document:

1. **Sprint Metadata** — Number, title, dates, duration
2. **Objectives** — What the sprint aimed to achieve
3. **Deliverables** — What was delivered
4. **Exit Criteria Results** — Pass/fail for each criterion
5. **Metrics** — Lines of code, test count, coverage
6. **Principle Compliance** — How each principle was upheld
7. **Challenges** — What was difficult
8. **Learnings** — What was learned
9. **Next Sprint Readiness** — Dependencies satisfied for next sprint

### 5.2 Commit Report

```bash
git add ops/reports/sprint-<N>-report.md
git commit -m "docs: Sprint <N> completion report"
git push origin sprint/<N>-<name>
```

### 5.3 Report Outputs

- Sprint report generated from template
- All exit criteria documented with results
- Metrics captured (coverage, lines changed, etc.)
- Principle compliance documented
- Report committed and pushed

---

## 6. Review Stage

### 6.1 Self-Review Checklist

Before requesting review:

- Re-read all changed files
- Diff against main: `git diff main...HEAD`
- Check for debug code, commented code, TODOs
- Verify no secrets or credentials
- Run full validation suite one more time

### 6.2 Version Management

Before creating a PR:

1. **Increment Version Number**
   - Update version in `Cargo.toml` (semantic versioning: MAJOR.MINOR.PATCH)
   - Update version in any other version sources
   - Commit version change with message: `chore(release): bump version <old> -> <new>`

2. **Version Consistency Check**
   - Verify all version references are synchronized:
     ```bash
     # Check version in Cargo.toml
     grep "^version" Cargo.toml

     # Verify consistency across project
     # (no VERSION constants or hardcoded versions elsewhere)
     grep -r "VERSION\|version" src/ --include="*.rs" | grep -v "// version"
     ```
   - Run full validation suite to ensure version change breaks nothing
   - Verify changelog entry matches new version number
   - If any version inconsistencies found, resolve before proceeding

### 6.3 Generate PR Description

The PR description must include:

1. **Sprint Summary** — What this sprint accomplishes
2. **Changes Made** — Bullet list of major changes
3. **Exit Criteria Status** — All criteria with pass/fail
4. **Test Results** — Summary of test output
5. **Coverage Report** — Coverage percentage
6. **Principle Compliance** — Any notable decisions
7. **Breaking Changes** — If any
8. **Sprint Report** — Link to `ops/reports/sprint-<N>-report.md`

### 6.4 Create Pull Request

```bash
# Ensure all changes committed
git status -sb

# Push branch
git push -u origin sprint/<N>-<name>

# Create PR
make sprint-pr

# Or manually:
gh pr create \
  --title "Sprint <N>: <Title>" \
  --base main \
  --head sprint/<N>-<name>
```

### 6.5 Review Outputs

- Self-review completed
- Version number incremented
- Version consistency verified
- PR description complete
- PR created

---

## 7. Merge Stage

### 7.1 Pre-Merge Verification

Before merging:

1. All CI checks pass ✅
2. Review comments addressed
3. **Sprint report is complete and committed**
4. Final validation run passes

### 7.2 Merge Protocol

```bash
# Squash merge to keep history clean
gh pr merge <PR-number> --squash --delete-branch
```

### 7.3 Merge Outputs

- PR merged to main
- Sprint branch deleted
- Sprint report present in main

---

## 8. CI Monitoring Stage (AFTER Merge)

### 8.1 Monitor CI Pipeline

After merge completes:

```bash
# Watch CI in real-time
gh run list --workflow=.github/workflows/ci.yml -L 1

# Or check GitHub Actions web UI
# https://github.com/anthropics/hivemind/actions
```

### 8.2 CI Success Path

If CI passes:

```bash
# Verify locally as well
git checkout main
git pull origin main

cargo build --release
cargo test --all-features
```

All green? Sprint is **complete and ready for next sprint**.

### 8.3 CI Failure Path

If CI fails after merge:

1. **Diagnose the failure** — Check CI logs
2. **Fix immediately** — Create fix commit on main
3. **Re-run CI** — Wait for pass
4. **Update sprint report** — Document the issue and fix
5. **Commit report update** — `git add ops/reports/sprint-<N>-report.md && git commit -m "docs: Sprint <N> report update (post-merge fix)"`

### 8.4 Tagging (Optional)

For milestone sprints, create a tag (after CI passes):

```bash
git tag -a sprint-<N>-complete -m "Sprint <N>: <Title> complete"
git push origin sprint-<N>-complete
```

### 8.5 Update Master Tracking

Update `ops/ROADMAP.md` on main:
- All checklist items for sprint marked `[x]`
- Add completion date as comment

Commit:
```bash
git add ops/ROADMAP.md
git commit -m "docs: Sprint <N> marked complete"
```

### 8.6 CI Monitoring Outputs

- CI pipeline passes
- Local build/test verification complete
- Tag created (if milestone)
- ops/ROADMAP.md updated on main
- Next sprint is ready to begin

---

## Quick Reference: Validation Commands

```bash
# Full validation suite
make validate

# Or individually:
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo test --all-features -- --ignored  # integration tests
cargo doc --no-deps
cargo llvm-cov --all-features
```

---

## Quick Reference: Git Workflow

```bash
# Start sprint
git checkout main && git pull
git checkout -b sprint/<N>-<name>

# During development
git add <files>
git commit -m "<type>(<scope>): <desc>\n\nSprint: <N>"

# Prepare for PR
make validate
git push -u origin sprint/<N>-<name>

# Create PR
make sprint-pr

# After review (on your branch)
# Address comments, push fixes

# After merge
git checkout main && git pull
git tag -a sprint-<N>-complete -m "Sprint <N> complete"  # optional
```

---

## Failure Handling

### Validation Failure (Pre-PR)

If validation fails before PR:
1. Fix the issue locally
2. Re-run full validation
3. Commit the fix with descriptive message
4. Do not push until all checks pass

### Exit Criteria Failure

If an exit criterion cannot be met:
1. Document why in the sprint report
2. Create issue for follow-up
3. Decide: block sprint or defer criterion
4. If blocking, fix before merge
5. If deferring, document explicitly in report

### Merge Conflict

If main has diverged:
```bash
git fetch origin main
git rebase origin/main
# Resolve conflicts
make validate  # Verify after rebase
git push --force-with-lease
```

### CI Failure (Post-Merge)

If CI fails after merge:
1. **Act immediately**
2. Fix on main or in new PR
3. Document in sprint report
4. Re-run CI until passing

---

## Invariants

This protocol enforces:

- **One branch per sprint** — isolation and traceability
- **All checks must pass** — no exceptions
- **Documentation is mandatory** — not optional
- **Sprint report before merge** — non-negotiable
- **CI monitoring after merge** — required
- **Reports are generated** — history is preserved
- **Changelog is updated** — changes are tracked

Violating these invariants requires explicit justification documented in the sprint report.

---

## Final Pre-Merge Checklist

Before considering this sprint complete, verify **nothing has been missed**:

### Documentation Review
- **Re-read** `docs/architecture/` — any patterns/interfaces you missed?
- **Re-read** `docs/design/` — any operational semantics you overlooked?
- **Cross-check** architecture against design docs — are they consistent?
- **Review** sprint-specific docs in architecture — are all relevant specs covered?

### Code Completeness
- All checklist items from `ops/ROADMAP.md` implemented
- No stray `TODO` comments without tracking issues
- No debug code or commented-out code
- All error cases handled explicitly

### Validation
- `make validate` passes completely
- All exit criteria verified with evidence
- Coverage meets threshold (80%+)
- Doc tests pass (`cargo test --doc`)

### Documentation
- Sprint report generated and committed
- `changelog.json` updated
- Public API items have doc comments
- README updated (if user-facing changes)

### Testing
- Unit tests cover all new code paths
- Integration tests pass for affected modules
- Edge cases tested explicitly
- **Manual CLI testing completed** — all commands exercised end-to-end
- **Manual end-to-end test project run completed** — run the bundled test project from a clean start and inspect logs/events
- Error scenarios tested via CLI (invalid inputs, missing args, etc.)
- CLI output verified matches expected format


### Final Verification
- **One final read-through** of all changed files
- **One final run** of full validation suite
- Sprint report accurately reflects what was done

> **WARNING:** If you think "I probably don't need to check this" — **check it anyway**. Undocumented assumptions cause failures.

---

**This protocol is mandatory. Follow it.**
