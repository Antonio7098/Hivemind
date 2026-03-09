#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/release/hivemind}"
CODEX_BIN="${CODEX_BIN:-$(command -v codex || true)}"
CODEX_MODEL="${CODEX_MODEL:-gpt-5.3-codex}"

if [ ! -x "$HIVEMIND" ]; then
  echo "hivemind binary not found or not executable: $HIVEMIND" >&2
  exit 1
fi

if [ -z "$CODEX_BIN" ] || [ ! -x "$CODEX_BIN" ]; then
  echo "codex binary not found or not executable: ${CODEX_BIN:-<empty>}" >&2
  exit 1
fi

TMP_BASE="${TMPDIR:-/tmp}/hivemind-test"
export HIVEMIND_DATA_DIR="$TMP_BASE/.hm_data_real_restore_turn_codex"
rm -rf "$HIVEMIND_DATA_DIR"

rm -rf "$TMP_BASE/test-real-runtime-restore-turn-codex"
mkdir -p "$TMP_BASE/test-real-runtime-restore-turn-codex"
cd "$TMP_BASE/test-real-runtime-restore-turn-codex"

git init >/dev/null
echo "# Real Runtime Restore Turn Test (Codex)" > README.md
git add README.md
git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit" >/dev/null

echo "=== Creating project ==="
"$HIVEMIND" project create "real-runtime-restore-turn-codex" --description "Real codex restore-turn validation" >/dev/null

echo "=== Attaching repository ==="
"$HIVEMIND" project attach-repo real-runtime-restore-turn-codex "$PWD" --name main >/dev/null

echo "=== Configuring real codex runtime ==="
"$HIVEMIND" project runtime-set real-runtime-restore-turn-codex \
  --adapter codex \
  --binary-path "$CODEX_BIN" \
  --model "$CODEX_MODEL" \
  --timeout-ms 240000 >/dev/null

PROMPT="Use apply_patch to create a file named restore-target.txt containing exactly snapshot and nothing else. Work only inside the current repository. Then reply with exactly this line and nothing else: restore turn ready."

echo "=== Creating task/graph/flow ==="
TASK_ID=$("$HIVEMIND" task create real-runtime-restore-turn-codex "$PROMPT" 2>&1 | awk '/ID:/ {print $2; exit}')
GRAPH_ID=$("$HIVEMIND" graph create real-runtime-restore-turn-codex "real-restore-turn-codex-graph" --from-tasks "$TASK_ID" 2>&1 | awk '/Graph ID:/ {print $3; exit}')
FLOW_ID=$("$HIVEMIND" flow create "$GRAPH_ID" 2>&1 | awk '/Flow ID:/ {print $3; exit}')

"$HIVEMIND" flow start "$FLOW_ID" >/dev/null

echo "=== Running flow tick ==="
set +e
TICK_OUT=$("$HIVEMIND" flow tick "$FLOW_ID" 2>&1)
TICK_RC=$?
set -e
echo "$TICK_OUT"

if [ "$TICK_RC" -ne 0 ] && [[ "$TICK_OUT" != *"checkpoints_incomplete"* ]]; then
  exit "$TICK_RC"
fi

EVENTS_JSON="$("$HIVEMIND" -f json events stream --flow "$FLOW_ID" --limit 1200)"
ATTEMPT_ID="$(printf '%s\n%s\n' "$TICK_OUT" "$EVENTS_JSON" | sed -n 's/.*attempt-id \([0-9a-f-]\{36\}\).*/\1/p; s/.*"attempt_id": "\([0-9a-f-]\{36\}\)".*/\1/p' | head -n1)"

if [ -z "$ATTEMPT_ID" ]; then
  echo "failed to discover attempt_id from tick output or events" >&2
  exit 1
fi

TURN_META="$(EVENTS_JSON="$EVENTS_JSON" ATTEMPT_ID="$ATTEMPT_ID" python3 - <<'PY'
import json
import os
import sys

data = json.loads(os.environ["EVENTS_JSON"])["data"]
attempt_id = os.environ["ATTEMPT_ID"]
turns = []
for event in data:
    payload = event.get("payload", {})
    if payload.get("type") != "runtime_turn_completed":
        continue
    if payload.get("attempt_id") != attempt_id:
        continue
    turns.append(payload)

if not turns:
    print("no runtime_turn_completed event for attempt", file=sys.stderr)
    sys.exit(1)

turns.sort(key=lambda payload: int(payload.get("ordinal", 0)))
turn = turns[-1]
if not turn.get("git_ref") and not turn.get("commit_sha"):
    print("turn event missing git ref and commit sha", file=sys.stderr)
    sys.exit(1)

print(f"{turn['ordinal']}\t{turn.get('git_ref', '')}\t{turn.get('commit_sha', '')}")
PY
)"
IFS=$'\t' read -r TURN_ORDINAL TURN_GIT_REF TURN_COMMIT_SHA <<< "$TURN_META"

echo "Using attempt $ATTEMPT_ID turn $TURN_ORDINAL"
echo "Turn ref: ${TURN_GIT_REF:--}"
echo "Turn commit: ${TURN_COMMIT_SHA:--}"

WORKTREE_JSON="$("$HIVEMIND" -f json worktree inspect "$TASK_ID")"
WORKTREE_PATH="$(WORKTREE_JSON="$WORKTREE_JSON" python3 - <<'PY'
import json
import os

print(json.loads(os.environ["WORKTREE_JSON"])["data"]["path"])
PY
)"

if [ ! -d "$WORKTREE_PATH" ]; then
  echo "worktree path missing: $WORKTREE_PATH" >&2
  exit 1
fi

if [ ! -f "$WORKTREE_PATH/restore-target.txt" ]; then
  echo "expected restore-target.txt in worktree before mutation" >&2
  exit 1
fi

if [ "$(cat "$WORKTREE_PATH/restore-target.txt")" != "snapshot" ]; then
  echo "unexpected initial restore-target.txt content" >&2
  cat "$WORKTREE_PATH/restore-target.txt" >&2
  exit 1
fi

HEAD_BEFORE_GIT="$(git -C "$WORKTREE_PATH" rev-parse HEAD)"

echo "=== Mutating worktree before restore ==="
printf 'mutated\n' > "$WORKTREE_PATH/restore-target.txt"
printf 'temporary noise\n' > "$WORKTREE_PATH/extra.txt"

echo "=== Restoring turn ref into active worktree ==="
RESTORE_JSON="$("$HIVEMIND" -f json worktree restore-turn "$ATTEMPT_ID" --ordinal "$TURN_ORDINAL" --confirm --force)"

RESTORE_CHECK="$(RESTORE_JSON="$RESTORE_JSON" WORKTREE_PATH="$WORKTREE_PATH" HEAD_BEFORE_GIT="$HEAD_BEFORE_GIT" python3 - <<'PY'
import json
import os
import sys

data = json.loads(os.environ["RESTORE_JSON"])
payload = data["data"]

if payload["worktree_path"] != os.environ["WORKTREE_PATH"]:
    print("restored worktree path mismatch", file=sys.stderr)
    sys.exit(1)
if payload["head_before"] != os.environ["HEAD_BEFORE_GIT"]:
    print("restore head_before mismatch", file=sys.stderr)
    sys.exit(1)
if payload["head_after"] != os.environ["HEAD_BEFORE_GIT"]:
    print("restore moved HEAD unexpectedly", file=sys.stderr)
    sys.exit(1)
if not payload["had_local_changes"]:
    print("restore should report local changes before restore", file=sys.stderr)
    sys.exit(1)

print("ok")
PY
)"

[ "$RESTORE_CHECK" = "ok" ]

if [ "$(cat "$WORKTREE_PATH/restore-target.txt")" != "snapshot" ]; then
  echo "restore-target.txt was not restored" >&2
  cat "$WORKTREE_PATH/restore-target.txt" >&2
  exit 1
fi

if [ -e "$WORKTREE_PATH/extra.txt" ]; then
  echo "extra.txt should have been removed by restore" >&2
  exit 1
fi

EVENTS_JSON="$("$HIVEMIND" -f json events stream --flow "$FLOW_ID" --limit 1200)"
[[ "$EVENTS_JSON" == *'"type": "worktree_turn_ref_restored"'* ]]

if [[ "$TICK_OUT" == *"checkpoints_incomplete"* || ( "$EVENTS_JSON" == *'"type": "checkpoint_requested"'* && "$EVENTS_JSON" != *'"type": "checkpoint_completed"'* ) ]]; then
  echo "=== Completing checkpoint after restore ==="
  "$HIVEMIND" checkpoint complete --attempt-id "$ATTEMPT_ID" --id checkpoint-1 --summary "restore turn codex smoke checkpoint" >/dev/null
  "$HIVEMIND" flow tick "$FLOW_ID" >/dev/null
fi

echo "=== Restore-turn event excerpt ==="
awk '/runtime_turn_completed|worktree_turn_ref_restored/ {print; c++; if (c>=8) exit}' <<<"$EVENTS_JSON"

echo "Real codex restore-turn smoke passed."