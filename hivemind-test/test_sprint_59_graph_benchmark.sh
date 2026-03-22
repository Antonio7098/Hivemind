#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOST_HOME="${HOME:-}"
if [[ -f "$SCRIPT_DIR/.env" ]]; then
  set -a; source "$SCRIPT_DIR/.env"; set +a
fi
if [[ -z "${HIVEMIND:-}" ]]; then
  echo "+ cargo build --bin hivemind"
  (cd "$REPO_ROOT" && cargo build --bin hivemind)
fi
RESOLVED_HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/debug/hivemind}"
if [[ ! -x "$RESOLVED_HIVEMIND" && -x "$HOST_HOME/.cargo/shared-target/debug/hivemind" ]]; then
  RESOLVED_HIVEMIND="$HOST_HOME/.cargo/shared-target/debug/hivemind"
fi
if [[ ! -x "$RESOLVED_HIVEMIND" ]]; then
  RESOLVED_HIVEMIND="$(cd "$REPO_ROOT" && cargo build --bin hivemind --message-format=json-render-diagnostics | jq -r 'select(.reason=="compiler-artifact" and .target.name=="hivemind") | .executable // empty' | tail -n 1)"
fi
[[ -x "$RESOLVED_HIVEMIND" ]] || { echo "missing hivemind binary after build resolution" >&2; exit 1; }
STABLE_BIN_DIR="${TMPDIR:-/tmp}/hm-sprint59-graph-benchmark-cli-bin"
mkdir -p "$STABLE_BIN_DIR"
HIVEMIND="$STABLE_BIN_DIR/hivemind"
cp "$RESOLVED_HIVEMIND" "$HIVEMIND"
chmod +x "$HIVEMIND"
MODEL_ID="${HIVEMIND_SPRINT59_GRAPH_BENCHMARK_MODEL_ID:-openrouter/openai/gpt-4o-mini}"
GRAPH_QUERY_PYTHON_BIN="${HIVEMIND_GRAPH_QUERY_PYTHON:-$REPO_ROOT/.venv-ucp/bin/python}"
REPORT_DIR="$REPO_ROOT/hivemind-test/test-report"
DATE_TAG="$(date +%F)"
LOG_PATH="$REPORT_DIR/${DATE_TAG}-sprint59-graph-benchmark.log"
EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-sprint59-graph-benchmark-events.json"
REPORT_PATH="$REPORT_DIR/${DATE_TAG}-sprint59-graph-benchmark-report.md"
mkdir -p "$REPORT_DIR"
exec > >(tee "$LOG_PATH") 2>&1

run() { echo; echo "+ $*"; "$@"; }
capture() { local out; echo >&2; echo "+ $*" >&2; out="$("$@" 2>&1)"; echo "$out" >&2; printf "%s" "$out"; }
assert_contains() { [[ "$1" == *"$2"* ]] || { echo "ASSERT FAILED [$3]: missing '$2'" >&2; exit 1; }; }
assert_eq() { [[ "$1" == "$2" ]] || { echo "ASSERT FAILED [$3]: expected '$2' got '$1'" >&2; exit 1; }; }
json_id() { jq -r '.data.id // .data.task_id // .data.graph_id // .data.flow_id // .data.project_id // .graph_id // .flow_id // .project_id // empty'; }

complete_checkpoint_if_needed() {
  local flow_id="$1" tick_output="$2"
  [[ "$tick_output" == *"checkpoints_incomplete"* ]] || return 0
  local attempt_id
  attempt_id="$($HIVEMIND -f json attempt list --flow "$flow_id" --limit 1 | jq -r '.data[0].attempt_id // empty')"
  [[ -n "$attempt_id" ]] || return 1
  run "$HIVEMIND" checkpoint complete --attempt-id "$attempt_id" --id checkpoint-1 --summary "sprint59 graph benchmark checkpoint"
}

run_flow_until_terminal() {
  local flow_id="$1"; local max_ticks="${2:-120}"
  local tick=0
  while (( tick < max_ticks )); do
    tick=$((tick + 1))
    local status_json
    status_json="$($HIVEMIND -f json flow status "$flow_id")"
    local state; state="$(printf '%s' "$status_json" | jq -r '.data.state')"
    echo "flow=${flow_id} tick=${tick} state=${state}"
    case "$state" in completed|failed|cancelled) return 0 ;; esac
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
  echo "flow ${flow_id} did not reach terminal state within ${max_ticks} ticks" >&2
  exit 1
}

TMP_HOME="${TMPDIR:-/tmp}/hm-sprint59-graph-benchmark/home"
rm -rf "${TMP_HOME%/home}"
mkdir -p "$TMP_HOME"
export HOME="$TMP_HOME"
export HIVEMIND_DATA_DIR="$HOME/.hivemind"
export RUST_LOG="${RUST_LOG:-error}"

echo "=== Sprint 59 Graph Benchmark Validation ==="
echo "Using model: $MODEL_ID"
echo "Using hivemind binary: $HIVEMIND"
echo "Using graph query python: $GRAPH_QUERY_PYTHON_BIN"

PROJECT_JSON="$(capture "$HIVEMIND" -f json project create sprint59-graph-benchmark --description "Sprint 59 graph benchmark validation project")"
PROJECT_ID="$(printf '%s' "$PROJECT_JSON" | json_id)"
run "$HIVEMIND" project attach-repo sprint59-graph-benchmark "$REPO_ROOT" --name repo
run "$HIVEMIND" project governance init sprint59-graph-benchmark
run "$HIVEMIND" graph snapshot refresh sprint59-graph-benchmark
run "$HIVEMIND" constitution init sprint59-graph-benchmark --content $'version: 1\nschema_version: constitution.v1\ncompatibility:\n  minimum_hivemind_version: 0.1.0\n  governance_schema_version: governance.v1\npartitions:\n  - id: core\n    path: src\nrules: []' --confirm
run "$HIVEMIND" project runtime-set sprint59-graph-benchmark --role worker --adapter native --binary-path builtin-native --model "$MODEL_ID" --timeout-ms 180000 --env HIVEMIND_NATIVE_PROVIDER=openrouter --env HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS=true --env HIVEMIND_GRAPH_QUERY_PYTHON="$GRAPH_QUERY_PYTHON_BIN" --env HIVEMIND_NATIVE_MAX_TURNS=16

TASK_DESC=$(cat <<'EOF'
Do not edit source files.
The core task is likely-test ranking and report generation, not checkpoint management.
Never call checkpoint_complete before the report file is written.
Never return DONE until after the report file exists.
Run a benchmark-style GraphCode workflow for likely-test ranking on this repository.
Execute the numbered steps exactly once, in order. Never repeat a successful step.
Use graph_query at least twice, starting with the filter step, then the ranking python_query, then read_file once, then write exactly one report file.
Your report must include the top two ranked logical keys from step 2 verbatim.
Treat the `ranked` array returned by step 2 as authoritative. In step 4, copy `ranked[0].logical_key` and `ranked[1].logical_key` exactly as returned; do not shorten them to file paths or node summaries.
Preserve the exact literal logical_key strings, including the `symbol:` prefix when present.
Do not add scores, counts, parentheses, markdown emphasis, or any extra commentary around the logical keys.
The first directive MUST be step 1 exactly. Do not skip directly to step 2.
After step 1 succeeds, the next directive MUST be step 2 exactly.
After step 2 succeeds, the next directive MUST be step 3 exactly.
After step 3 succeeds, the next directive MUST be step 4, using `report_content` from step 2 exactly with no edits.
After step 4 succeeds, the next directive MUST be step 5 exactly.
Emit exactly one directive per turn.
Start immediately with step 1 as an ACT directive on the first turn. Do not emit THINK unless a tool failure blocks progress.
1) ACT:tool:graph_query:{"kind":"filter","node_type":"file","path_prefix":"src/native/tool_engine/graph_query_tool","max_results":12}
2) ACT:tool:graph_query:{"kind":"python_query","repo_name":"repo","max_results":12,"limits":{"max_seconds":2.0,"max_operations":120,"max_trace_events":3000,"max_stdout_chars":600},"code":"tests = graph.find(node_class='symbol', path_regex='(tests/integration.rs|src/native/tests.rs)', limit=120)\nkeywords = ['graph', 'query', 'runtime']\nranked = []\nfor node in tests:\n    logical_key = (node.get('logical_key') or '').lower()\n    score = 0\n    for keyword in keywords:\n        if keyword in logical_key:\n            score += 1\n    if score:\n        ranked.append({'score': score, 'logical_key': node.get('logical_key')})\nranked = sorted(ranked, key=lambda item: (-item['score'], item['logical_key']))[:5]\nfor item in ranked[:3]:\n    matches = [node for node in tests if node.get('logical_key') == item['logical_key']]\n    if matches:\n        session.add(matches[0], detail='summary')\ntop1 = ranked[0]['logical_key'] if len(ranked) > 0 else ''\ntop2 = ranked[1]['logical_key'] if len(ranked) > 1 else ''\nreport_content = f'Sprint 59 benchmark validation complete.\\n- Top ranked logical key: {top1}\\n- Second ranked logical key: {top2}\\n'\nresult = {'ranked': ranked, 'top1': top1, 'top2': top2, 'report_content': report_content}"}
3) ACT:tool:read_file:{"path":"src/native/tests.rs"}
4) ACT:tool:write_file:{"path":"docs/sprint59-graph-benchmark.md","content":"<replace with python_result.report_content from step 2 exactly, byte-for-byte, with no edits>","append":false}
5) ACT:tool:checkpoint_complete:{"id":"checkpoint-1","summary":"completed graph benchmark validation and report generation"}
6) DONE:graph benchmark validation complete
EOF
)
TASK_JSON="$(capture "$HIVEMIND" -f json task create sprint59-graph-benchmark graph-benchmark-eval --description "$TASK_DESC")"
TASK_ID="$(printf '%s' "$TASK_JSON" | json_id)"
GRAPH_JSON="$(capture "$HIVEMIND" -f json graph create sprint59-graph-benchmark graph-benchmark-eval --from-tasks "$TASK_ID")"
GRAPH_ID="$(printf '%s' "$GRAPH_JSON" | json_id)"
FLOW_JSON="$(capture "$HIVEMIND" -f json flow create "$GRAPH_ID")"
FLOW_ID="$(printf '%s' "$FLOW_JSON" | json_id)"
run "$HIVEMIND" flow start "$FLOW_ID"
run_flow_until_terminal "$FLOW_ID" 120

"$HIVEMIND" -f json events list --limit 5000 | tee "$EVENTS_PATH"
FLOW_EVENTS_FILTER='[.data[] | select((.metadata.correlation.flow_id // empty) == $flow_id or (.payload.native_correlation.flow_id // empty) == $flow_id)]'
FLOW_STATUS_JSON="$($HIVEMIND -f json flow status "$FLOW_ID")"
FLOW_STATE="$(printf '%s' "$FLOW_STATUS_JSON" | jq -r '.data.state')"
TASK_STATE="$(printf '%s' "$FLOW_STATUS_JSON" | jq -r --arg task_id "$TASK_ID" '.data.task_executions[$task_id].state // empty')"
assert_eq "$FLOW_STATE" "completed" "flow terminal state"
case "$TASK_STATE" in success|completed) ;; *) echo "ASSERT FAILED [task execution state]: expected success/completed got '$TASK_STATE'" >&2; exit 1 ;; esac

PROVIDER_LINE="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"agent_invocation_started\")) | map(.provider + \"|\" + .model) | .[0] // empty" "$EVENTS_PATH")"
assert_contains "$PROVIDER_LINE" "openrouter|" "provider provenance"
assert_contains "$PROVIDER_LINE" "$MODEL_ID" "model provenance"
GRAPH_QUERY_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"graph_query_executed\")) | length" "$EVENTS_PATH")"
FILTER_QUERY_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"graph_query_executed\" and .query_kind==\"filter\")) | length" "$EVENTS_PATH")"
PYTHON_QUERY_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"graph_query_executed\" and .query_kind==\"python_query\")) | length" "$EVENTS_PATH")"
READ_FILE_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"tool_call_completed\" and .tool_name==\"read_file\")) | length" "$EVENTS_PATH")"
WRITE_FILE_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"tool_call_completed\" and .tool_name==\"write_file\")) | length" "$EVENTS_PATH")"
REPORT_ARTIFACT_COUNT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select((.type==\"file_modified\" and .path==\"docs/sprint59-graph-benchmark.md\") or (.type==\"runtime_filesystem_observed\" and ((.files_created // []) | index(\"docs/sprint59-graph-benchmark.md\") != null)))) | length" "$EVENTS_PATH")"
[[ "$GRAPH_QUERY_COUNT" -ge 2 ]] || { echo "expected >=2 graph queries, saw $GRAPH_QUERY_COUNT" >&2; exit 1; }
[[ "$FILTER_QUERY_COUNT" -ge 1 ]] || { echo "expected >=1 filter graph query, saw $FILTER_QUERY_COUNT" >&2; exit 1; }
[[ "$PYTHON_QUERY_COUNT" -ge 1 ]] || { echo "expected >=1 python_query, saw $PYTHON_QUERY_COUNT" >&2; exit 1; }
[[ "$READ_FILE_COUNT" -ge 1 ]] || { echo "expected >=1 read_file call, saw $READ_FILE_COUNT" >&2; exit 1; }
[[ "$WRITE_FILE_COUNT" -ge 1 ]] || { echo "expected >=1 write_file call, saw $WRITE_FILE_COUNT" >&2; exit 1; }
[[ "$REPORT_ARTIFACT_COUNT" -ge 1 ]] || { echo "missing report artifact event for docs/sprint59-graph-benchmark.md" >&2; exit 1; }

RANKING_PAYLOAD="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"tool_call_completed\" and .tool_name==\"graph_query\")) | map(.response.payload | fromjson | .output) | map(select(.query_kind==\"python_query\" and (.python_result.ranked // empty))) | .[0] // empty" "$EVENTS_PATH")"
TOP1="$(printf '%s' "$RANKING_PAYLOAD" | jq -r '.python_result.ranked[0].logical_key // empty')"
TOP2="$(printf '%s' "$RANKING_PAYLOAD" | jq -r '.python_result.ranked[1].logical_key // empty')"
[[ -n "$TOP1" && -n "$TOP2" ]] || { echo "missing ranked candidates in python_query output" >&2; exit 1; }

WORKTREE_PATH="$($HIVEMIND -f json worktree inspect "$TASK_ID" | jq -r '.data.path // empty')"
WRITE_FILE_CONTENT="$(jq -r --arg flow_id "$FLOW_ID" "$FLOW_EVENTS_FILTER | map(.payload) | map(select(.type==\"tool_call_requested\" and .tool_name==\"write_file\")) | map(.request.payload | fromjson | .input.content // empty) | .[0] // empty" "$EVENTS_PATH")"
assert_contains "$WRITE_FILE_CONTENT" "$TOP1" "top ranked candidate in report content"
assert_contains "$WRITE_FILE_CONTENT" "$TOP2" "second ranked candidate in report content"

cat > "$REPORT_PATH" <<EOF
## Sprint 59 Graph Benchmark Validation

- Provider/model: ${PROVIDER_LINE}
- Flow: ${FLOW_ID}
- Flow state: ${FLOW_STATE}
- Task: ${TASK_ID}
- Task state: ${TASK_STATE}
- Graph queries observed: ${GRAPH_QUERY_COUNT}
- Filter graph queries observed: ${FILTER_QUERY_COUNT}
- Python graph queries observed: ${PYTHON_QUERY_COUNT}
- read_file calls observed: ${READ_FILE_COUNT}
- write_file calls observed: ${WRITE_FILE_COUNT}
- Report artifact events observed: ${REPORT_ARTIFACT_COUNT}
- Top ranked candidate: ${TOP1}
- Second ranked candidate: ${TOP2}
- Events log: $(basename "$EVENTS_PATH")
- Expected worktree path: ${WORKTREE_PATH}
- Artifact path (event-confirmed): docs/sprint59-graph-benchmark.md
EOF

echo "Report written to $REPORT_PATH"