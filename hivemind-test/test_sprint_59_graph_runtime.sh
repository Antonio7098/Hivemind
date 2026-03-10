#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOST_HOME="${HOME:-}"
HIVEMIND_EXPLICIT="${HIVEMIND:-}"
if [[ -z "$HIVEMIND_EXPLICIT" ]]; then
  echo "+ cargo build --bin hivemind"
  (cd "$REPO_ROOT" && cargo build --bin hivemind)
fi
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/debug/hivemind}"
if [[ ! -x "$HIVEMIND" && -x "$HOST_HOME/.cargo/shared-target/debug/hivemind" ]]; then
  HIVEMIND="$HOST_HOME/.cargo/shared-target/debug/hivemind"
fi
MODEL_ID="${OPENROUTER_MODEL_ID:-openrouter/openai/gpt-4o-mini}"
GRAPH_QUERY_PYTHON_BIN="${HIVEMIND_GRAPH_QUERY_PYTHON:-$REPO_ROOT/.venv-ucp/bin/python}"
REPORT_DIR="$REPO_ROOT/hivemind-test/test-report"
DATE_TAG="$(date +%F)"
LOG_PATH="$REPORT_DIR/${DATE_TAG}-sprint59-graph-runtime.log"
EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-sprint59-graph-runtime-events.json"
REPORT_PATH="$REPORT_DIR/${DATE_TAG}-sprint59-graph-runtime-report.md"
if [[ -f "$REPO_ROOT/hivemind-test/.env" ]]; then set -a; source "$REPO_ROOT/hivemind-test/.env"; set +a; fi
[[ -x "$HIVEMIND" ]] || { echo "missing hivemind binary: $HIVEMIND" >&2; exit 1; }
[[ -n "${OPENROUTER_API_KEY:-}" ]] || { echo "OPENROUTER_API_KEY is required" >&2; exit 1; }
[[ -x "$GRAPH_QUERY_PYTHON_BIN" ]] || { echo "missing graph query python interpreter: $GRAPH_QUERY_PYTHON_BIN" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
command -v sqlite3 >/dev/null || { echo "sqlite3 is required" >&2; exit 1; }
mkdir -p "$REPORT_DIR"
exec > >(tee "$LOG_PATH") 2>&1

run() { echo; echo "+ $*"; "$@"; }
capture() { local out; echo >&2; echo "+ $*" >&2; out="$("$@" 2>&1)"; echo "$out" >&2; printf "%s" "$out"; }
assert_contains() { [[ "$1" == *"$2"* ]] || { echo "ASSERT FAILED [$3]: missing '$2'" >&2; exit 1; }; }
json_id() { jq -r '.data.id // .data.task_id // .data.graph_id // .data.flow_id // .data.project_id // .graph_id // .flow_id // .project_id // empty'; }

complete_checkpoint_if_needed() {
  local flow_id="$1" tick_output="$2"
  [[ "$tick_output" == *"checkpoints_incomplete"* ]] || return 0
  local attempt_id
  attempt_id="$($HIVEMIND -f json attempt list --flow "$flow_id" --limit 1 | jq -r '.data[0].attempt_id // empty')"
  [[ -n "$attempt_id" ]] || return 1
  run "$HIVEMIND" checkpoint complete --attempt-id "$attempt_id" --id checkpoint-1 --summary "sprint59 graph runtime checkpoint"
}

run_flow_until_terminal() {
  local flow_id="$1"
  local tick=0
  while (( tick < 25 )); do
    tick=$((tick + 1))
    local state
    state="$($HIVEMIND -f json flow status "$flow_id" | jq -r '.data.state' | tr '[:upper:]' '[:lower:]')"
    echo "flow=$flow_id tick=$tick state=$state"
    [[ "$state" == "completed" || "$state" == "merged" ]] && return 0
    local tick_out tick_rc=0
    set +e
    tick_out="$($HIVEMIND flow tick "$flow_id" 2>&1)"
    tick_rc=$?
    set -e
    echo "$tick_out"
    complete_checkpoint_if_needed "$flow_id" "$tick_out" || true
    if (( tick_rc == 0 )); then sleep 1; continue; fi
    [[ "$tick_out" == *"checkpoints_incomplete"* || "$tick_out" == *"verification_pending"* ]] && { sleep 1; continue; }
    sleep 1
  done
  echo "flow did not reach terminal state" >&2
  return 1
}

echo "=== Sprint 59 Graph Runtime Validation ==="
echo "Using model: $MODEL_ID"
echo "Using hivemind binary: $HIVEMIND"
echo "Using graph query python: $GRAPH_QUERY_PYTHON_BIN"
TMP_BASE="/tmp/hm-sprint59-graph-runtime"
export HOME="$TMP_BASE/home"
rm -rf "$TMP_BASE"
mkdir -p "$HOME"

PROJECT_JSON="$(capture "$HIVEMIND" -f json project create sprint59-graph-runtime --description 'Sprint 59 graph runtime validation project')"
PROJECT_ID="$(printf '%s' "$PROJECT_JSON" | json_id)"
run "$HIVEMIND" project attach-repo sprint59-graph-runtime "$REPO_ROOT" --name repo
run "$HIVEMIND" project governance init sprint59-graph-runtime
run "$HIVEMIND" graph snapshot refresh sprint59-graph-runtime
CONSTITUTION_CONTENT=$'version: 1\nschema_version: constitution.v1\ncompatibility:\n  minimum_hivemind_version: 0.1.0\n  governance_schema_version: governance.v1\npartitions:\n  - id: core\n    path: src\nrules: []'
run "$HIVEMIND" constitution init sprint59-graph-runtime --content "$CONSTITUTION_CONTENT" --confirm
run "$HIVEMIND" project runtime-set sprint59-graph-runtime --role worker --adapter native --binary-path builtin-native --model "$MODEL_ID" --timeout-ms 180000 --env HIVEMIND_NATIVE_PROVIDER=openrouter --env HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS=true --env HIVEMIND_GRAPH_QUERY_PYTHON="$GRAPH_QUERY_PYTHON_BIN" --env HIVEMIND_NATIVE_MAX_TURNS=12

TASK_PROMPT=$(cat <<'EOF'
Do not edit source files.
The core task is GraphCode exploration and report generation, not checkpoint management.
Never call checkpoint_complete before the report file is written.
Never return DONE until after the report file exists.
Use graph_query at least three times, including at least two python_query calls, then read_file once, then write exactly one report file.
Emit exactly one directive per turn.
Start immediately with step 1 as an ACT directive on the first turn. Do not emit THINK unless a tool failure blocks progress.
1) ACT:tool:graph_query:{"kind":"filter","node_type":"file","path_prefix":"src/native/tool_engine/graph_query_tool","max_results":12}
2) ACT:tool:graph_query:{"kind":"python_query","repo_name":"repo","max_results":12,"limits":{"max_seconds":4.0,"max_operations":60,"max_trace_events":4000,"max_stdout_chars":1000},"code":"files = graph.find(node_class='file', path_regex=r'src/native/tool_engine/graph_query_tool/(execute|types|snapshot)\\.rs', limit=8)\nsymbols = graph.find(node_class='symbol', name_regex='handle_graph_query|persist_graph_query_session|load_runtime_graph_snapshot', limit=8)\nfor symbol in symbols[:3]:\n    session.add(symbol, detail='summary')\n    session.walk(symbol, mode='dependencies', depth=1, limit=8)\nresult = {'files': [node['logical_key'] for node in files], 'symbols': [node['logical_key'] for node in symbols]}"}
3) ACT:tool:read_file:{"path":"src/native/tool_engine/graph_query_tool/execute.rs"}
4) ACT:tool:graph_query:{"kind":"python_query","repo_name":"repo","max_results":12,"limits":{"max_seconds":4.0,"max_operations":60,"max_trace_events":4000,"max_stdout_chars":1000},"code":"state_symbols = graph.find(node_class='symbol', name_regex='GraphQueryInput|GraphQueryResult|RuntimeGraphSnapshotRepository', limit=10)\nfor symbol in state_symbols[:4]:\n    branch = session.fork()\n    branch.add(symbol, detail='summary')\n    branch.walk(symbol, mode='dependents', depth=1, limit=6)\n    exported = branch.export(compact=True)\n    if any((node.get('path') or '').endswith('execute.rs') for node in exported['nodes']):\n        session.add(symbol, detail='summary')\n        session.walk(symbol, mode='dependents', depth=1, limit=6)\nresult = {'count': len(state_symbols), 'symbols': [node['logical_key'] for node in state_symbols]}"}
5) ACT:tool:write_file:{"path":"docs/sprint59-graph-eval.md","content":"Sprint 59 graph runtime validation complete.\n- Used graph_query filter once.\n- Used graph_query python_query twice.\n- Inspected src/native/tool_engine/graph_query_tool/execute.rs.\n","append":false}
6) ACT:tool:checkpoint_complete:{"id":"checkpoint-1","summary":"completed graph runtime validation and report generation"}
7) DONE:graph runtime validation complete
EOF
)
TASK_JSON="$(capture "$HIVEMIND" -f json task create sprint59-graph-runtime graph-runtime-eval --description "$TASK_PROMPT")"
TASK_ID="$(printf '%s' "$TASK_JSON" | json_id)"
GRAPH_ID="$(capture "$HIVEMIND" -f json graph create sprint59-graph-runtime graph-runtime-eval --from-tasks "$TASK_ID" | jq -r '.data.graph_id // .graph_id // empty')"
FLOW_ID="$(capture "$HIVEMIND" -f json flow create "$GRAPH_ID" | jq -r '.data.flow_id // .flow_id // empty')"
run "$HIVEMIND" flow start "$FLOW_ID"
run_flow_until_terminal "$FLOW_ID"

echo
echo "+ $HIVEMIND -f json events list --limit 5000"
"$HIVEMIND" -f json events list --limit 5000 | tee "$EVENTS_PATH"
FLOW_EVENTS_FILTER='[.data[] | select((.metadata.correlation.flow_id // empty) == $flow_id or (.payload.native_correlation.flow_id // empty) == $flow_id)]'
PROVIDER_LINE="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"agent_invocation_started\")) | map(.provider + \"|\" + .model) | .[0] // empty" "$EVENTS_PATH")"
assert_contains "$PROVIDER_LINE" "openrouter|" "provider provenance"
assert_contains "$PROVIDER_LINE" "$MODEL_ID" "model provenance"
GRAPH_QUERY_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"graph_query_executed\")) | length" "$EVENTS_PATH")"
[[ "$GRAPH_QUERY_COUNT" -ge 3 ]] || { echo "expected >=3 graph_query calls, saw $GRAPH_QUERY_COUNT" >&2; exit 1; }
PYTHON_QUERY_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"graph_query_executed\" and .query_kind==\"python_query\")) | length" "$EVENTS_PATH")"
[[ "$PYTHON_QUERY_COUNT" -ge 2 ]] || { echo "expected >=2 python_query graph calls, saw $PYTHON_QUERY_COUNT" >&2; exit 1; }
READ_FILE_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"tool_call_completed\" and .tool_name==\"read_file\")) | length" "$EVENTS_PATH")"
WRITE_FILE_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"tool_call_completed\" and .tool_name==\"write_file\")) | length" "$EVENTS_PATH")"
REPORT_ARTIFACT_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select((.type==\"file_modified\" and .path==\"docs/sprint59-graph-eval.md\") or (.type==\"runtime_filesystem_observed\" and ((.files_created // []) | index(\"docs/sprint59-graph-eval.md\") != null)))) | length" "$EVENTS_PATH")"
[[ "$READ_FILE_COUNT" -ge 1 ]] || { echo "expected >=1 read_file call, saw $READ_FILE_COUNT" >&2; exit 1; }
[[ "$WRITE_FILE_COUNT" -ge 1 ]] || { echo "expected >=1 write_file call, saw $WRITE_FILE_COUNT" >&2; exit 1; }
[[ "$REPORT_ARTIFACT_COUNT" -ge 1 ]] || { echo "missing report artifact event for docs/sprint59-graph-eval.md" >&2; exit 1; }
WORKTREE_PATH="$($HIVEMIND -f json worktree inspect "$TASK_ID" | jq -r '.data.path // empty')"

STATE_DB_PATH="$HOME/.hivemind/native-runtime/native/runtime-state.sqlite"
CODEGRAPH_BACKEND="$(sqlite3 -noheader -batch "$STATE_DB_PATH" "SELECT storage_backend FROM graphcode_artifacts WHERE project_id = '$PROJECT_ID' AND substrate_kind = 'codegraph' LIMIT 1;")"
GENERIC_GRAPH_COUNT="$(sqlite3 -noheader -batch "$STATE_DB_PATH" "SELECT COUNT(1) FROM graphcode_artifacts WHERE project_id = '$PROJECT_ID' AND substrate_kind = 'graph';")"
WORKTREE_PROVENANCE="$(sqlite3 -noheader -batch "$STATE_DB_PATH" "SELECT repo_manifest_json FROM graphcode_artifacts WHERE project_id = '$PROJECT_ID' AND substrate_kind = 'codegraph' LIMIT 1;")"
assert_contains "$CODEGRAPH_BACKEND" "ucp_graph_sqlite" "codegraph storage backend"
assert_contains "$WORKTREE_PROVENANCE" "worktree_path" "repo/worktree provenance"
[[ "$GENERIC_GRAPH_COUNT" -ge 1 ]] || { echo "expected generic graph artifact" >&2; exit 1; }

cat > "$REPORT_PATH" <<EOF
## Sprint 59 Graph Runtime Validation

- Provider/model: ${PROVIDER_LINE}
- Flow: ${FLOW_ID}
- Task: ${TASK_ID}
- Graph queries observed: ${GRAPH_QUERY_COUNT}
- Python graph queries observed: ${PYTHON_QUERY_COUNT}
- read_file calls observed: ${READ_FILE_COUNT}
- write_file calls observed: ${WRITE_FILE_COUNT}
- Report artifact events observed: ${REPORT_ARTIFACT_COUNT}
- Codegraph storage backend: ${CODEGRAPH_BACKEND}
- Generic graph artifacts: ${GENERIC_GRAPH_COUNT}
- Events log: $(basename "$EVENTS_PATH")
- Expected worktree path: ${WORKTREE_PATH}
- Artifact path (event-confirmed): docs/sprint59-graph-eval.md
EOF

echo "Report written to $REPORT_PATH"