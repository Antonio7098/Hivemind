# Sprint 58 Native Runtime Report (2026-03-07)

## Scope
- Real OpenRouter-backed validation for Sprint 58 native tool loop, agent modes, and prompt assembly metadata.
- Task-executor success flow with multi-turn tool use, tool-result reuse, tests, git inspection, and graph query.
- Freeflow attribution flow proving mutating-tool availability.
- Planner denial flow proving mode-aware tool rejection.

## Environment
- Hivemind binary: /home/antonio/.cargo/shared-target/debug/hivemind
- Model: nvidia/nemotron-3-nano-30b-a3b:free
- Project: sprint58-native-runtime
- Proof phrase: sprint58-proof-1772913693
- Prompt headroom: 512

## Flow Results
- Flow 1 (task_executor): 405ae8be-6d56-455a-b50a-f3b8cac653b3 / completed
- Flow 2 (freeflow): e9169878-5654-4d0c-b984-49ce91b54349 / completed
- Flow 3 (planner denial): 84a5ad62-0066-42a1-97f9-51bed70c6546 / denial observed

## Assertions Passed
- Task executor invocation exposed agent_mode=task_executor, allowed tools, prompt hashes, context hashes, and prompt headroom.
- Multi-turn in-loop execution was observed with increasing visible_item_count after prior tool results.
- The verification artifact quoted the exact proof phrase obtained from a prior read_file call.
- Freeflow invocation exposed agent_mode=freeflow and successfully completed read_file + write_file.
- Planner invocation exposed agent_mode=planner, omitted write_file from allowed_tools, and emitted native_tool_mode_denied with planner policy tags.

## Metrics
- Flow 1 tools: git_diff git_status graph_query list_files read_file run_command write_file 
- Flow 2 tools: read_file write_file 
- Flow 3 failure codes: native_tool_mode_denied 
- Flow 1 event count: 135
- Flow 2 event count: 79
- Flow 3 event count: 54

## Artifacts
- Log: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-native-runtime.log
- Report: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-native-runtime-report.md
- Flow 1 events: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-flow1-events.json
- Flow 2 events: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-flow2-events.json
- Flow 3 events: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-flow3-events.json
- Flow 1 native summary: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-flow1-native-summary.json
- Flow 2 native summary: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-flow2-native-summary.json
- Flow 3 native summary: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-flow3-native-summary.json
- Attempt context: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-attempt-context.json
- Analysis: /home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-07-sprint58-analysis.json
