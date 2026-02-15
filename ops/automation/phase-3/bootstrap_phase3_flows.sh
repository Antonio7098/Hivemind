#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

HM_BIN="${HM_BIN:-$REPO_ROOT/target/release/hivemind}"
HIVEMIND_DATA_DIR="${HIVEMIND_DATA_DIR:-$HOME/.hivemind}"
PROJECT_NAME="${PROJECT_NAME:-hivemind-phase3-governance-context}"
ATTACH_REPO_PATH="${ATTACH_REPO_PATH:-$REPO_ROOT}"

CODEX_BIN="${CODEX_BIN:-$(command -v codex || true)}"
OPENCODE_BIN="${OPENCODE_BIN:-$(command -v opencode || true)}"
WORKER_MODEL="${WORKER_MODEL:-gpt-5.3-codex-high}"
VALIDATOR_MODEL="${VALIDATOR_MODEL:-opencode/minimax-m2.5}"
WORKER_TIMEOUT_MS="${WORKER_TIMEOUT_MS:-1800000}"
VALIDATOR_TIMEOUT_MS="${VALIDATOR_TIMEOUT_MS:-1800000}"
WORKER_MAX_PARALLEL="${WORKER_MAX_PARALLEL:-4}"
VALIDATOR_MAX_PARALLEL="${VALIDATOR_MAX_PARALLEL:-1}"

HIVEMIND_TEST_ROOT="${HIVEMIND_TEST_ROOT:-/home/antonio/programming/Hivemind/hivemind-test}"
MANUAL_REPORT_ROOT="${MANUAL_REPORT_ROOT:-$HIVEMIND_TEST_ROOT/phase-3}"

mkdir -p "$HIVEMIND_DATA_DIR" "$MANUAL_REPORT_ROOT"
export HIVEMIND_DATA_DIR

if [[ ! -x "$HM_BIN" ]]; then
  echo "hivemind binary not found/executable: $HM_BIN" >&2
  exit 1
fi

if [[ -z "$CODEX_BIN" || ! -x "$CODEX_BIN" ]]; then
  echo "codex binary not found; set CODEX_BIN" >&2
  exit 1
fi

if [[ -z "$OPENCODE_BIN" || ! -x "$OPENCODE_BIN" ]]; then
  echo "opencode binary not found; set OPENCODE_BIN" >&2
  exit 1
fi

if ! git -C "$ATTACH_REPO_PATH" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "ATTACH_REPO_PATH is not a git repository: $ATTACH_REPO_PATH" >&2
  exit 1
fi

hm() {
  "$HM_BIN" -f json "$@"
}

extract_id() {
  python3 -c '
import json, sys
obj = json.load(sys.stdin)
if isinstance(obj, dict) and "data" in obj and isinstance(obj["data"], dict):
    data = obj["data"]
else:
    data = obj
for key in ("id", "task_id", "graph_id", "flow_id", "project_id"):
    if isinstance(data, dict) and key in data:
        print(data[key])
        raise SystemExit(0)
raise SystemExit("no id field found")
'
}

project_id_from_name() {
  local name="$1"
  hm project list | python3 -c '
import json, sys
name = sys.argv[1]
items = json.load(sys.stdin).get("data", [])
for item in items:
    if item.get("name") == name:
        print(item.get("id", ""))
        break
' "$name"
}

ensure_project() {
  local existing
  existing="$(project_id_from_name "$PROJECT_NAME" || true)"
  if [[ -n "$existing" ]]; then
    echo "$existing"
    return 0
  fi
  hm project create "$PROJECT_NAME" --description "Phase 3 (S34-S41) governance/context execution plan" | extract_id
}

project_has_repo_path() {
  local project="$1"
  local repo_path="$2"
  hm project inspect "$project" | python3 -c '
import json, sys
target = sys.argv[1]
repos = json.load(sys.stdin).get("data", {}).get("repositories", [])
for repo in repos:
    if repo.get("path") == target:
        print("yes")
        break
' "$repo_path"
}

create_task() {
  local project="$1"
  local title="$2"
  local description="$3"
  hm task create "$project" "$title" --description "$description" | extract_id
}

create_graph() {
  local project="$1"
  local name="$2"
  shift 2
  hm graph create "$project" "$name" --from-tasks "$@" | extract_id
}

create_flow() {
  local graph_id="$1"
  local name="$2"
  hm flow create "$graph_id" --name "$name" | extract_id
}

add_dep() {
  local graph_id="$1"
  local prerequisite_task="$2"
  local dependent_task="$3"
  hm graph add-dependency "$graph_id" "$prerequisite_task" "$dependent_task" >/dev/null
}

add_required_check() {
  local graph_id="$1"
  local task_id="$2"
  local name="$3"
  local command="$4"
  hm graph add-check "$graph_id" "$task_id" --name "$name" --command "$command" --required >/dev/null
}

set_task_auto() {
  local task_id="$1"
  hm task set-run-mode "$task_id" auto >/dev/null
}

set_flow_runtime_defaults() {
  local flow_id="$1"
  hm flow runtime-set "$flow_id" \
    --role worker \
    --adapter codex \
    --binary-path "$CODEX_BIN" \
    --model "$WORKER_MODEL" \
    --timeout-ms "$WORKER_TIMEOUT_MS" \
    --max-parallel-tasks "$WORKER_MAX_PARALLEL" >/dev/null

  hm flow runtime-set "$flow_id" \
    --role validator \
    --adapter opencode \
    --binary-path "$OPENCODE_BIN" \
    --model "$VALIDATOR_MODEL" \
    --timeout-ms "$VALIDATOR_TIMEOUT_MS" \
    --max-parallel-tasks "$VALIDATOR_MAX_PARALLEL" >/dev/null
}

set_verification_task_runtime() {
  local task_id="$1"
  hm task runtime-set "$task_id" \
    --role worker \
    --adapter opencode \
    --binary-path "$OPENCODE_BIN" \
    --model "$VALIDATOR_MODEL" \
    --timeout-ms "$VALIDATOR_TIMEOUT_MS" >/dev/null

  hm task runtime-set "$task_id" \
    --role validator \
    --adapter opencode \
    --binary-path "$OPENCODE_BIN" \
    --model "$VALIDATOR_MODEL" \
    --timeout-ms "$VALIDATOR_TIMEOUT_MS" >/dev/null
}

add_verification_gate_checks() {
  local sprint="$1"
  local graph_id="$2"
  local gate_task_id="$3"
  local report_path="$MANUAL_REPORT_ROOT/sprint-${sprint}-manual-report.md"

  add_required_check "$graph_id" "$gate_task_id" "fmt-check" "cargo fmt --all --check"
  add_required_check "$graph_id" "$gate_task_id" "clippy-strict" "cargo clippy --all-targets --all-features -- -D warnings"
  add_required_check "$graph_id" "$gate_task_id" "test-all-features" "cargo test --all-features"
  add_required_check "$graph_id" "$gate_task_id" "test-doc" "cargo test --doc"
  add_required_check "$graph_id" "$gate_task_id" "manual-report-exists" "test -f \"$report_path\""
  add_required_check "$graph_id" "$gate_task_id" "manual-report-covers-exit-criteria" "rg -qi \"exit criteria|completion criteria\" \"$report_path\""
  add_required_check "$graph_id" "$gate_task_id" "manual-report-covers-architecture" "rg -qi \"architecture|principle\" \"$report_path\""
  add_required_check "$graph_id" "$gate_task_id" "manual-report-covers-testing" "rg -qi \"cargo fmt|clippy|cargo test|manual testing\" \"$report_path\""
}

COMMON_REQ="Follow hivemind/ops/process/sprint-execution.md as the phase-execution protocol. Uphold hivemind/docs/overview/principles.md. Complete active checkpoints from runtime output via hivemind checkpoint complete before task complete. Keep work scoped to this sprint and update roadmap/docs/tests as needed."
MERGE_REQ="Merge strategy requirement for this sprint flow: use hivemind merge prepare <flow-id> --target main, hivemind merge approve <flow-id>, then hivemind merge execute <flow-id> --mode pr --monitor-ci --auto-merge --pull-after (GitHub PR with squash merge)."

RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
MANIFEST_DIR="$HIVEMIND_DATA_DIR/phase3-artifacts"
mkdir -p "$MANIFEST_DIR"
MANIFEST_PATH="$MANIFEST_DIR/phase3-flow-manifest-$RUN_STAMP.tsv"
printf "type\tsprint\tkey\tid\tnotes\n" > "$MANIFEST_PATH"

PROJECT_ID="$(ensure_project)"
printf "project\t-\t%s\t%s\t%s\n" "$PROJECT_NAME" "$PROJECT_ID" "$ATTACH_REPO_PATH" >> "$MANIFEST_PATH"

if [[ "$(project_has_repo_path "$PROJECT_ID" "$ATTACH_REPO_PATH" || true)" != "yes" ]]; then
  hm project attach-repo "$PROJECT_ID" "$ATTACH_REPO_PATH" --name hivemind --access rw >/dev/null
fi

hm project runtime-set "$PROJECT_ID" \
  --role worker \
  --adapter codex \
  --binary-path "$CODEX_BIN" \
  --model "$WORKER_MODEL" \
  --timeout-ms "$WORKER_TIMEOUT_MS" \
  --max-parallel-tasks "$WORKER_MAX_PARALLEL" >/dev/null

hm project runtime-set "$PROJECT_ID" \
  --role validator \
  --adapter opencode \
  --binary-path "$OPENCODE_BIN" \
  --model "$VALIDATOR_MODEL" \
  --timeout-ms "$VALIDATOR_TIMEOUT_MS" \
  --max-parallel-tasks "$VALIDATOR_MAX_PARALLEL" >/dev/null

# Launch gate (safe orchestration boundary; all sprint flows depend on this flow).
LAUNCH_TASK="$(create_task "$PROJECT_ID" "Phase 3 Launch Gate" "Operational gate for Phase 3 automation. Start this flow when ready to begin Sprint 34-41 chain. $COMMON_REQ")"
set_task_auto "$LAUNCH_TASK"
LAUNCH_GRAPH="$(create_graph "$PROJECT_ID" "phase3-launch-gate" "$LAUNCH_TASK")"
add_required_check "$LAUNCH_GRAPH" "$LAUNCH_TASK" "launch-gate-ack" "echo phase3-launch-approved"
LAUNCH_FLOW="$(create_flow "$LAUNCH_GRAPH" "phase3-launch-gate")"
set_flow_runtime_defaults "$LAUNCH_FLOW"
hm flow set-run-mode "$LAUNCH_FLOW" manual >/dev/null
printf "flow\t-\tlaunch-gate\t%s\tmanual trigger flow for safe start\n" "$LAUNCH_FLOW" >> "$MANIFEST_PATH"
printf "task\t-\tlaunch-gate\t%s\tlaunch gate task\n" "$LAUNCH_TASK" >> "$MANIFEST_PATH"

# Sprint 34
S34_T1="$(create_task "$PROJECT_ID" "S34.1 Storage Topology and Boundaries" "Implement roadmap section 34.1 (governance storage topology/boundaries and explicit export-import boundary). $COMMON_REQ")"
S34_T2="$(create_task "$PROJECT_ID" "S34.2 Event and Projection Contracts" "Implement roadmap section 34.2 (governance event families, deterministic projections, schema/version markers). $COMMON_REQ")"
S34_T3="$(create_task "$PROJECT_ID" "S34.3 Backward-Compatible Migration" "Implement roadmap section 34.3 (migration path, replay determinism preservation, explicit migration events and rollback hints). $COMMON_REQ")"
S34_T4="$(create_task "$PROJECT_ID" "S34.4 Manual Testing in @hivemind-test" "Execute roadmap section 34.4 manual validation in $HIVEMIND_TEST_ROOT and publish report at $MANUAL_REPORT_ROOT/sprint-34-manual-report.md. $COMMON_REQ")"
S34_T5="$(create_task "$PROJECT_ID" "S34.5 Validation Gate and Merge Readiness" "Validate Sprint 34 exit criteria, architecture soundness, comprehensive testing, and merge readiness. $MERGE_REQ $COMMON_REQ")"
for t in "$S34_T1" "$S34_T2" "$S34_T3" "$S34_T4" "$S34_T5"; do set_task_auto "$t"; done
S34_GRAPH="$(create_graph "$PROJECT_ID" "phase3-sprint-34" "$S34_T1" "$S34_T2" "$S34_T3" "$S34_T4" "$S34_T5")"
add_dep "$S34_GRAPH" "$S34_T1" "$S34_T3"
add_dep "$S34_GRAPH" "$S34_T2" "$S34_T3"
add_dep "$S34_GRAPH" "$S34_T3" "$S34_T4"
add_dep "$S34_GRAPH" "$S34_T4" "$S34_T5"
add_verification_gate_checks "34" "$S34_GRAPH" "$S34_T5"
S34_FLOW="$(create_flow "$S34_GRAPH" "phase3-sprint-34-governance-storage-foundation")"
set_flow_runtime_defaults "$S34_FLOW"
set_verification_task_runtime "$S34_T5"
printf "flow\t34\tsprint-34\t%s\tGovernance Storage Foundation\n" "$S34_FLOW" >> "$MANIFEST_PATH"

# Sprint 35
S35_T1="$(create_task "$PROJECT_ID" "S35.1 Project Documents" "Implement roadmap section 35.1 project document lifecycle, metadata, revisions, and attach controls. $COMMON_REQ")"
S35_T2="$(create_task "$PROJECT_ID" "S35.2 Global Skills and System Prompts" "Implement roadmap section 35.2 global skill/system prompt registries with schema validation and structured errors. $COMMON_REQ")"
S35_T3="$(create_task "$PROJECT_ID" "S35.3 Templates and Instantiation Inputs" "Implement roadmap section 35.3 template lifecycle, strict reference validation, and TemplateInstantiated events. $COMMON_REQ")"
S35_T4="$(create_task "$PROJECT_ID" "S35.4 Notepads (Non-Injectable)" "Implement roadmap section 35.4 global/project notepad CRUD with explicit non-injection contract. $COMMON_REQ")"
S35_T5="$(create_task "$PROJECT_ID" "S35.5 Manual Testing in @hivemind-test" "Execute roadmap section 35.5 manual validation in $HIVEMIND_TEST_ROOT and publish report at $MANUAL_REPORT_ROOT/sprint-35-manual-report.md. $COMMON_REQ")"
S35_T6="$(create_task "$PROJECT_ID" "S35.6 Validation Gate and Merge Readiness" "Validate Sprint 35 exit criteria, architecture soundness, comprehensive testing, and merge readiness. $MERGE_REQ $COMMON_REQ")"
for t in "$S35_T1" "$S35_T2" "$S35_T3" "$S35_T4" "$S35_T5" "$S35_T6"; do set_task_auto "$t"; done
S35_GRAPH="$(create_graph "$PROJECT_ID" "phase3-sprint-35" "$S35_T1" "$S35_T2" "$S35_T3" "$S35_T4" "$S35_T5" "$S35_T6")"
add_dep "$S35_GRAPH" "$S35_T1" "$S35_T3"
add_dep "$S35_GRAPH" "$S35_T2" "$S35_T3"
add_dep "$S35_GRAPH" "$S35_T3" "$S35_T4"
add_dep "$S35_GRAPH" "$S35_T4" "$S35_T5"
add_dep "$S35_GRAPH" "$S35_T5" "$S35_T6"
add_verification_gate_checks "35" "$S35_GRAPH" "$S35_T6"
S35_FLOW="$(create_flow "$S35_GRAPH" "phase3-sprint-35-documents-global-context")"
set_flow_runtime_defaults "$S35_FLOW"
set_verification_task_runtime "$S35_T6"
printf "flow\t35\tsprint-35\t%s\tDocuments and global context artifacts\n" "$S35_FLOW" >> "$MANIFEST_PATH"

# Sprint 36
S36_T1="$(create_task "$PROJECT_ID" "S36.1 Constitution Schema v1" "Implement roadmap section 36.1 constitution schema, strict validation, and documented failure modes. $COMMON_REQ")"
S36_T2="$(create_task "$PROJECT_ID" "S36.2 Constitution Lifecycle Commands" "Implement roadmap section 36.2 constitution init/show/validate/update with one constitution per project and explicit mutation confirmation. $COMMON_REQ")"
S36_T3="$(create_task "$PROJECT_ID" "S36.3 Constitution Event Model" "Implement roadmap section 36.3 constitution lifecycle events, attribution audit trail, and digest/version projection state. $COMMON_REQ")"
S36_T4="$(create_task "$PROJECT_ID" "S36.4 Manual Testing in @hivemind-test" "Execute roadmap section 36.4 manual validation in $HIVEMIND_TEST_ROOT and publish report at $MANUAL_REPORT_ROOT/sprint-36-manual-report.md. $COMMON_REQ")"
S36_T5="$(create_task "$PROJECT_ID" "S36.5 Validation Gate and Merge Readiness" "Validate Sprint 36 exit criteria, architecture soundness, comprehensive testing, and merge readiness. $MERGE_REQ $COMMON_REQ")"
for t in "$S36_T1" "$S36_T2" "$S36_T3" "$S36_T4" "$S36_T5"; do set_task_auto "$t"; done
S36_GRAPH="$(create_graph "$PROJECT_ID" "phase3-sprint-36" "$S36_T1" "$S36_T2" "$S36_T3" "$S36_T4" "$S36_T5")"
add_dep "$S36_GRAPH" "$S36_T1" "$S36_T3"
add_dep "$S36_GRAPH" "$S36_T2" "$S36_T3"
add_dep "$S36_GRAPH" "$S36_T3" "$S36_T4"
add_dep "$S36_GRAPH" "$S36_T4" "$S36_T5"
add_verification_gate_checks "36" "$S36_GRAPH" "$S36_T5"
S36_FLOW="$(create_flow "$S36_GRAPH" "phase3-sprint-36-constitution-core")"
set_flow_runtime_defaults "$S36_FLOW"
set_verification_task_runtime "$S36_T5"
printf "flow\t36\tsprint-36\t%s\tConstitution core\n" "$S36_FLOW" >> "$MANIFEST_PATH"

# Sprint 37
S37_T1="$(create_task "$PROJECT_ID" "S37.1 UCP Integration Contract" "Implement roadmap section 37.1 integration contract (UCP authoritative extraction backend, profile compliance, canonical fingerprint/logical key semantics). $COMMON_REQ")"
S37_T2="$(create_task "$PROJECT_ID" "S37.2 Snapshot Projection Model" "Implement roadmap section 37.2 Hivemind snapshot envelope/projection fields under projects/<project-id>/graph_snapshot.json. $COMMON_REQ")"
S37_T3="$(create_task "$PROJECT_ID" "S37.3 Lifecycle and Observability" "Implement roadmap section 37.3 triggers and observability: attach/checkpoint/merge/manual refresh lifecycle and explicit snapshot events. $COMMON_REQ")"
S37_T4="$(create_task "$PROJECT_ID" "S37.4 Integrity and Staleness Gates" "Implement roadmap section 37.4 stale snapshot detection, failure hints, canonical fingerprint integrity, and logical-key semantic diffing. $COMMON_REQ")"
S37_T5="$(create_task "$PROJECT_ID" "S37.5 Compatibility and Non-Regression" "Implement roadmap section 37.5 non-regression contract for existing execution semantics, replay determinism, CLI contracts, adapters, and non-constitution projects. $COMMON_REQ")"
S37_T6="$(create_task "$PROJECT_ID" "S37.6 Manual Testing in @hivemind-test" "Execute roadmap section 37.6 manual integration validation in $HIVEMIND_TEST_ROOT and publish report at $MANUAL_REPORT_ROOT/sprint-37-manual-report.md. $COMMON_REQ")"
S37_T7="$(create_task "$PROJECT_ID" "S37.7 Validation Gate and Merge Readiness" "Validate Sprint 37 exit criteria, architecture soundness, comprehensive testing, and merge readiness. $MERGE_REQ $COMMON_REQ")"
for t in "$S37_T1" "$S37_T2" "$S37_T3" "$S37_T4" "$S37_T5" "$S37_T6" "$S37_T7"; do set_task_auto "$t"; done
S37_GRAPH="$(create_graph "$PROJECT_ID" "phase3-sprint-37" "$S37_T1" "$S37_T2" "$S37_T3" "$S37_T4" "$S37_T5" "$S37_T6" "$S37_T7")"
add_dep "$S37_GRAPH" "$S37_T1" "$S37_T3"
add_dep "$S37_GRAPH" "$S37_T2" "$S37_T3"
add_dep "$S37_GRAPH" "$S37_T3" "$S37_T4"
add_dep "$S37_GRAPH" "$S37_T3" "$S37_T5"
add_dep "$S37_GRAPH" "$S37_T4" "$S37_T6"
add_dep "$S37_GRAPH" "$S37_T5" "$S37_T6"
add_dep "$S37_GRAPH" "$S37_T6" "$S37_T7"
add_verification_gate_checks "37" "$S37_GRAPH" "$S37_T7"
S37_FLOW="$(create_flow "$S37_GRAPH" "phase3-sprint-37-ucp-integration-snapshots")"
set_flow_runtime_defaults "$S37_FLOW"
set_verification_task_runtime "$S37_T7"
printf "flow\t37\tsprint-37\t%s\tUCP graph integration and snapshot projection\n" "$S37_FLOW" >> "$MANIFEST_PATH"

# Sprint 38
S38_T1="$(create_task "$PROJECT_ID" "S38.1 Enforcement Engine" "Implement roadmap section 38.1 validate(graph_snapshot, constitution), severity-aware outcomes, and ConstitutionViolationDetected evidence events. $COMMON_REQ")"
S38_T2="$(create_task "$PROJECT_ID" "S38.2 Enforcement Gates" "Implement roadmap section 38.2 constitution checks after checkpoints and pre-merge gates, plus explicit constitution check CLI command. $COMMON_REQ")"
S38_T3="$(create_task "$PROJECT_ID" "S38.3 UX and Explainability" "Implement roadmap section 38.3 structured violation outputs, persisted history, and no hidden retries/downgrades. $COMMON_REQ")"
S38_T4="$(create_task "$PROJECT_ID" "S38.4 Manual Testing in @hivemind-test" "Execute roadmap section 38.4 manual validation in $HIVEMIND_TEST_ROOT and publish report at $MANUAL_REPORT_ROOT/sprint-38-manual-report.md. $COMMON_REQ")"
S38_T5="$(create_task "$PROJECT_ID" "S38.5 Validation Gate and Merge Readiness" "Validate Sprint 38 exit criteria, architecture soundness, comprehensive testing, and merge readiness. $MERGE_REQ $COMMON_REQ")"
for t in "$S38_T1" "$S38_T2" "$S38_T3" "$S38_T4" "$S38_T5"; do set_task_auto "$t"; done
S38_GRAPH="$(create_graph "$PROJECT_ID" "phase3-sprint-38" "$S38_T1" "$S38_T2" "$S38_T3" "$S38_T4" "$S38_T5")"
add_dep "$S38_GRAPH" "$S38_T1" "$S38_T3"
add_dep "$S38_GRAPH" "$S38_T2" "$S38_T3"
add_dep "$S38_GRAPH" "$S38_T3" "$S38_T4"
add_dep "$S38_GRAPH" "$S38_T4" "$S38_T5"
add_verification_gate_checks "38" "$S38_GRAPH" "$S38_T5"
S38_FLOW="$(create_flow "$S38_GRAPH" "phase3-sprint-38-constitution-enforcement")"
set_flow_runtime_defaults "$S38_FLOW"
set_verification_task_runtime "$S38_T5"
printf "flow\t38\tsprint-38\t%s\tConstitution enforcement\n" "$S38_FLOW" >> "$MANIFEST_PATH"

# Sprint 39
S39_T1="$(create_task "$PROJECT_ID" "S39.1 Context Assembly Contract" "Implement roadmap section 39.1 deterministic ordered context assembly inputs, no implicit memory/notepad injection, and explicit size/truncation policy eventing. $COMMON_REQ")"
S39_T2="$(create_task "$PROJECT_ID" "S39.2 Attempt Context Manifest" "Implement roadmap section 39.2 immutable per-attempt context manifests, assembly/delivery events, and attempt inspect context visibility. $COMMON_REQ")"
S39_T3="$(create_task "$PROJECT_ID" "S39.3 Template Resolution Semantics" "Implement roadmap section 39.3 resolution freeze at attempt start, explicit include/exclude overrides with audit events, and retry linkage to prior manifests. $COMMON_REQ")"
S39_T4="$(create_task "$PROJECT_ID" "S39.4 Manual Testing in @hivemind-test" "Execute roadmap section 39.4 manual validation in $HIVEMIND_TEST_ROOT and publish report at $MANUAL_REPORT_ROOT/sprint-39-manual-report.md. $COMMON_REQ")"
S39_T5="$(create_task "$PROJECT_ID" "S39.5 Validation Gate and Merge Readiness" "Validate Sprint 39 exit criteria, architecture soundness, comprehensive testing, and merge readiness. $MERGE_REQ $COMMON_REQ")"
for t in "$S39_T1" "$S39_T2" "$S39_T3" "$S39_T4" "$S39_T5"; do set_task_auto "$t"; done
S39_GRAPH="$(create_graph "$PROJECT_ID" "phase3-sprint-39" "$S39_T1" "$S39_T2" "$S39_T3" "$S39_T4" "$S39_T5")"
add_dep "$S39_GRAPH" "$S39_T1" "$S39_T3"
add_dep "$S39_GRAPH" "$S39_T2" "$S39_T3"
add_dep "$S39_GRAPH" "$S39_T3" "$S39_T4"
add_dep "$S39_GRAPH" "$S39_T4" "$S39_T5"
add_verification_gate_checks "39" "$S39_GRAPH" "$S39_T5"
S39_FLOW="$(create_flow "$S39_GRAPH" "phase3-sprint-39-deterministic-context-assembly")"
set_flow_runtime_defaults "$S39_FLOW"
set_verification_task_runtime "$S39_T5"
printf "flow\t39\tsprint-39\t%s\tDeterministic agent context assembly\n" "$S39_FLOW" >> "$MANIFEST_PATH"

# Sprint 40
S40_T1="$(create_task "$PROJECT_ID" "S40.1 Replay Semantics for Governance" "Implement roadmap section 40.1 governance replay projections, replay verification command(s), and idempotence checks. $COMMON_REQ")"
S40_T2="$(create_task "$PROJECT_ID" "S40.2 Snapshot and Restore" "Implement roadmap section 40.2 optional snapshots, restore procedures preserving event authority, and snapshot lifecycle events. $COMMON_REQ")"
S40_T3="$(create_task "$PROJECT_ID" "S40.3 Corruption and Drift Handling" "Implement roadmap section 40.3 drift detection/preview/repair workflow with structured actionable failure taxonomy. $COMMON_REQ")"
S40_T4="$(create_task "$PROJECT_ID" "S40.4 Manual Testing in @hivemind-test" "Execute roadmap section 40.4 manual validation in $HIVEMIND_TEST_ROOT and publish report at $MANUAL_REPORT_ROOT/sprint-40-manual-report.md. $COMMON_REQ")"
S40_T5="$(create_task "$PROJECT_ID" "S40.5 Validation Gate and Merge Readiness" "Validate Sprint 40 exit criteria, architecture soundness, comprehensive testing, and merge readiness. $MERGE_REQ $COMMON_REQ")"
for t in "$S40_T1" "$S40_T2" "$S40_T3" "$S40_T4" "$S40_T5"; do set_task_auto "$t"; done
S40_GRAPH="$(create_graph "$PROJECT_ID" "phase3-sprint-40" "$S40_T1" "$S40_T2" "$S40_T3" "$S40_T4" "$S40_T5")"
add_dep "$S40_GRAPH" "$S40_T1" "$S40_T3"
add_dep "$S40_GRAPH" "$S40_T2" "$S40_T3"
add_dep "$S40_GRAPH" "$S40_T3" "$S40_T4"
add_dep "$S40_GRAPH" "$S40_T4" "$S40_T5"
add_verification_gate_checks "40" "$S40_GRAPH" "$S40_T5"
S40_FLOW="$(create_flow "$S40_GRAPH" "phase3-sprint-40-governance-replay-recovery")"
set_flow_runtime_defaults "$S40_FLOW"
set_verification_task_runtime "$S40_T5"
printf "flow\t40\tsprint-40\t%s\tGovernance replay and recovery\n" "$S40_FLOW" >> "$MANIFEST_PATH"

# Sprint 41
S41_T1="$(create_task "$PROJECT_ID" "S41.1 Testing and Reliability" "Implement roadmap section 41.1 unit/integration/e2e coverage, regression suites, and concurrency stress testing for governance lifecycle. $COMMON_REQ")"
S41_T2="$(create_task "$PROJECT_ID" "S41.2 Observability and Operations" "Implement roadmap section 41.2 event query filters, diagnostics, and runbooks for migration/drift/constitution policy changes. $COMMON_REQ")"
S41_T3="$(create_task "$PROJECT_ID" "S41.3 Documentation and Adoption" "Implement roadmap section 41.3 CLI/docs updates and quickstart scenario for constitution + template-driven execution. $COMMON_REQ")"
S41_T4="$(create_task "$PROJECT_ID" "S41.4 Manual Testing in @hivemind-test" "Execute roadmap section 41.4 full manual Phase 3 end-to-end validation run in $HIVEMIND_TEST_ROOT and publish report at $MANUAL_REPORT_ROOT/sprint-41-manual-report.md. $COMMON_REQ")"
S41_T5="$(create_task "$PROJECT_ID" "S41.5 Validation Gate and Merge Readiness" "Validate Sprint 41 exit criteria, architecture soundness, comprehensive testing, and merge readiness. $MERGE_REQ $COMMON_REQ")"
S41_T6="$(create_task "$PROJECT_ID" "Phase 3 Completion Gates and Changelog Sync" "Close Phase 3 completion gates: confirm all Sprint 34-41 reports under @hivemind-test, update changelog, and align docs/roadmap completion state. $COMMON_REQ $MERGE_REQ")"
for t in "$S41_T1" "$S41_T2" "$S41_T3" "$S41_T4" "$S41_T5" "$S41_T6"; do set_task_auto "$t"; done
S41_GRAPH="$(create_graph "$PROJECT_ID" "phase3-sprint-41" "$S41_T1" "$S41_T2" "$S41_T3" "$S41_T4" "$S41_T5" "$S41_T6")"
add_dep "$S41_GRAPH" "$S41_T1" "$S41_T3"
add_dep "$S41_GRAPH" "$S41_T2" "$S41_T3"
add_dep "$S41_GRAPH" "$S41_T3" "$S41_T4"
add_dep "$S41_GRAPH" "$S41_T4" "$S41_T5"
add_dep "$S41_GRAPH" "$S41_T5" "$S41_T6"
add_verification_gate_checks "41" "$S41_GRAPH" "$S41_T5"
add_required_check "$S41_GRAPH" "$S41_T6" "all-manual-reports-present" "test -f \"$MANUAL_REPORT_ROOT/sprint-34-manual-report.md\" && test -f \"$MANUAL_REPORT_ROOT/sprint-35-manual-report.md\" && test -f \"$MANUAL_REPORT_ROOT/sprint-36-manual-report.md\" && test -f \"$MANUAL_REPORT_ROOT/sprint-37-manual-report.md\" && test -f \"$MANUAL_REPORT_ROOT/sprint-38-manual-report.md\" && test -f \"$MANUAL_REPORT_ROOT/sprint-39-manual-report.md\" && test -f \"$MANUAL_REPORT_ROOT/sprint-40-manual-report.md\" && test -f \"$MANUAL_REPORT_ROOT/sprint-41-manual-report.md\""
add_required_check "$S41_GRAPH" "$S41_T6" "phase3-roadmap-check" "rg -q \"## Phase 3 Completion Gates\" ops/roadmap/phase-3.md"
S41_FLOW="$(create_flow "$S41_GRAPH" "phase3-sprint-41-governance-production-readiness")"
set_flow_runtime_defaults "$S41_FLOW"
set_verification_task_runtime "$S41_T5"
set_verification_task_runtime "$S41_T6"
printf "flow\t41\tsprint-41\t%s\tGovernance hardening and production readiness\n" "$S41_FLOW" >> "$MANIFEST_PATH"

# Flow dependency graph (parallelized safely)
hm flow add-dependency "$S34_FLOW" "$LAUNCH_FLOW" >/dev/null
hm flow add-dependency "$S35_FLOW" "$S34_FLOW" >/dev/null
hm flow add-dependency "$S36_FLOW" "$S34_FLOW" >/dev/null
hm flow add-dependency "$S37_FLOW" "$S36_FLOW" >/dev/null
hm flow add-dependency "$S38_FLOW" "$S36_FLOW" >/dev/null
hm flow add-dependency "$S38_FLOW" "$S37_FLOW" >/dev/null
hm flow add-dependency "$S39_FLOW" "$S35_FLOW" >/dev/null
hm flow add-dependency "$S39_FLOW" "$S37_FLOW" >/dev/null
hm flow add-dependency "$S40_FLOW" "$S38_FLOW" >/dev/null
hm flow add-dependency "$S40_FLOW" "$S39_FLOW" >/dev/null
hm flow add-dependency "$S41_FLOW" "$S40_FLOW" >/dev/null

# Auto-run for all sprint flows (they remain CREATED until dependencies are completed).
for flow_id in "$S34_FLOW" "$S35_FLOW" "$S36_FLOW" "$S37_FLOW" "$S38_FLOW" "$S39_FLOW" "$S40_FLOW" "$S41_FLOW"; do
  hm flow set-run-mode "$flow_id" auto >/dev/null
done

printf "dependency\t-\tS34<-launch\t%s\t%s\n" "$S34_FLOW" "$LAUNCH_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS35<-S34\t%s\t%s\n" "$S35_FLOW" "$S34_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS36<-S34\t%s\t%s\n" "$S36_FLOW" "$S34_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS37<-S36\t%s\t%s\n" "$S37_FLOW" "$S36_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS38<-S36\t%s\t%s\n" "$S38_FLOW" "$S36_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS38<-S37\t%s\t%s\n" "$S38_FLOW" "$S37_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS39<-S35\t%s\t%s\n" "$S39_FLOW" "$S35_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS39<-S37\t%s\t%s\n" "$S39_FLOW" "$S37_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS40<-S38\t%s\t%s\n" "$S40_FLOW" "$S38_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS40<-S39\t%s\t%s\n" "$S40_FLOW" "$S39_FLOW" >> "$MANIFEST_PATH"
printf "dependency\t-\tS41<-S40\t%s\t%s\n" "$S41_FLOW" "$S40_FLOW" >> "$MANIFEST_PATH"

echo "Phase 3 sprint flow orchestration created."
echo "Project: $PROJECT_NAME ($PROJECT_ID)"
echo "Launch gate flow (manual start): $LAUNCH_FLOW"
echo "Sprint flows:"
echo "  34: $S34_FLOW"
echo "  35: $S35_FLOW"
echo "  36: $S36_FLOW"
echo "  37: $S37_FLOW"
echo "  38: $S38_FLOW"
echo "  39: $S39_FLOW"
echo "  40: $S40_FLOW"
echo "  41: $S41_FLOW"
echo "Manifest: $MANIFEST_PATH"
echo "To launch automation chain: $HM_BIN flow start $LAUNCH_FLOW"
