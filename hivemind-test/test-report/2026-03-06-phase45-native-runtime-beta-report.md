# Phase 4.5 Native Runtime Beta Report (2026-03-06)

## Scope
- Phase 4.5 hardening validation (Sprints 52-57 implemented scope)
- Native runtime end-to-end beta using a substantial multi-module API project
- Two flow execution with inter-flow dependency, merge cycle, and host validation
- Unit matrix for startup hardening, env policy, sandbox/approval/network policy, transport retry/fallback, runtime durability/secrets/readiness

## Environment
- Hivemind binary: `/tmp/hivemind-manual-target/release/hivemind`
- Runtime adapter/model: `native` / `nvidia/nemotron-3-nano-30b-a3b:free`
- Project: `phase45-native-runtime-beta-api`
- Test repository: `/tmp/hm-phase45-native-runtime-beta/repo`
- Data dir: `/tmp/hm-phase45-native-runtime-beta/home/.hivemind`

## IDs
- Flow 1: `9cb66ef6-5a76-401b-819e-8230d822b598`
- Flow 2: `3cf8f93f-56be-42c8-b132-c7b06c12e526`
- Tasks: `82c2bf7a-ba46-4c0e-a478-8f62deeaf48b`, `da997154-c941-49a9-aa55-1b0ac1504c57`, `629f3f01-d809-4de8-ae93-b407e63fdff5`, `167a4330-5a07-417f-bffe-6ec4f6462137`

## Beta Results
- Flow 1 status: `completed`
- Flow 2 status: `completed`
- Flow 1 attempts: `4`
- Flow 2 attempts: `1`
- Context pass-through checks: `5`
- Provider/model observed: `openrouter|nvidia/nemotron-3-nano-30b-a3b:free`

## Tool Coverage
- Project tools: `git_diff git_status graph_query list_files read_file run_command write_file `
- Flow 1 tools: `git_diff git_status graph_query list_files read_file run_command write_file `
- Flow 2 tools: `git_diff git_status read_file run_command write_file `

## Event Metrics
- Project events inspected: `568`
- Flow 1 events inspected: `398`
- Flow 2 events inspected: `102`
- Runtime env payload sample: `{"type":"runtime_environment_prepared","attempt_id":"9967fc13-d39c-4605-9b47-70969c931c44","adapter_name":"native","inherit_mode":"all","inherited_keys":["ANDROID_HOME","ANDROID_SDK_ROOT","AUGMENT_AGENT","CLUTTER_DISABLE_MIPMAPPED_TEXT","COLORTERM","DBUS_SESSION_BUS_ADDRESS","DEBUGINFOD_URLS","DESKTOP_SESSION","DISPLAY","DOCKER_HOST","DOTNET_BUNDLE_EXTRACT_BASE_DIR","DOTNET_ROOT","GDMSESSION","GNOME_DESKTOP_SESSION_ID","GNOME_SHELL_SESSION_MODE","GNOME_TERMINAL_SCREEN","GNOME_TERMINAL_SERVICE","GPG_AGENT_INFO","GSM_SKIP_SSH_AGENT_WORKAROUND","GTK_MODULES","HIVEMIND","HIVEMIND_DATA_DIR","HOME","LANG","LESSCLOSE","LESSOPEN","LIBVIRT_DEFAULT_URI","LOGNAME","LS_COLORS","MEMORY_PRESSURE_WATCH","MEMORY_PRESSURE_WRITE","NVM_DIR","OLDPWD","OPENROUTER_MODEL_ID","PATH","PWD","QT_ACCESSIBILITY","QT_IM_MODULE","SESSION_MANAGER","SHELL","SHLVL","SSH_AUTH_SOCK","SYSTEMD_EXEC_PID","TERM","USER","USERNAME","VTE_VERSION","WINDOWPATH","XAUTHORITY","XDG_CONFIG_DIRS","XDG_CURRENT_DESKTOP","XDG_DATA_DIRS","XDG_MENU_PREFIX","XDG_RUNTIME_DIR","XDG_SESSION_CLASS","XDG_SESSION_DESKTOP","XDG_SESSION_TYPE","XMODIFIERS","_"],"overlay_keys":["CARGO_TARGET_DIR","HIVEMIND_AGENT_BIN","HIVEMIND_ALL_WORKTREES","HIVEMIND_ATTEMPT_ID","HIVEMIND_BIN","HIVEMIND_FLOW_ID","HIVEMIND_GRAPH_SNAPSHOT_PATH","HIVEMIND_NATIVE_APPROVAL_MODE","HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS","HIVEMIND_NATIVE_EXEC_SESSION_CAP","HIVEMIND_NATIVE_NETWORK_MODE","HIVEMIND_NATIVE_NETWORK_PROXY_MODE","HIVEMIND_NATIVE_PROVIDER","HIVEMIND_NATIVE_SANDBOX_MODE","HIVEMIND_NATIVE_SECRETS_MASTER_KEY","HIVEMIND_NATIVE_STATE_DIR","HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST","HIVEMIND_PRIMARY_WORKTREE","HIVEMIND_PROJECT_CONSTITUTION_PATH","HIVEMIND_REPO_MAIN_WORKTREE","HIVEMIND_TASK_ID","OPENROUTER_API_KEY","PATH"],"explicit_sensitive_overlay_keys":["HIVEMIND_NATIVE_SECRETS_MASTER_KEY","OPENROUTER_API_KEY"],"dropped_sensitive_inherited_keys":["BETA_PHASE45_SECRET_TOKEN","OPENROUTER_API_KEY"],"dropped_reserved_inherited_keys":["HIVEMIND_RUNTIME_INTERNAL_PHASE45_RESERVED"]}`

## Runtime State Durability Evidence
- State DB path: `/tmp/hm-phase45-native-runtime-beta/native-state/native/runtime-state.sqlite`
- SQLite journal mode: `wal`
- runtime_logs rows: `10`
- job_leases rows: `0`
- Secrets store path: `/tmp/hm-phase45-native-runtime-beta/native-state/native/secrets.enc.json`
- Secrets keyring path: `/tmp/hm-phase45-native-runtime-beta/native-state/native/secrets.keyring` (`not_created_env_override`)
- Plaintext secret scan: `PASS`

## Host Validation
- `python3 -m unittest discover -s tests -v` passed after merging both flows.

## Artifacts
- Main log: `/home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-06-phase45-native-runtime-beta.log`
- Unit matrix log: `/home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-06-phase45-native-runtime-unit.log`
- Flow 1 events: `/home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-06-phase45-native-runtime-flow1-events.json`
- Flow 2 events: `/home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-06-phase45-native-runtime-flow2-events.json`
- Project events: `/home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-06-phase45-native-runtime-project-events.json`
- Report: `/home/antonio/programming/Hivemind/hivemind/hivemind-test/test-report/2026-03-06-phase45-native-runtime-beta-report.md`
