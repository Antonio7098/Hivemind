#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOST_HOME="${HOME:-}"
HOST_CARGO_HOME="${CARGO_HOME:-$HOST_HOME/.cargo}"
HOST_RUSTUP_HOME="${RUSTUP_HOME:-$HOST_HOME/.rustup}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
if [[ -z "${HIVEMIND:-}" ]]; then
  echo "+ cargo build --bin hivemind"
  (cd "$REPO_ROOT" && cargo build --bin hivemind)
fi
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/debug/hivemind}"
if [[ ! -x "$HIVEMIND" && -x "$HOST_HOME/.cargo/shared-target/debug/hivemind" ]]; then
  HIVEMIND="$HOST_HOME/.cargo/shared-target/debug/hivemind"
fi
GRAPH_QUERY_PYTHON_BIN="${HIVEMIND_GRAPH_QUERY_PYTHON:-$REPO_ROOT/.venv-ucp/bin/python}"
REPORT_DIR="$REPO_ROOT/hivemind-test/test-report"
DATE_TAG="$(date +%F-%H%M%S)"
RUN_KEY="deep-prompt-provenance"
LOG_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}.log"
EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-events.json"
NATIVE_SUMMARY_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-native-summary.json"
NATIVE_CONTEXT_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-native-context.json"
REQUEST_SNAPSHOTS_DIR="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-request-snapshots"
ATTEMPTS_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-attempts.json"
FLOW_STATUS_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-flow-status.json"
TRACE_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-tick-trace.jsonl"
REPORT_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-report.md"
QUALITY_PATH="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-quality-gate.txt"
if [[ -f "$REPO_ROOT/hivemind-test/.env" ]]; then set -a; source "$REPO_ROOT/hivemind-test/.env"; set +a; fi
PROVIDER_NAME="${HIVEMIND_DEEP_OBS_PROVIDER:-openrouter}"
MODEL_ID="${HIVEMIND_DEEP_OBS_MODEL_ID:-${OPENROUTER_MODEL_ID:-nvidia/nemotron-3-nano-30b-a3b:free}}"
NATIVE_MAX_TURNS="${HIVEMIND_NATIVE_MAX_TURNS:-120}"
[[ -x "$HIVEMIND" ]] || { echo "missing hivemind binary: $HIVEMIND" >&2; exit 1; }
if [[ "$PROVIDER_NAME" == "openrouter" ]]; then
  [[ -n "${OPENROUTER_API_KEY:-}" ]] || { echo "OPENROUTER_API_KEY is required" >&2; exit 1; }
elif [[ "$PROVIDER_NAME" == "groq" ]]; then
  [[ -n "${GROQ_API_KEY:-}" ]] || { echo "GROQ_API_KEY is required" >&2; exit 1; }
elif [[ "$PROVIDER_NAME" == "minimax" ]]; then
  [[ -n "${MINIMAX_API_KEY:-}" ]] || { echo "MINIMAX_API_KEY is required" >&2; exit 1; }
fi
[[ -x "$GRAPH_QUERY_PYTHON_BIN" ]] || { echo "missing graph query python interpreter: $GRAPH_QUERY_PYTHON_BIN" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
mkdir -p "$REPORT_DIR"

HIVEMIND_SOURCE="$HIVEMIND"
HIVEMIND_STABLE_COPY="$(mktemp "$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-hivemind.XXXXXX")"
cp "$HIVEMIND_SOURCE" "$HIVEMIND_STABLE_COPY"
chmod +x "$HIVEMIND_STABLE_COPY"
HIVEMIND="$HIVEMIND_STABLE_COPY"

cleanup() {
  [[ -n "${HIVEMIND_STABLE_COPY:-}" ]] && rm -f "$HIVEMIND_STABLE_COPY"
}
trap cleanup EXIT

exec > >(tee "$LOG_PATH") 2>&1

run() { echo; echo "+ $*"; "$@"; }
capture() { local out; echo >&2; echo "+ $*" >&2; out="$("$@" 2>&1)"; echo "$out" >&2; printf "%s" "$out"; }
capture_file() { local path="$1"; shift; echo >&2; echo "+ $*" >&2; "$@" | tee "$path"; }
capture_file_quiet() { local path="$1"; shift; echo >&2; echo "+ $*" >&2; "$@" > "$path"; }
json_id() { jq -r '.data.id // .data.task_id // .data.graph_id // .data.flow_id // .data.project_id // .graph_id // .flow_id // .project_id // empty'; }
attempt_report_path() { printf '%s/%s-%s-attempt-%s.json' "$REPORT_DIR" "$DATE_TAG" "$RUN_KEY" "$1"; }

append_quality_failure() {
  local message="$1"
  QUALITY_FAILURES+=("$message")
}

attempt_touches_regex() {
  local attempt_file="$1" regex="$2"
  jq -e --arg re "$regex" '[.files_created[]?, .files_modified[]?, .files_deleted[]?] | any(test($re))' "$attempt_file" >/dev/null
}

attempt_match_count() {
  local attempt_file="$1" regex="$2"
  jq -r --arg re "$regex" '[.files_created[]?, .files_modified[]?, .files_deleted[]?] | map(select(test($re))) | unique | length' "$attempt_file"
}

attempt_commands_summary() {
  local attempt_id="$1"
  jq -r --arg aid "$attempt_id" '
    def cmd_text:
      (.payload.request.payload? | fromjson? | .input) as $input
      | (($input.command? // $input.cmd? // empty)
        + (if (($input.args? // []) | length) > 0 then " " + (($input.args // []) | join(" ")) else "" end));
    [
      .data[]
      | select(.metadata.correlation.attempt_id==$aid and .payload.type=="tool_call_completed" and (.payload.tool_name=="run_command" or .payload.tool_name=="exec_command"))
      | (cmd_text + " => exit=" + ((((.payload.response.payload? | fromjson? | .output.exit_code?) // 0) | tostring)))
    ]
    | unique
    | join(" || ")
  ' "$EVENTS_PATH"
}

attempt_nonzero_command_count() {
  local attempt_id="$1"
  jq -r --arg aid "$attempt_id" '
    [
      .data[]
      | select(.metadata.correlation.attempt_id==$aid and .payload.type=="tool_call_completed" and (.payload.tool_name=="run_command" or .payload.tool_name=="exec_command"))
      | ((.payload.response.payload? | fromjson? | .output.exit_code?) // 0)
      | select(. != 0)
    ]
    | length
  ' "$EVENTS_PATH"
}

attempt_read_paths() {
  local attempt_id="$1"
  jq -r --arg aid "$attempt_id" '
    [
      .data[]
      | select(.metadata.correlation.attempt_id==$aid and .payload.type=="tool_call_requested" and .payload.tool_name=="read_file")
      | (.payload.request.payload? | fromjson? | .input.path? // empty)
    ]
    | unique
    | join(", ")
  ' "$EVENTS_PATH"
}

attempt_write_paths() {
  local attempt_id="$1"
  jq -r --arg aid "$attempt_id" '
    [
      .data[]
      | select(.metadata.correlation.attempt_id==$aid and .payload.type=="tool_call_requested" and .payload.tool_name=="write_file")
      | (.payload.request.payload? | fromjson? | .input.path? // empty)
    ]
    | unique
    | join(", ")
  ' "$EVENTS_PATH"
}

attempt_tool_failures() {
  local attempt_id="$1"
  jq -r --arg aid "$attempt_id" '
    [
      .data[]
      | select(.metadata.correlation.attempt_id==$aid and .payload.type=="tool_call_failed")
      | (.payload.tool_name + ":" + .payload.code + ":" + .payload.message)
    ]
    | join(" || ")
  ' "$EVENTS_PATH"
}

attempt_suspicious_writes() {
  local attempt_id="$1"
  jq -r --arg aid "$attempt_id" '
    [
      .data[]
      | select(.metadata.correlation.attempt_id==$aid and .payload.type=="tool_call_requested" and .payload.tool_name=="write_file")
      | (.payload.request.payload? | fromjson?) as $req
      | select(($req.input.path? // "") | test("\\.rs$"))
      | ($req.input.content? // "") as $content
      | select(
          ($content | test("<new content>|\\(source code\\)"))
          or (($content | gsub("[[:space:]]"; "") | length) < 40)
        )
      | (($req.input.path // "") + ":" + (($content | split("\n") | .[0]) // ""))
    ]
    | join(" || ")
  ' "$EVENTS_PATH"
}

attempt_has_focused_cargo_tests() {
  local attempt_id="$1"
  jq -e --arg aid "$attempt_id" '
    def cmd_text:
      (.payload.request.payload? | fromjson? | .input) as $input
      | (($input.command? // $input.cmd? // empty)
        + (if (($input.args? // []) | length) > 0 then " " + (($input.args // []) | join(" ")) else "" end));
    [
      .data[]
      | select(.metadata.correlation.attempt_id==$aid and .payload.type=="tool_call_requested" and (.payload.tool_name=="run_command" or .payload.tool_name=="exec_command"))
      | cmd_text
    ]
    | any(test("cargo test .*native::|cargo test .*native_summary|cargo test .*task_runtime_event|cargo test .*prompt_assembly|cargo test .*runtime_event_projection"))
  ' "$EVENTS_PATH" >/dev/null
}

append_attempt_report() {
  local task_label="$1" attempt_id="$2" attempt_file="$3"
  local created modified deleted command_summary read_paths write_paths tool_failures suspicious_writes
  created="$(jq -r '(.files_created // []) | join(", ")' "$attempt_file")"
  modified="$(jq -r '(.files_modified // []) | join(", ")' "$attempt_file")"
  deleted="$(jq -r '(.files_deleted // []) | join(", ")' "$attempt_file")"
  command_summary="$(attempt_commands_summary "$attempt_id")"
  read_paths="$(attempt_read_paths "$attempt_id")"
  write_paths="$(attempt_write_paths "$attempt_id")"
  tool_failures="$(attempt_tool_failures "$attempt_id")"
  suspicious_writes="$(attempt_suspicious_writes "$attempt_id")"
  printf -v ATTEMPT_REPORT '%s\n### %s (%s)\n- Created files: %s\n- Modified files: %s\n- Deleted files: %s\n- Command results: %s\n- Read paths: %s\n- Write paths: %s\n- Suspicious writes: %s\n- Tool failures: %s\n' \
    "$ATTEMPT_REPORT" "$task_label" "$attempt_id" "${created:-none}" "${modified:-none}" "${deleted:-none}" "${command_summary:-none}" "${read_paths:-none}" "${write_paths:-none}" "${suspicious_writes:-none}" "${tool_failures:-none}"
}

validate_attempt_quality() {
  local task_id="$1" task_label="$2" required_regex="$3" forbidden_regex="$4" require_modified="$5" require_focused_tests="$6" minimum_required_matches="$7"
  local attempt_id attempt_file modified_count matched_count suspicious_writes
  attempt_id="$(jq -r --arg tid "$task_id" '.data[] | select(.task_id==$tid) | .attempt_id' "$ATTEMPTS_PATH" | head -n 1)"
  if [[ -z "$attempt_id" ]]; then
    append_quality_failure "$task_label: missing attempt metadata"
    return 0
  fi
  attempt_file="$(attempt_report_path "$attempt_id")"
  append_attempt_report "$task_label" "$attempt_id" "$attempt_file"
  if [[ -n "$required_regex" ]] && ! attempt_touches_regex "$attempt_file" "$required_regex"; then
    append_quality_failure "$task_label: did not touch required path set"
  fi
  if [[ -n "$required_regex" && -n "$minimum_required_matches" ]]; then
    matched_count="$(attempt_match_count "$attempt_file" "$required_regex")"
    if (( matched_count < minimum_required_matches )); then
      append_quality_failure "$task_label: touched only ${matched_count}/${minimum_required_matches} required paths"
    fi
  fi
  modified_count="$(jq -r '(.files_modified // []) | length' "$attempt_file")"
  if [[ "$require_modified" == "yes" && "$modified_count" == "0" ]]; then
    append_quality_failure "$task_label: modified no existing files"
  fi
  if [[ -n "$forbidden_regex" ]] && attempt_touches_regex "$attempt_file" "$forbidden_regex"; then
    append_quality_failure "$task_label: touched forbidden or suspicious paths"
  fi
  if [[ "$(attempt_nonzero_command_count "$attempt_id")" != "0" ]]; then
    append_quality_failure "$task_label: observed non-zero command exits"
  fi
  if [[ "$require_focused_tests" == "yes" ]] && ! attempt_has_focused_cargo_tests "$attempt_id"; then
    append_quality_failure "$task_label: did not run focused cargo tests"
  fi
  suspicious_writes="$(attempt_suspicious_writes "$attempt_id")"
  if [[ -n "$suspicious_writes" ]]; then
    append_quality_failure "$task_label: suspicious placeholder or tiny source write detected"
  fi
}

complete_active_checkpoint_if_needed() {
  local pending checkpoint_json attempt_id checkpoint_id
  checkpoint_json="$($HIVEMIND -f json events list --flow "$FLOW_ID" --limit 10000 | jq -cr '
    [.data[] | select(.payload.type=="checkpoint_activated") | {attempt_id:(.payload.attempt_id|tostring), checkpoint_id:.payload.checkpoint_id}] as $active
    | [.data[] | select(.payload.type=="checkpoint_completed") | {attempt_id:(.payload.attempt_id|tostring), checkpoint_id:.payload.checkpoint_id}] as $completed
    | ($active | map(select(($completed | index(.)) | not)) | last) // empty
  ' )"
  [[ -n "$checkpoint_json" ]] || return 0
  attempt_id="$(printf '%s' "$checkpoint_json" | jq -r '.attempt_id // empty')"
  checkpoint_id="$(printf '%s' "$checkpoint_json" | jq -r '.checkpoint_id // empty')"
  [[ -n "$attempt_id" && -n "$checkpoint_id" ]] || return 0
  echo "auto-completing checkpoint attempt=$attempt_id checkpoint=$checkpoint_id"
  "$HIVEMIND" checkpoint complete --attempt-id "$attempt_id" --id "$checkpoint_id" --summary "deep prompt provenance checkpoint" >/dev/null
}

snapshot_flow() {
  local tick="$1" status_path events_path native_path context_path request_snapshot_path
  status_path="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-flow-status-tick-${tick}.json"
  events_path="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-events-tick-${tick}.json"
  native_path="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-native-summary-tick-${tick}.json"
  context_path="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-native-context-tick-${tick}.json"
  request_snapshot_path="$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-request-snapshots-tick-${tick}"
  "$HIVEMIND" -f json flow status "$FLOW_ID" > "$status_path"
  "$HIVEMIND" -f json events list --flow "$FLOW_ID" --limit 10000 > "$events_path"
  "$HIVEMIND" -f json events native-summary --flow "$FLOW_ID" > "$native_path"
  "$HIVEMIND" -f json events native-summary --flow "$FLOW_ID" --include-reconstructed-context > "$context_path"
  rm -rf "$request_snapshot_path"
  mkdir -p "$request_snapshot_path"
  if [[ -d "$TMP_BASE/native-state/request-snapshots" ]]; then
    cp -a "$TMP_BASE/native-state/request-snapshots/." "$request_snapshot_path/"
  fi
  local flow_state task_states event_count graph_count read_count write_count compactions invocation_count latest_prompt_bytes
  flow_state="$(jq -r '.data.state // .state // empty' "$status_path")"
  task_states="$(jq -r '.data.task_executions | to_entries | map("\(.key)=\(.value.state|ascii_downcase)#\(.value.attempt_count)") | join(",")' "$status_path")"
  event_count="$(jq -r '.data | length' "$events_path")"
  graph_count="$(jq -r '[.data[] | select(.payload.type=="graph_query_executed")] | length' "$events_path")"
  read_count="$(jq -r '[.data[] | select(.payload.type=="tool_call_completed" and .payload.tool_name=="read_file")] | length' "$events_path")"
  write_count="$(jq -r '[.data[] | select(.payload.type=="tool_call_completed" and .payload.tool_name=="write_file")] | length' "$events_path")"
  compactions="$(jq -r '[.data[] | select(.payload.type=="native_history_compaction_recorded")] | length' "$events_path")"
  invocation_count="$(jq -r '.data.invocation_count // .invocation_count // 0' "$native_path")"
  latest_prompt_bytes="$(jq -r '(.data.invocations // .invocations // []) | map(.turns[-1].rendered_prompt_bytes // 0) | max // 0' "$native_path")"
  jq -nc \
    --argjson tick "$tick" \
    --arg flow_state "$flow_state" \
    --arg task_states "$task_states" \
    --argjson event_count "$event_count" \
    --argjson graph_count "$graph_count" \
    --argjson read_count "$read_count" \
    --argjson write_count "$write_count" \
    --argjson compactions "$compactions" \
    --argjson invocation_count "$invocation_count" \
    --argjson latest_prompt_bytes "$latest_prompt_bytes" \
    '{tick:$tick,flow_state:$flow_state,task_states:$task_states,event_count:$event_count,graph_query_count:$graph_count,read_file_count:$read_count,write_file_count:$write_count,compaction_count:$compactions,invocation_count:$invocation_count,latest_prompt_bytes:$latest_prompt_bytes}' >> "$TRACE_PATH"
  echo "tick=$tick state=$flow_state tasks=$task_states events=$event_count graph=$graph_count read=$read_count write=$write_count compactions=$compactions invocations=$invocation_count latest_prompt_bytes=$latest_prompt_bytes"
}

complete_checkpoint_if_needed() {
  local tick_output="$1"
  [[ "$tick_output" == *"checkpoints_incomplete"* ]] || return 0
  local attempt_id
  attempt_id="$($HIVEMIND -f json attempt list --flow "$FLOW_ID" --limit 1 | jq -r '.data[0].attempt_id // empty')"
  [[ -n "$attempt_id" ]] || return 1
  run "$HIVEMIND" checkpoint complete --attempt-id "$attempt_id" --id checkpoint-1 --summary "deep prompt provenance checkpoint"
}

run_flow_until_terminal() {
  local tick=0
  while (( tick < 180 )); do
    tick=$((tick + 1))
    snapshot_flow "$tick"
    local status_json state has_failed tick_out tick_rc
    status_json="$($HIVEMIND -f json flow status "$FLOW_ID")"
    state="$(printf '%s' "$status_json" | jq -r '.data.state' | tr '[:upper:]' '[:lower:]')"
    has_failed="$(printf '%s' "$status_json" | jq -r '[.data.task_executions[] | select(.state=="failed")] | length')"
    [[ "$state" == "completed" || "$state" == "merged" ]] && return 0
    if [[ "$state" == "aborted" ]]; then
      echo "flow entered aborted state" >&2
      return 2
    fi
    if (( has_failed > 0 )); then
      echo "detected failed task execution; aborting flow" >&2
      "$HIVEMIND" flow abort "$FLOW_ID" --reason "deep harness detected failed task execution" >/dev/null 2>&1 || true
      return 2
    fi
    complete_active_checkpoint_if_needed || true
    set +e
    tick_out="$($HIVEMIND flow tick "$FLOW_ID" 2>&1)"
    tick_rc=$?
    set -e
    [[ -n "$tick_out" ]] && echo "$tick_out"
    complete_checkpoint_if_needed "$tick_out" || true
    if (( tick_rc != 0 )) && [[ "$tick_out" != *"checkpoints_incomplete"* ]]; then
      echo "flow tick failed rc=$tick_rc" >&2
      echo "$tick_out" >&2
      return 2
    fi
    if (( tick_rc != 0 )); then
      sleep 1
    else
      sleep 5
    fi
  done
  echo "flow did not reach terminal state" >&2
  return 1
}

assert_initial_task_order() {
  local status_json ready_tasks
  status_json="$($HIVEMIND -f json flow status "$FLOW_ID")"
  ready_tasks="$(printf '%s' "$status_json" | jq -r '.data.task_executions | to_entries | map(select(.value.state=="ready")) | map(.key) | join(",")')"
  if [[ "$ready_tasks" != "$TASK_A" ]]; then
    echo "unexpected initial ready task set: $ready_tasks (expected $TASK_A)" >&2
    return 1
  fi
}

echo "=== Deep Prompt Provenance Flow ==="
echo "Using model: $MODEL_ID"
echo "Using hivemind binary: $HIVEMIND"
echo "Using graph query python: $GRAPH_QUERY_PYTHON_BIN"
TMP_BASE="/tmp/hm-${RUN_KEY}"
export HOME="$TMP_BASE/home"
rm -rf "$TMP_BASE"
mkdir -p "$HOME"

PROJECT_NAME="$RUN_KEY"
PROJECT_JSON="$(capture "$HIVEMIND" -f json project create "$PROJECT_NAME" --description 'Deep prompt provenance feature flow')"
PROJECT_ID="$(printf '%s' "$PROJECT_JSON" | json_id)"
run "$HIVEMIND" project attach-repo "$PROJECT_NAME" "$REPO_ROOT" --name repo
run "$HIVEMIND" project governance init "$PROJECT_NAME"
run "$HIVEMIND" graph snapshot refresh "$PROJECT_NAME"
run "$HIVEMIND" constitution init "$PROJECT_NAME" --content $'version: 1\nschema_version: constitution.v1\ncompatibility:\n  minimum_hivemind_version: 0.1.0\n  governance_schema_version: governance.v1\npartitions:\n  - id: core\n    path: src\nrules: []' --confirm
echo
echo "+ $HIVEMIND project runtime-set $PROJECT_NAME --role worker --adapter native --binary-path builtin-native --model $MODEL_ID --timeout-ms 900000 --env HIVEMIND_RUNTIME_ENV_INHERIT=all --env HIVEMIND_NATIVE_PROVIDER=$PROVIDER_NAME --env HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS=true --env HIVEMIND_GRAPH_QUERY_PYTHON=$GRAPH_QUERY_PYTHON_BIN --env HIVEMIND_NATIVE_MAX_TURNS=$NATIVE_MAX_TURNS --env HIVEMIND_NATIVE_SANDBOX_MODE=workspace-write --env HIVEMIND_NATIVE_APPROVAL_MODE=never --env HIVEMIND_NATIVE_NETWORK_PROXY_MODE=off --env HIVEMIND_NATIVE_NETWORK_MODE=full --env HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST=python3,python,bash,git,cargo,rg,sed,jq --env HIVEMIND_NATIVE_STATE_DIR=$TMP_BASE/native-state --env ${PROVIDER_NAME^^}_API_KEY=[REDACTED]"
PROJECT_RUNTIME_CMD=(
  "$HIVEMIND" project runtime-set "$PROJECT_NAME"
  --role worker
  --adapter native
  --binary-path builtin-native
  --model "$MODEL_ID"
  --timeout-ms 900000
  --env "HIVEMIND_RUNTIME_ENV_INHERIT=all"
  --env "HIVEMIND_NATIVE_PROVIDER=$PROVIDER_NAME"
  --env "HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS=true"
  --env "HIVEMIND_GRAPH_QUERY_PYTHON=$GRAPH_QUERY_PYTHON_BIN"
  --env "HIVEMIND_NATIVE_MAX_TURNS=$NATIVE_MAX_TURNS"
  --env "HIVEMIND_NATIVE_SANDBOX_MODE=workspace-write"
  --env "HIVEMIND_NATIVE_APPROVAL_MODE=never"
  --env "HIVEMIND_NATIVE_NETWORK_PROXY_MODE=off"
  --env "HIVEMIND_NATIVE_NETWORK_MODE=full"
  --env "HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST=python3,python,bash,git,cargo,rg,sed,jq"
  --env "HIVEMIND_NATIVE_STATE_DIR=$TMP_BASE/native-state"
)
if [[ -d "$HOST_CARGO_HOME" ]]; then
  PROJECT_RUNTIME_CMD+=(--env "CARGO_HOME=$HOST_CARGO_HOME")
fi
if [[ -d "$HOST_RUSTUP_HOME" ]]; then
  PROJECT_RUNTIME_CMD+=(--env "RUSTUP_HOME=$HOST_RUSTUP_HOME")
fi
if [[ "$PROVIDER_NAME" == "openrouter" ]]; then
  PROJECT_RUNTIME_CMD+=(--env "OPENROUTER_API_KEY=$OPENROUTER_API_KEY")
elif [[ "$PROVIDER_NAME" == "groq" ]]; then
  PROJECT_RUNTIME_CMD+=(--env "GROQ_API_KEY=$GROQ_API_KEY")
elif [[ "$PROVIDER_NAME" == "minimax" ]]; then
  PROJECT_RUNTIME_CMD+=(--env "MINIMAX_API_KEY=$MINIMAX_API_KEY")
fi
"${PROJECT_RUNTIME_CMD[@]}"

TASK_A_DESC=$(cat <<'EOF'
Implement bounded per-turn prompt provenance telemetry for native runtime execution.
Goal: explain exactly which runtime-local prompt inputs were visible to the model without persisting full prompt bodies.
Touch the deep runtime path: src/native/prompt_assembly.rs, src/native/turn_items.rs, src/adapters/runtime/telemetry.rs, src/core/events/payload/fragments/runtime_native_unknown.rs, and the native event projection path under src/core/registry/runtime/native_events/.
You must modify at least 3 existing files from that path set. Do not satisfy this task by only creating new standalone files.
If you create a new file, integrate it through existing module declarations and modify the existing call sites in the same attempt.
Capture bounded, summary-safe provenance for visible file/code windows, visible graph/code-navigation items, and compacted or suppressed history categories.
Respect payload limits and avoid dumping full file contents or raw prompt text into new telemetry.
Add focused unit coverage for the new provenance structure and event emission.
Use run_command or exec_command to run focused cargo tests before finishing; the harness checks for those command events explicitly.
Run the smallest relevant cargo tests before finishing, and do not claim success if any cargo command exits non-zero.
EOF
)
TASK_B_DESC=$(cat <<'EOF'
Expose the new prompt provenance telemetry in human inspection surfaces.
Extend the native event inspection path so operators can understand per-turn context composition from CLI output, especially events native-summary and any closely related attempt/event inspection code that already surfaces prompt hashes and context hashes.
Modify existing inspection/runtime files under src/cli/, src/core/runtime_event_projection/, or src/core/registry/runtime/native_events/.
Prefer direct read_file/list_files inspection of src/cli/commands/task_runtime_event.rs, src/core/runtime_event_projection/, and src/core/registry/runtime/native_events/. Avoid graph_query for this task unless it is strictly necessary.
Do not satisfy this task by only writing standalone markdown reports or disconnected tests.
Make the output concise but actionable: which file/code-window paths were visible, which graph/code-navigation entries were visible, and what was compacted or suppressed.
Keep the representation bounded and explainable.
Use run_command or exec_command to run focused cargo tests before finishing; the harness checks for those command events explicitly.
Add focused tests for the inspection/reporting layer and run them before finishing, and do not claim success if command exits are non-zero.
EOF
)
TASK_C_DESC=$(cat <<'EOF'
Finalize prompt provenance support with regression coverage and operator-facing documentation.
Add or update tests across the native runtime and CLI inspection layers so the new telemetry is validated end to end.
If a small docs note is appropriate, add it near the event or runtime observability documentation.
Run focused cargo tests covering the touched native prompt assembly and native-summary inspection code, not a generic bare cargo test.
Use run_command or exec_command for those tests so the command results are captured.
Before finishing, summarize exactly what changed and which commands passed with exit code 0. Do not claim success if any cargo command exits non-zero.
EOF
)

TASK_A="$(capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Add prompt provenance trace model" --description "$TASK_A_DESC" | json_id)"
TASK_B="$(capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Surface prompt provenance in inspection" --description "$TASK_B_DESC" | json_id)"
TASK_C="$(capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Validate and document prompt provenance" --description "$TASK_C_DESC" | json_id)"
GRAPH_ID="$(capture "$HIVEMIND" -f json graph create "$PROJECT_NAME" "$RUN_KEY" --from-tasks "$TASK_A" "$TASK_B" "$TASK_C" | jq -r '.data.graph_id // .graph_id // empty')"
run "$HIVEMIND" graph add-dependency "$GRAPH_ID" "$TASK_A" "$TASK_B"
run "$HIVEMIND" graph add-dependency "$GRAPH_ID" "$TASK_B" "$TASK_C"
run "$HIVEMIND" task set-run-mode "$TASK_A" auto
run "$HIVEMIND" task set-run-mode "$TASK_B" auto
run "$HIVEMIND" task set-run-mode "$TASK_C" auto
run "$HIVEMIND" graph add-check "$GRAPH_ID" "$TASK_C" --name prompt-provenance-focused-tests --command "cargo test native::tests:: -- --nocapture && cargo test cli::handlers::events::native_summary::tests:: -- --nocapture" --required
FLOW_ID="$(capture "$HIVEMIND" -f json flow create "$GRAPH_ID" --name "$RUN_KEY" | jq -r '.data.flow_id // .flow_id // empty')"
run "$HIVEMIND" flow set-run-mode "$FLOW_ID" manual
run "$HIVEMIND" flow start "$FLOW_ID"
assert_initial_task_order
set +e
run_flow_until_terminal
FLOW_RUN_RC=$?
set -e

capture_file "$FLOW_STATUS_PATH" "$HIVEMIND" -f json flow status "$FLOW_ID"
capture_file "$EVENTS_PATH" "$HIVEMIND" -f json events list --flow "$FLOW_ID" --limit 10000
capture_file "$NATIVE_SUMMARY_PATH" "$HIVEMIND" -f json events native-summary --flow "$FLOW_ID"
capture_file "$NATIVE_CONTEXT_PATH" "$HIVEMIND" -f json events native-summary --flow "$FLOW_ID" --include-reconstructed-context
rm -rf "$REQUEST_SNAPSHOTS_DIR"
mkdir -p "$REQUEST_SNAPSHOTS_DIR"
if [[ -d "$TMP_BASE/native-state/request-snapshots" ]]; then
  cp -a "$TMP_BASE/native-state/request-snapshots/." "$REQUEST_SNAPSHOTS_DIR/"
fi
capture_file "$ATTEMPTS_PATH" "$HIVEMIND" -f json attempt list --flow "$FLOW_ID" --limit 100
mapfile -t ATTEMPT_IDS < <(jq -r '.data[].attempt_id' "$ATTEMPTS_PATH")
for attempt_id in "${ATTEMPT_IDS[@]}"; do
  capture_file_quiet "$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-attempt-${attempt_id}.json" "$HIVEMIND" -f json attempt inspect "$attempt_id" --context --diff --output
done
for task_id in "$TASK_A" "$TASK_B" "$TASK_C"; do
  capture_file_quiet "$REPORT_DIR/${DATE_TAG}-${RUN_KEY}-worktree-${task_id}.json" "$HIVEMIND" -f json worktree inspect "$task_id" || true
done

FLOW_STATE="$(jq -r '.data.state' "$FLOW_STATUS_PATH")"
TASK_STATES="$(jq -r '.data.task_executions | to_entries | map("\(.key)=\(.value.state|ascii_downcase)#\(.value.attempt_count)") | join(", ")' "$FLOW_STATUS_PATH")"
INVOCATION_COUNT="$(jq -r '.data.invocation_count // .invocation_count // 0' "$NATIVE_SUMMARY_PATH")"
TURN_SUMMARY_COUNT="$(jq -r '(.data.invocations // .invocations // []) | map(.turns | length) | add // 0' "$NATIVE_SUMMARY_PATH")"
COMPACTION_COUNT="$(jq -r '[.data[] | select(.payload.type=="native_history_compaction_recorded")] | length' "$EVENTS_PATH")"
GRAPH_QUERY_COUNT="$(jq -r '[.data[] | select(.payload.type=="graph_query_executed")] | length' "$EVENTS_PATH")"
READ_FILE_COUNT="$(jq -r '[.data[] | select(.payload.type=="tool_call_completed" and .payload.tool_name=="read_file")] | length' "$EVENTS_PATH")"
WRITE_FILE_COUNT="$(jq -r '[.data[] | select(.payload.type=="tool_call_completed" and .payload.tool_name=="write_file")] | length' "$EVENTS_PATH")"
COMMAND_COUNT="$(jq -r '[.data[] | select(.payload.type=="runtime_command_observed")] | length' "$EVENTS_PATH")"
FILES_OBSERVED_COUNT="$(jq -r '[.data[] | select(.payload.type=="runtime_filesystem_observed")] | length' "$EVENTS_PATH")"
LATEST_PROMPT_BYTES="$(jq -r '(.data.invocations // .invocations // []) | map(.max_rendered_prompt_bytes // 0) | max // 0' "$NATIVE_SUMMARY_PATH")"
DISTINCT_CONTEXT_HASHES="$(jq -r '(.data.invocations // .invocations // []) | map(.turns[]?.delivered_context_hash // empty) | unique | length' "$NATIVE_SUMMARY_PATH")"
READ_PATHS="$(jq -r '[.data[] | select(.payload.type=="tool_call_requested" and .payload.tool_name=="read_file") | .payload.request.payload? | fromjson? | .input.path?] | map(select(. != null)) | unique | join(", ")' "$EVENTS_PATH")"
WRITE_PATHS="$(jq -r '[.data[] | select(.payload.type=="tool_call_requested" and .payload.tool_name=="write_file") | .payload.request.payload? | fromjson? | .input.path?] | map(select(. != null)) | unique | join(", ")' "$EVENTS_PATH")"
LAST_RUNTIME_ERROR_CODE="$(jq -r '[.data[] | select(.payload.type=="runtime_error_classified")] | last | .payload.code // empty' "$EVENTS_PATH")"
LAST_RUNTIME_ERROR_MESSAGE="$(jq -r '[.data[] | select(.payload.type=="runtime_error_classified")] | last | .payload.message // empty' "$EVENTS_PATH")"
QUALITY_FAILURES=()
ATTEMPT_REPORT=""

validate_attempt_quality "$TASK_A" "Task A" '^(src/native/prompt_assembly\.rs|src/native/turn_items\.rs|src/adapters/runtime/telemetry\.rs|src/core/events/payload/fragments/runtime_native_unknown\.rs|src/core/registry/runtime/native_events/)' '(^src/core/events/payload/fragments/[^/]*test\.rs$|^reports/)' yes yes 3
validate_attempt_quality "$TASK_B" "Task B" '^(src/cli/|src/core/runtime_event_projection/|src/core/registry/runtime/native_events/|src/core/events/payload/fragments/runtime_native_unknown\.rs)' '(^reports/|\.md$)' yes yes 1
validate_attempt_quality "$TASK_C" "Task C" '^(docs/|src/|tests/)' '' no yes 0

QUALITY_STATUS="passed"
if (( ${#QUALITY_FAILURES[@]} > 0 )); then
  QUALITY_STATUS="failed"
  printf '%s\n' "${QUALITY_FAILURES[@]}" > "$QUALITY_PATH"
  if [[ "$FLOW_RUN_RC" == "0" ]]; then
    FLOW_RUN_RC=1
  fi
else
  printf '%s\n' "quality gate passed" > "$QUALITY_PATH"
fi

cat > "$REPORT_PATH" <<EOF
# Deep Prompt Provenance Flow Report

- Project: ${PROJECT_NAME} (${PROJECT_ID})
- Flow: ${FLOW_ID}
- Model: ${MODEL_ID}
- Flow run rc: ${FLOW_RUN_RC}
- Flow state: ${FLOW_STATE}
- Task states: ${TASK_STATES}
- Invocation count: ${INVOCATION_COUNT}
- Turn summaries captured: ${TURN_SUMMARY_COUNT}
- Graph queries observed: ${GRAPH_QUERY_COUNT}
- read_file calls observed: ${READ_FILE_COUNT}
- write_file calls observed: ${WRITE_FILE_COUNT}
- Runtime commands observed: ${COMMAND_COUNT}
- Filesystem observation events: ${FILES_OBSERVED_COUNT}
- History compactions observed: ${COMPACTION_COUNT}
- Max rendered prompt bytes: ${LATEST_PROMPT_BYTES}
- Distinct delivered context hashes: ${DISTINCT_CONTEXT_HASHES}
- Last runtime error code: ${LAST_RUNTIME_ERROR_CODE}
- Last runtime error message: ${LAST_RUNTIME_ERROR_MESSAGE}
- Quality gate: ${QUALITY_STATUS}
- Quality gate file: $(basename "$QUALITY_PATH")
- Read paths: ${READ_PATHS}
- Write paths: ${WRITE_PATHS}
- Tick trace: $(basename "$TRACE_PATH")
- Events: $(basename "$EVENTS_PATH")
- Native summary: $(basename "$NATIVE_SUMMARY_PATH")
- Native reconstructed context: $(basename "$NATIVE_CONTEXT_PATH")
- Live request snapshots dir: $(basename "$REQUEST_SNAPSHOTS_DIR")
- Flow status: $(basename "$FLOW_STATUS_PATH")
- Attempts: $(basename "$ATTEMPTS_PATH")

## Quality gate failures

$(if (( ${#QUALITY_FAILURES[@]} > 0 )); then printf '%s\n' "${QUALITY_FAILURES[@]}" | sed 's/^/- /'; else echo '- none'; fi)

## Per-attempt summary

${ATTEMPT_REPORT}
EOF

echo "Report written to $REPORT_PATH"
exit "$FLOW_RUN_RC"