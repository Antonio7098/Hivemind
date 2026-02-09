#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/release/hivemind}"

TMP_BASE="${TMPDIR:-/tmp}/hivemind-test"

export HOME="$TMP_BASE/.hm_home"
rm -rf "$HOME"

rm -rf "$TMP_BASE/test-fresh"
mkdir -p "$TMP_BASE/test-fresh"
cd "$TMP_BASE/test-fresh"

git init
echo "# Test Project" > README.md
git add README.md
git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit"

echo "=== Creating project ==="
PROJECT_OUTPUT=$($HIVEMIND project create "test-fresh" --description "Test project" 2>&1)
echo "$PROJECT_OUTPUT"
PROJECT_ID=$(echo "$PROJECT_OUTPUT" | grep "ID:" | head -1 | awk '{print $2}')

echo "=== Attaching repository ==="
$HIVEMIND project attach-repo test-fresh "$PWD" --name "main" 2>&1

echo "=== Configuring runtime ==="
$HIVEMIND project runtime-set test-fresh \
  --adapter opencode \
  --binary-path /usr/bin/env \
  --arg sh \
  --arg -c \
  --arg "printf 'Hello, World!' > hello.txt" \
  --timeout-ms 1000 2>&1

echo "=== Creating task with better description ==="
TASK_OUTPUT=$($HIVEMIND task create test-fresh "Create hello world file" --description "Create a file called hello.txt with the content 'Hello, World!'" 2>&1)
echo "$TASK_OUTPUT"
TASK_ID=$(echo "$TASK_OUTPUT" | grep "ID:" | head -1 | awk '{print $2}')
echo "Task ID: $TASK_ID"

echo "=== Creating new graph ==="
GRAPH_OUTPUT=$($HIVEMIND graph create test-fresh "exec-graph" --from-tasks "$TASK_ID" 2>&1)
echo "$GRAPH_OUTPUT"
GRAPH_ID=$(echo "$GRAPH_OUTPUT" | grep "Graph ID:" | awk '{print $3}')
echo "Graph ID: $GRAPH_ID"

echo "=== Creating new flow ==="
FLOW_OUTPUT=$($HIVEMIND flow create "$GRAPH_ID" 2>&1)
echo "$FLOW_OUTPUT"
FLOW_ID=$(echo "$FLOW_OUTPUT" | grep "Flow ID:" | awk '{print $3}')
echo "Flow ID: $FLOW_ID"

echo "=== Starting flow ==="
$HIVEMIND flow start "$FLOW_ID" 2>&1

echo "=== Ticking flow ==="
$HIVEMIND flow tick "$FLOW_ID" 2>&1

echo "=== Checking events ==="
$HIVEMIND events list --project "$PROJECT_ID" --limit 50 2>&1

echo "=== Listing files in worktree ==="
ls -la .hivemind/worktrees/*/ 2>&1 || echo "No worktrees"

echo "=== Done ==="
