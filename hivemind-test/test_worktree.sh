#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -d "$SCRIPT_DIR/../hivemind" ]; then
  REPO_ROOT="$(cd "$SCRIPT_DIR/../hivemind" && pwd)"
else
  REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
fi
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/release/hivemind}"

TMP_BASE="${TMPDIR:-/tmp}/hivemind-test"

export HOME="$TMP_BASE/.hm_home"
rm -rf "$HOME"

echo "=== Creating fresh test ==="
rm -rf "$TMP_BASE/test-fresh"
mkdir -p "$TMP_BASE/test-fresh"
cd "$TMP_BASE/test-fresh"
git init

# Create initial commit
echo "# Test Project" > README.md
git add README.md
git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit"

echo "=== Creating project ==="
$HIVEMIND project create "test-fresh" --description "Test project" 2>&1

echo "=== Attaching repository ==="
$HIVEMIND project attach-repo test-fresh "$PWD" --name "main" 2>&1

echo "=== Creating task ==="
TASK_OUTPUT=$($HIVEMIND task create test-fresh "Test task" --description "A test task" 2>&1)
echo "$TASK_OUTPUT"
TASK_ID=$(echo "$TASK_OUTPUT" | grep "ID:" | head -1 | awk '{print $2}')
echo "Task ID: $TASK_ID"

echo "=== Creating graph ==="
GRAPH_OUTPUT=$($HIVEMIND graph create test-fresh "test-graph" --from-tasks "$TASK_ID" 2>&1)
echo "$GRAPH_OUTPUT"
GRAPH_ID=$(echo "$GRAPH_OUTPUT" | grep "Graph ID:" | awk '{print $3}')
echo "Graph ID: $GRAPH_ID"

echo "=== Creating flow ==="
FLOW_OUTPUT=$($HIVEMIND flow create "$GRAPH_ID" 2>&1)
echo "$FLOW_OUTPUT"
FLOW_ID=$(echo "$FLOW_OUTPUT" | grep "Flow ID:" | awk '{print $3}')
echo "Flow ID: $FLOW_ID"

echo "=== Checking worktrees before start ==="
$HIVEMIND worktree list "$FLOW_ID" 2>&1

echo "=== Starting flow ==="
$HIVEMIND flow start "$FLOW_ID" 2>&1

echo "=== Starting task (creates worktree) ==="
$HIVEMIND task start "$TASK_ID" 2>&1

echo "=== Checking worktrees after task start ==="
$HIVEMIND worktree list "$FLOW_ID" 2>&1

echo "=== Inspecting worktree ==="
$HIVEMIND worktree inspect "$TASK_ID" 2>&1

echo "=== Listing worktree directory ==="
ls -la "$HOME/hivemind/worktrees/" 2>&1 || echo "Directory does not exist"

echo "=== Test complete ==="
