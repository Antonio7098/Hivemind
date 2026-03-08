#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/release/hivemind}"
CODEX_BIN="${CODEX_BIN:-$(command -v codex)}"
CODEX_MODEL="${CODEX_MODEL:-gpt-5.3-codex}"

TMP_BASE="${TMPDIR:-/tmp}/hivemind-test"
export HIVEMIND_DATA_DIR="$TMP_BASE/.hm_data_real_projection_codex"
rm -rf "$HIVEMIND_DATA_DIR"

rm -rf "$TMP_BASE/test-real-runtime-projection-codex"
mkdir -p "$TMP_BASE/test-real-runtime-projection-codex"
cd "$TMP_BASE/test-real-runtime-projection-codex"

git init >/dev/null
echo "# Real Runtime Projection Test (Codex)" > README.md
git add README.md
git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit" >/dev/null

echo "=== Creating project ==="
$HIVEMIND project create "real-runtime-projection-codex" --description "Real codex runtime projection validation" >/dev/null

echo "=== Attaching repository ==="
$HIVEMIND project attach-repo real-runtime-projection-codex "$PWD" --name main >/dev/null

echo "=== Configuring real codex runtime ==="
$HIVEMIND project runtime-set real-runtime-projection-codex \
  --adapter codex \
  --binary-path "$CODEX_BIN" \
  --model "$CODEX_MODEL" \
  --timeout-ms 240000 >/dev/null

PROMPT="Use apply_patch to create a file named projection.txt containing exactly data. Also use a todo list with the item collect logs, run the pwd command once, then reply with exactly this line and nothing else: I will verify outputs now."

echo "=== Creating task/graph/flow ==="
TASK_ID=$($HIVEMIND task create real-runtime-projection-codex "$PROMPT" 2>&1 | awk '/ID:/ {print $2; exit}')
GRAPH_ID=$($HIVEMIND graph create real-runtime-projection-codex "real-projection-codex-graph" --from-tasks "$TASK_ID" 2>&1 | awk '/Graph ID:/ {print $3; exit}')
FLOW_ID=$($HIVEMIND flow create "$GRAPH_ID" 2>&1 | awk '/Flow ID:/ {print $3; exit}')

$HIVEMIND flow start "$FLOW_ID" >/dev/null

set +e
TICK_OUT=$($HIVEMIND flow tick "$FLOW_ID" 2>&1)
TICK_RC=$?
set -e
echo "$TICK_OUT"

if [ "$TICK_RC" -ne 0 ] && ! echo "$TICK_OUT" | grep -q "checkpoints_incomplete"; then
  exit "$TICK_RC"
fi

EVENTS_JSON="$($HIVEMIND -f json events stream --flow "$FLOW_ID" --limit 1200)"

if [[ "$TICK_OUT" == *"checkpoints_incomplete"* || ( "$EVENTS_JSON" == *'"type": "checkpoint_requested"'* && "$EVENTS_JSON" != *'"type": "checkpoint_completed"'* ) ]]; then
  ATTEMPT_ID="$(printf '%s\n%s\n' "$TICK_OUT" "$EVENTS_JSON" | sed -n 's/.*attempt-id \([0-9a-f-]\{36\}\).*/\1/p; s/.*"attempt_id": "\([0-9a-f-]\{36\}\)".*/\1/p' | head -n1)"
  if [ -z "$ATTEMPT_ID" ]; then
    echo "failed to discover attempt_id from events" >&2
    exit 1
  fi

  echo "=== Completing checkpoint ==="
  $HIVEMIND checkpoint complete --attempt-id "$ATTEMPT_ID" --id checkpoint-1 --summary "real codex runtime projection checkpoint" >/dev/null

  echo "=== Finalizing attempt after checkpoint completion ==="
  $HIVEMIND flow tick "$FLOW_ID" >/dev/null
fi

echo "=== Validating projected runtime events from real codex output ==="
EVENTS_JSON="$($HIVEMIND -f json events stream --flow "$FLOW_ID" --limit 1200)"

[[ "$EVENTS_JSON" == *"runtime_started"* ]]
[[ "$EVENTS_JSON" == *"runtime_output_chunk"* ]]
[[ "$EVENTS_JSON" == *"runtime_exited"* ]]
[[ "$EVENTS_JSON" == *"[codex.json]"* ]]
[[ "$EVENTS_JSON" == *"runtime_command_completed"* ]]
[[ "$EVENTS_JSON" == *"runtime_tool_call_observed"* ]]
[[ "$EVENTS_JSON" == *"runtime_todo_snapshot_updated"* ]]
[[ "$EVENTS_JSON" == *"runtime_narrative_output_observed"* ]]
[[ "$EVENTS_JSON" == *"File change: add"* ]]
[[ "$EVENTS_JSON" != *"runtime_command_observed"* ]]

EVENTS_JSON="$EVENTS_JSON" python3 - <<'PY'
import json
import os

data = json.loads(os.environ["EVENTS_JSON"])["data"]
observed = [
    event["payload"]
    for event in data
    if event["payload"]["type"] in {
        "runtime_command_completed",
        "runtime_tool_call_observed",
        "runtime_todo_snapshot_updated",
        "runtime_narrative_output_observed",
    }
]
assert observed, "expected projected runtime observations"
assert all(payload["stream"] == "stdout" for payload in observed), observed
PY

echo "=== Runtime projection event excerpt ==="
awk '/runtime_(command_completed|tool_call_observed|todo_snapshot_updated|narrative_output_observed)/ {print; c++; if (c>=8) exit}' <<<"$EVENTS_JSON"

echo "Real codex projection events found."