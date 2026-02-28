#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-/home/antonio/.cargo/shared-target/debug/hivemind}"
MODEL_ID="${OPENROUTER_MODEL_ID:-openrouter/openai/gpt-4o-mini}"
REPORT_DIR="$REPO_ROOT/hivemind-test/test-report"
DATE_TAG="$(date +%F)"
LOG_PATH="$REPORT_DIR/${DATE_TAG}-native-openrouter-beta-api.log"
REPORT_PATH="$REPORT_DIR/${DATE_TAG}-native-openrouter-beta-api-report.md"

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "OPENROUTER_API_KEY is required in environment." >&2
  exit 1
fi

mkdir -p "$REPORT_DIR"
exec > >(tee "$LOG_PATH") 2>&1

run() {
  echo
  echo "+ $*"
  "$@"
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "$haystack" != *"$needle"* ]]; then
    echo "ASSERTION FAILED: $label (missing '$needle')" >&2
    exit 1
  fi
}

lowercase() {
  tr '[:upper:]' '[:lower:]'
}

task_artifacts_ready() {
  local task_id="$1"
  local worktree_path
  worktree_path="$("$HIVEMIND" -f json worktree inspect "$task_id" | jq -r '.data.path // empty')"
  if [[ -z "$worktree_path" || ! -d "$worktree_path" ]]; then
    return 1
  fi

  if [[ -n "${TASK_A_ID:-}" && "$task_id" == "$TASK_A_ID" ]]; then
    [[ -f "$worktree_path/api_app/__init__.py" && -f "$worktree_path/api_app/store.py" && -f "$worktree_path/api_app/router.py" ]]
    return $?
  fi

  if [[ -n "${TASK_B_ID:-}" && "$task_id" == "$TASK_B_ID" ]]; then
    [[ -f "$worktree_path/api_app/server.py" && -f "$worktree_path/main.py" ]]
    return $?
  fi

  if [[ -n "${TASK_C_ID:-}" && "$task_id" == "$TASK_C_ID" ]]; then
    [[ -f "$worktree_path/tests/test_router.py" && -f "$worktree_path/README.md" ]]
    return $?
  fi

  if [[ -n "${TASK_D_ID:-}" && "$task_id" == "$TASK_D_ID" ]]; then
    [[ -f "$worktree_path/docs/verification.md" ]]
    return $?
  fi

  return 0
}

complete_checkpoint_if_needed() {
  local flow_id="$1"
  local tick_output="$2"
  if [[ "$tick_output" != *"checkpoints_incomplete"* ]]; then
    return 0
  fi

  local attempt_id
  attempt_id="$(printf '%s\n' "$tick_output" | sed -n 's/.*attempt-id \([0-9a-f-]\{36\}\).*/\1/p' | head -n1)"
  if [[ -z "$attempt_id" ]]; then
    attempt_id="$("$HIVEMIND" -f json attempt list --flow "$flow_id" --limit 1 | jq -r '.data[0].attempt_id')"
  fi
  [[ -n "$attempt_id" ]] || { echo "Unable to determine attempt_id for checkpoint completion" >&2; exit 1; }

  local running_task
  running_task="$("$HIVEMIND" -f json flow status "$flow_id" | jq -r '.data.task_executions | to_entries[]? | select(.value.state=="running") | .key' | head -n1)"

  run "$HIVEMIND" checkpoint complete --attempt-id "$attempt_id" --id checkpoint-1 --summary "beta-openrouter checkpoint"
  if [[ -n "$running_task" ]]; then
    if task_artifacts_ready "$running_task"; then
      run "$HIVEMIND" task complete "$running_task" || true
    else
      echo "Task $running_task missing expected artifacts after checkpoint; aborting task to force retry"
      run "$HIVEMIND" task abort "$running_task" --reason "missing expected artifacts after checkpoint" || true
    fi
  fi
}

retry_failed_tasks_if_any() {
  local flow_id="$1"
  local failed_tasks
  failed_tasks="$("$HIVEMIND" -f json flow status "$flow_id" | jq -r '.data.task_executions | to_entries[]? | select(.value.state=="failed") | .key')"
  if [[ -z "$failed_tasks" ]]; then
    return 0
  fi
  while IFS= read -r task_id; do
    [[ -n "$task_id" ]] || continue
    echo
    echo "+ $HIVEMIND task retry $task_id --mode clean"
    local retry_out
    local retry_rc=0
    set +e
    retry_out="$("$HIVEMIND" task retry "$task_id" --mode clean 2>&1)"
    retry_rc=$?
    set -e
    echo "$retry_out"
    if (( retry_rc != 0 )) && [[ "$retry_out" == *"retry_limit_exceeded"* ]]; then
      echo "Retry limit exceeded for task=$task_id; aborting flow=$flow_id to avoid infinite loop" >&2
      "$HIVEMIND" flow abort "$flow_id" || true
      return 2
    fi
  done <<< "$failed_tasks"
}

run_flow_until_terminal() {
  local flow_id="$1"
  local max_ticks="${2:-80}"
  local tick=0

  while (( tick < max_ticks )); do
    tick=$((tick + 1))
    local flow_json
    flow_json="$("$HIVEMIND" -f json flow status "$flow_id")"
    local state
    state="$(printf '%s' "$flow_json" | jq -r '.data.state' | lowercase)"
    echo "flow=$flow_id tick=$tick state=$state"

    if [[ "$state" == "completed" || "$state" == "merged" || "$state" == "aborted" ]]; then
      return 0
    fi

    local retry_rc=0
    set +e
    retry_failed_tasks_if_any "$flow_id"
    retry_rc=$?
    set -e
    if (( retry_rc == 2 )); then
      return 1
    fi

    local tick_out=""
    local tick_rc=0
    set +e
    tick_out="$("$HIVEMIND" flow tick "$flow_id" 2>&1)"
    tick_rc=$?
    set -e
    echo "$tick_out"

    if (( tick_rc != 0 )); then
      complete_checkpoint_if_needed "$flow_id" "$tick_out"
      set +e
      retry_failed_tasks_if_any "$flow_id"
      retry_rc=$?
      set -e
      if (( retry_rc == 2 )); then
        return 1
      fi
      sleep 1
    else
      complete_checkpoint_if_needed "$flow_id" "$tick_out"
      set +e
      retry_failed_tasks_if_any "$flow_id"
      retry_rc=$?
      set -e
      if (( retry_rc == 2 )); then
        return 1
      fi
    fi
  done

  echo "Flow $flow_id did not reach terminal state within $max_ticks ticks" >&2
  return 1
}

echo "=== Native OpenRouter Beta API Test ==="
echo "Using model: $MODEL_ID"
echo "Using hivemind binary: $HIVEMIND"

TMP_BASE="/tmp/hm-native-openrouter-beta-api"
export HOME="$TMP_BASE/home"
rm -rf "$TMP_BASE"
mkdir -p "$HOME" "$TMP_BASE/repo"
cd "$TMP_BASE/repo"

run git init
run git branch -m main
echo "# Beta API Project" > README.md
run git add README.md
run git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit"

PROJECT_NAME="beta-openrouter-api"
run "$HIVEMIND" project create "$PROJECT_NAME" --description "Native OpenRouter beta test project"
run "$HIVEMIND" project attach-repo "$PROJECT_NAME" "$PWD" --name main

echo "=== Governance setup ==="
run "$HIVEMIND" project governance init "$PROJECT_NAME"
run "$HIVEMIND" project governance document create "$PROJECT_NAME" architecture-notes --title "Architecture Notes" --owner "qa" --content "Layered modules: store, router, server, tests."
run "$HIVEMIND" project governance document create "$PROJECT_NAME" api-contract --title "API Contract" --owner "qa" --content "Routes: GET /health, GET /items, POST /items with JSON body."
run "$HIVEMIND" project governance document create "$PROJECT_NAME" testing-policy --title "Testing Policy" --owner "qa" --content "Use unittest; tests must pass before DONE."
run "$HIVEMIND" global system-prompt create beta-native-system --content "Execute checklist steps exactly in order. Emit exactly one THINK/ACT/DONE directive line per turn. Never emit DONE until every numbered ACT step in the task prompt has succeeded."
run "$HIVEMIND" global skill create beta-python-skill --name "Python API Builder" --content "Prefer small modules and explicit interfaces."
run "$HIVEMIND" global skill create beta-testing-skill --name "Testing Discipline" --content "Write tests and validate behavior before DONE."
run "$HIVEMIND" global template create beta-api-template --system-prompt-id beta-native-system --skill-id beta-python-skill --skill-id beta-testing-skill --document-id architecture-notes --document-id api-contract --document-id testing-policy
run "$HIVEMIND" global template instantiate "$PROJECT_NAME" beta-api-template

cat > /tmp/hm_beta_constitution.yaml <<'YAML'
version: 1
schema_version: constitution.v1
compatibility:
  minimum_hivemind_version: 0.1.32
  governance_schema_version: governance.v1
partitions: []
rules: []
YAML
run "$HIVEMIND" constitution init "$PROJECT_NAME" --from-file /tmp/hm_beta_constitution.yaml --confirm
run "$HIVEMIND" constitution validate "$PROJECT_NAME"
run "$HIVEMIND" constitution check --project "$PROJECT_NAME"
run "$HIVEMIND" graph snapshot refresh "$PROJECT_NAME"
run "$HIVEMIND" -f json project governance diagnose "$PROJECT_NAME"

echo "=== Runtime setup (native + OpenRouter) ==="
run "$HIVEMIND" project runtime-set "$PROJECT_NAME" \
  --role worker \
  --adapter native \
  --binary-path builtin-native \
  --model "$MODEL_ID" \
  --timeout-ms 180000 \
  --env HIVEMIND_NATIVE_PROVIDER=openrouter \
  --env HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS=true \
  --env HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST=python,python3

TASK_A_PROMPT=$(cat <<'EOF'
Build base API modules. Use only list_files and write_file; do not call run_command or git tools.
Emit exactly one directive per turn.
1) ACT:tool:list_files:{"path":".","recursive":false}
2) ACT:tool:write_file:{"path":"api_app/__init__.py","content":"\"\"\"API package.\"\"\"\n","append":false}
3) ACT:tool:write_file:{"path":"api_app/store.py","content":"\"\"\"In-memory item store.\"\"\"\n\nfrom typing import List\n\n_ITEMS: List[str] = [\"alpha\", \"beta\"]\n\n\ndef get_items() -> List[str]:\n    return list(_ITEMS)\n\n\ndef add_item(name: str) -> List[str]:\n    _ITEMS.append(name)\n    return list(_ITEMS)\n","append":false}
4) ACT:tool:write_file:{"path":"api_app/router.py","content":"\"\"\"Route handlers for the simple API.\"\"\"\n\nimport json\nfrom typing import Tuple\n\nfrom .store import add_item, get_items\n\nResponse = Tuple[int, dict]\n\n\ndef route_request(method: str, path: str, body: str) -> Response:\n    if method == \"GET\" and path == \"/health\":\n        return 200, {\"status\": \"ok\"}\n    if method == \"GET\" and path == \"/items\":\n        return 200, {\"items\": get_items()}\n    if method == \"POST\" and path == \"/items\":\n        payload = json.loads(body or \"{}\")\n        name = str(payload.get(\"name\", \"\")).strip()\n        if not name:\n            return 400, {\"error\": \"name is required\"}\n        return 201, {\"items\": add_item(name)}\n    return 404, {\"error\": \"not found\"}\n","append":false}
5) DONE:base api modules created
EOF
)

TASK_B_PROMPT=$(cat <<'EOF'
Build server wiring and entrypoint. Use write_file only; do not call run_command or git tools.
Emit exactly one directive per turn.
1) ACT:tool:write_file:{"path":"api_app/server.py","content":"\"\"\"HTTP server wiring for the simple API.\"\"\"\n\nimport json\nfrom http.server import BaseHTTPRequestHandler, HTTPServer\n\nfrom .router import route_request\n\n\nclass ApiHandler(BaseHTTPRequestHandler):\n    def _send(self, status: int, payload: dict) -> None:\n        data = json.dumps(payload).encode(\"utf-8\")\n        self.send_response(status)\n        self.send_header(\"Content-Type\", \"application/json\")\n        self.send_header(\"Content-Length\", str(len(data)))\n        self.end_headers()\n        self.wfile.write(data)\n\n    def do_GET(self) -> None:\n        status, payload = route_request(\"GET\", self.path, \"\")\n        self._send(status, payload)\n\n    def do_POST(self) -> None:\n        length = int(self.headers.get(\"Content-Length\", \"0\"))\n        raw = self.rfile.read(length).decode(\"utf-8\") if length else \"\"\n        status, payload = route_request(\"POST\", self.path, raw)\n        self._send(status, payload)\n\n\ndef run_server(host: str = \"127.0.0.1\", port: int = 8000) -> HTTPServer:\n    server = HTTPServer((host, port), ApiHandler)\n    return server\n","append":false}
2) ACT:tool:write_file:{"path":"main.py","content":"\"\"\"Application entrypoint.\"\"\"\n\nfrom api_app.server import run_server\n\n\nif __name__ == \"__main__\":\n    server = run_server()\n    print(\"Serving on http://127.0.0.1:8000\")\n    server.serve_forever()\n","append":false}
3) DONE:server and entrypoint created
EOF
)

TASK_C_PROMPT=$(cat <<'EOF'
Create tests/docs and validate via runtime tools. Emit exactly one directive per turn.
1) ACT:tool:write_file:{"path":"tests/test_router.py","content":"import unittest\n\nfrom api_app.router import route_request\n\n\nclass RouterTests(unittest.TestCase):\n    def test_health(self):\n        status, payload = route_request(\"GET\", \"/health\", \"\")\n        self.assertEqual(status, 200)\n        self.assertEqual(payload[\"status\"], \"ok\")\n\n    def test_items_list(self):\n        status, payload = route_request(\"GET\", \"/items\", \"\")\n        self.assertEqual(status, 200)\n        self.assertIn(\"items\", payload)\n\n    def test_add_item(self):\n        status, payload = route_request(\"POST\", \"/items\", '{\"name\":\"gamma\"}')\n        self.assertEqual(status, 201)\n        self.assertIn(\"gamma\", payload[\"items\"])\n\n\nif __name__ == \"__main__\":\n    unittest.main()\n","append":false}
2) ACT:tool:write_file:{"path":"README.md","content":"# Beta API Project\n\nSimple multi-file API built via Hivemind native OpenRouter runtime.\n\n## Endpoints\n- GET /health\n- GET /items\n- POST /items\n\n## Local test\npython3 -m unittest discover -s tests -v\n","append":false}
3) ACT:tool:run_command:{"command":"python3","args":["-m","unittest","discover","-s","tests","-v"]}
4) ACT:tool:git_status:{}
5) ACT:tool:git_diff:{"staged":false}
6) DONE:tests executed and git state inspected
EOF
)

TASK_D_PROMPT=$(cat <<'EOF'
Create a follow-up verification artifact in a second flow. Emit exactly one directive per turn.
1) ACT:tool:read_file:{"path":"README.md"}
2) ACT:tool:write_file:{"path":"docs/verification.md","content":"# Verification Notes\n\n- Runtime: native + OpenRouter\n- API modules: api_app/store.py, api_app/router.py, api_app/server.py\n- Tests: python3 -m unittest discover -s tests -v\n","append":false}
3) ACT:tool:run_command:{"command":"python3","args":["-m","unittest","discover","-s","tests","-v"]}
4) DONE:second flow verification artifact added
EOF
)

echo "=== Create tasks and graph/flow #1 ==="
TASK_A_ID="$("$HIVEMIND" task create "$PROJECT_NAME" "$TASK_A_PROMPT" 2>&1 | awk '/ID:/ {print $2; exit}')"
TASK_B_ID="$("$HIVEMIND" task create "$PROJECT_NAME" "$TASK_B_PROMPT" 2>&1 | awk '/ID:/ {print $2; exit}')"
TASK_C_ID="$("$HIVEMIND" task create "$PROJECT_NAME" "$TASK_C_PROMPT" 2>&1 | awk '/ID:/ {print $2; exit}')"
echo "TASK_A_ID=$TASK_A_ID"
echo "TASK_B_ID=$TASK_B_ID"
echo "TASK_C_ID=$TASK_C_ID"

run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$TASK_A_ID" api-contract
run "$HIVEMIND" project governance attachment exclude "$PROJECT_NAME" "$TASK_A_ID" testing-policy
run "$HIVEMIND" project governance attachment exclude "$PROJECT_NAME" "$TASK_B_ID" testing-policy
run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$TASK_C_ID" testing-policy

GRAPH1_ID="$("$HIVEMIND" graph create "$PROJECT_NAME" beta-api-graph-1 --from-tasks "$TASK_A_ID" "$TASK_B_ID" "$TASK_C_ID" 2>&1 | awk '/Graph ID:/ {print $3; exit}')"
echo "GRAPH1_ID=$GRAPH1_ID"
run "$HIVEMIND" graph add-dependency "$GRAPH1_ID" "$TASK_A_ID" "$TASK_B_ID"
run "$HIVEMIND" graph add-dependency "$GRAPH1_ID" "$TASK_B_ID" "$TASK_C_ID"
run "$HIVEMIND" graph validate "$GRAPH1_ID"

FLOW1_ID="$("$HIVEMIND" flow create "$GRAPH1_ID" 2>&1 | awk '/Flow ID:/ {print $3; exit}')"
echo "FLOW1_ID=$FLOW1_ID"
run "$HIVEMIND" flow start "$FLOW1_ID"
run_flow_until_terminal "$FLOW1_ID" 120

echo "=== Create task and graph/flow #2 (depends on flow #1) ==="
TASK_D_ID="$("$HIVEMIND" task create "$PROJECT_NAME" "$TASK_D_PROMPT" 2>&1 | awk '/ID:/ {print $2; exit}')"
echo "TASK_D_ID=$TASK_D_ID"
run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$TASK_D_ID" architecture-notes

GRAPH2_ID="$("$HIVEMIND" graph create "$PROJECT_NAME" beta-api-graph-2 --from-tasks "$TASK_D_ID" 2>&1 | awk '/Graph ID:/ {print $3; exit}')"
FLOW2_ID="$("$HIVEMIND" flow create "$GRAPH2_ID" 2>&1 | awk '/Flow ID:/ {print $3; exit}')"
echo "GRAPH2_ID=$GRAPH2_ID"
echo "FLOW2_ID=$FLOW2_ID"
run "$HIVEMIND" flow add-dependency "$FLOW2_ID" "$FLOW1_ID"
run "$HIVEMIND" flow start "$FLOW2_ID"
run_flow_until_terminal "$FLOW2_ID" 80

FLOW1_STATUS="$("$HIVEMIND" -f json flow status "$FLOW1_ID" | jq -r '.data.state')"
FLOW2_STATUS="$("$HIVEMIND" -f json flow status "$FLOW2_ID" | jq -r '.data.state')"
[[ "${FLOW1_STATUS,,}" == "completed" ]] || { echo "Flow1 not completed: $FLOW1_STATUS" >&2; exit 1; }
[[ "${FLOW2_STATUS,,}" == "completed" ]] || { echo "Flow2 not completed: $FLOW2_STATUS" >&2; exit 1; }

echo "=== Merge completed flows ==="
run "$HIVEMIND" merge prepare "$FLOW1_ID"
run "$HIVEMIND" merge approve "$FLOW1_ID"
run "$HIVEMIND" merge execute "$FLOW1_ID" --mode local
run "$HIVEMIND" merge prepare "$FLOW2_ID"
run "$HIVEMIND" merge approve "$FLOW2_ID"
run "$HIVEMIND" merge execute "$FLOW2_ID" --mode local

echo "=== Host-side validation ==="
for required in api_app/store.py api_app/router.py api_app/server.py tests/test_router.py docs/verification.md main.py; do
  [[ -f "$required" ]] || { echo "Missing required file: $required" >&2; exit 1; }
done
run python3 -m unittest discover -s tests -v

FLOW1_EVENTS="$("$HIVEMIND" -f json events stream --flow "$FLOW1_ID" --limit 4000)"
FLOW2_EVENTS="$("$HIVEMIND" -f json events stream --flow "$FLOW2_ID" --limit 2500)"
PROJECT_EVENTS="$("$HIVEMIND" -f json events stream --project "$PROJECT_NAME" --limit 8000)"

FLOW1_TOOLS="$(printf '%s' "$FLOW1_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_completed") | .tool_name' | sort -u | tr '\n' ' ')"
FLOW2_TOOLS="$(printf '%s' "$FLOW2_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_completed") | .tool_name' | sort -u | tr '\n' ' ')"
PROJECT_TOOLS="$(printf '%s' "$PROJECT_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_completed") | .tool_name' | sort -u | tr '\n' ' ')"
echo "FLOW1_TOOLS=$FLOW1_TOOLS"
echo "FLOW2_TOOLS=$FLOW2_TOOLS"
echo "PROJECT_TOOLS=$PROJECT_TOOLS"

for tool in list_files write_file read_file git_status git_diff run_command; do
  assert_contains "$PROJECT_TOOLS" "$tool" "project tool coverage"
done

PROVIDER_LINE="$(printf '%s' "$FLOW1_EVENTS" | jq -r '.data[] | .payload | select(.type=="agent_invocation_started") | (.provider + "|" + .model)' | head -n1)"
assert_contains "$PROVIDER_LINE" "openrouter|" "native provider"
assert_contains "$PROVIDER_LINE" "$MODEL_ID" "model provenance"

ATTEMPTS_FLOW1="$("$HIVEMIND" -f json attempt list --flow "$FLOW1_ID" --limit 20 | jq -r '.data[].attempt_id')"
ATTEMPTS_FLOW2="$("$HIVEMIND" -f json attempt list --flow "$FLOW2_ID" --limit 20 | jq -r '.data[].attempt_id')"

CONTEXT_PASS_COUNT=0
for attempt_id in $ATTEMPTS_FLOW1 $ATTEMPTS_FLOW2; do
  CTX_JSON="$("$HIVEMIND" -f json attempt inspect "$attempt_id" --context)"
  TEMPLATE_ID="$(printf '%s' "$CTX_JSON" | jq -r '.context.manifest.template_id // empty')"
  SKILL_COUNT="$(printf '%s' "$CTX_JSON" | jq -r '.context.manifest.skills | length')"
  DOC_COUNT="$(printf '%s' "$CTX_JSON" | jq -r '.context.manifest.documents | length')"
  if [[ "$TEMPLATE_ID" == "beta-api-template" ]] && (( SKILL_COUNT >= 2 )) && (( DOC_COUNT >= 1 )); then
    CONTEXT_PASS_COUNT=$((CONTEXT_PASS_COUNT + 1))
  fi
done
if (( CONTEXT_PASS_COUNT == 0 )); then
  echo "No attempt context carried template/skills/documents as expected" >&2
  exit 1
fi

assert_contains "$PROJECT_EVENTS" "\"type\": \"governance_project_storage_initialized\"" "governance init event"
assert_contains "$PROJECT_EVENTS" "\"type\": \"template_instantiated\"" "template instantiate event"
assert_contains "$PROJECT_EVENTS" "\"type\": \"constitution_initialized\"" "constitution event"
assert_contains "$PROJECT_EVENTS" "\"type\": \"attempt_context_assembled\"" "context assembly event"
assert_contains "$PROJECT_EVENTS" "\"type\": \"runtime_capabilities_evaluated\"" "runtime capabilities event"
assert_contains "$PROJECT_EVENTS" "\"type\": \"agent_invocation_started\"" "native invocation event"

FLOW1_NATIVE_FAILS="$(printf '%s' "$FLOW1_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_failed") | .code' | wc -l | tr -d ' ')"
FLOW2_NATIVE_FAILS="$(printf '%s' "$FLOW2_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_failed") | .code' | wc -l | tr -d ' ')"

FLOW1_ATTEMPTS_COUNT="$(printf '%s\n' "$ATTEMPTS_FLOW1" | sed '/^$/d' | wc -l | tr -d ' ')"
FLOW2_ATTEMPTS_COUNT="$(printf '%s\n' "$ATTEMPTS_FLOW2" | sed '/^$/d' | wc -l | tr -d ' ')"
PROJECT_EVENT_COUNT="$(printf '%s' "$PROJECT_EVENTS" | jq '.data | length')"
FLOW1_EVENT_COUNT="$(printf '%s' "$FLOW1_EVENTS" | jq '.data | length')"
FLOW2_EVENT_COUNT="$(printf '%s' "$FLOW2_EVENTS" | jq '.data | length')"

cat > "$REPORT_PATH" <<EOF
# Native OpenRouter Beta API Report ($DATE_TAG)

## Scope
- Multi-file API build across multiple task flows
- Live native runtime using OpenRouter provider
- Governance artifact lifecycle + context pass-through verification
- Event trail and tool-call integrity checks

## Environment
- Hivemind binary: \`$HIVEMIND\`
- Model configured: \`$MODEL_ID\`
- Project: \`$PROJECT_NAME\`
- Repo path: \`$PWD\`

## IDs
- Flow 1: \`$FLOW1_ID\`
- Flow 2: \`$FLOW2_ID\`
- Tasks: \`$TASK_A_ID\`, \`$TASK_B_ID\`, \`$TASK_C_ID\`, \`$TASK_D_ID\`

## Validation Results
- Flow 1 status: \`$FLOW1_STATUS\`
- Flow 2 status: \`$FLOW2_STATUS\`
- Flow 1 tools completed: \`$FLOW1_TOOLS\`
- Flow 2 tools completed: \`$FLOW2_TOOLS\`
- Project tools completed: \`$PROJECT_TOOLS\`
- Provider/model observed: \`$PROVIDER_LINE\`
- Context pass-through checks passed on attempts: \`$CONTEXT_PASS_COUNT\`

## Event Metrics
- Project events inspected: \`$PROJECT_EVENT_COUNT\`
- Flow 1 events inspected: \`$FLOW1_EVENT_COUNT\`
- Flow 2 events inspected: \`$FLOW2_EVENT_COUNT\`
- Flow 1 attempts: \`$FLOW1_ATTEMPTS_COUNT\`
- Flow 2 attempts: \`$FLOW2_ATTEMPTS_COUNT\`
- Flow 1 tool_call_failed count: \`$FLOW1_NATIVE_FAILS\`
- Flow 2 tool_call_failed count: \`$FLOW2_NATIVE_FAILS\`

## Host-side Build Check
- \`python3 -m unittest discover -s tests -v\` passed.

## Artifact Files
- \`api_app/store.py\`
- \`api_app/router.py\`
- \`api_app/server.py\`
- \`tests/test_router.py\`
- \`docs/verification.md\`

## Logs
- Full log: \`$LOG_PATH\`
EOF

echo
echo "BETA_TEST_RESULT=PASS"
echo "REPORT_PATH=$REPORT_PATH"
echo "LOG_PATH=$LOG_PATH"
