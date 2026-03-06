#!/usr/bin/env python3
"""Registry split migrator helper.

Extracts `impl Registry` methods from `src/core/registry_original.rs`, classifies
methods by module, and emits a coverage report plus optional extracted method files.

This is intentionally conservative: it does not patch Rust files automatically.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Tuple


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src/core/registry_original.rs"
OUT_DIR = ROOT / "ops/registry_migration"


MODULE_RULES: List[Tuple[str, List[str]]] = [
    (
        "governance",
        [
            r"^project_governance_",
            r"^global_",
            r"^constitution_",
            r"^governance_",
            r"^read_(global|project)_",
            r"^append_governance_",
            r"^default_constitution_artifact$",
            r"^read_constitution_artifact$",
            r"^parse_constitution_yaml$",
            r"^write_constitution_artifact$",
            r"^project_document_",
            r"^project_document_summary_from_artifact$",
            r"^validate_governance_identifier$",
            r"^normalized_string_list$",
            r"^resolve_actor$",
            r"^next_governance_revision$",
            r"^ensure_governance_layout$",
            r"^legacy_governance_mappings$",
            r"^copy_dir_if_missing$",
            r"^copy_file_if_missing$",
            r"^validate_constitution$",
            r"^load_snapshot_manifest$",
            r"^list_snapshot_manifests$",
            r"^load_template_instantiation_snapshot$",
            r"^ensure_global_governance_layout$",
            r"^latest_template_instantiation_snapshot$",
            r"^attempt_manifest_hash_for_attempt$",
            r"^read_governance_json$",
            r"^write_governance_json$",
            r"^governance_json_paths$",
            r"^build_governance_repair_plan$",
        ],
    ),
    (
        "runtime",
        [
            r"^runtime_",
            r"^project_runtime_",
            r"^effective_runtime_",
            r"^select_runtime_",
            r"^schedule_runtime_",
            r"^handle_runtime_failure$",
            r"^build_runtime_adapter$",
            r"^active_runtime_execution_path$",
            r"^classify_runtime_error$",
            r"^detect_runtime_output_failure$",
            r"^emit_runtime_error_classified$",
            r"^projected_runtime_event_payload$",
            r"^append_projected_runtime_observations$",
            r"^native_capture_mode_for_event$",
            r"^native_event_correlation$",
            r"^native_blob_sha256$",
            r"^persist_native_blob$",
            r"^append_native_invocation_events$",
            r"^ensure_supported_runtime_adapter$",
            r"^parse_runtime_env_pairs$",
            r"^task_runtime_override_for_role$",
            r"^task_runtime_to_project_runtime$",
            r"^max_parallel_from_defaults$",
            r"^task_runtime_set$",
            r"^task_runtime_set_role$",
            r"^task_runtime_clear$",
            r"^task_runtime_clear_role$",
            r"^task_set_run_mode$",
            r"^binary_available$",
            r"^health_for_runtime$",
            r"^parse_global_parallel_limit$",
        ],
    ),
    (
        "events",
        [
            r"^events_",
            r"^append_event$",
            r"^event_count$",
            r"^get_events$",
            r"^mirror_events_",
            r"^record_error_event$",
            r"^read_events$",
            r"^stream_events$",
            r"^get_event$",
            r"^normalize_concatenated_json_objects$",
            r"^read_mirror_events$",
            r"^summarize_event_log$",
            r"^first_event_mismatch$",
            r"^validate_mirror_recovery_source$",
            r"^timestamp_to_nanos$",
            r"^sql_text_expr$",
            r"^sql_optional_uuid_expr$",
            r"^sqlite_exec$",
            r"^build_sqlite_recovery_sql$",
        ],
    ),
    (
        "graph",
        [
            r"^graph_",
            r"^validate_graph$",
            r"^add_graph_",
            r"^build_code_graph$",
            r"^canonical_fingerprint$",
            r"^codegraph_prompt_projection$",
            r"^load_graph_",
            r"^list_graphs$",
            r"^resolve_repo_head_commit$",
            r"^aggregate_codegraph_stats$",
            r"^compute_snapshot_fingerprint$",
            r"^ensure_codegraph_scope_contract$",
            r"^read_graph_snapshot_artifact$",
            r"^append_graph_snapshot_failed_event$",
            r"^trigger_graph_snapshot_refresh$",
            r"^append_graph_query_executed_event$",
            r"^append_graph_query_event_for_native_tool_call$",
            r"^map_graph_query_error$",
            r"^encode_runtime_graph_query_gate_error$",
            r"^set_native_graph_query_runtime_env$",
            r"^ensure_graph_snapshot_current_for_constitution$",
            r"^normalize_graph_path$",
            r"^path_in_partition$",
            r"^evaluate_constitution_rules$",
            r"^append_constitution_violation_events$",
            r"^run_constitution_check$",
            r"^enforce_constitution_gate$",
            r"^get_graph$",
            r"^create_graph$",
            r"^validate_graph_issues$",
            r"^delete_graph$",
        ],
    ),
    (
        "flow",
        [
            r"^flow_",
            r"^start_flow$",
            r"^pause_flow$",
            r"^resume_flow$",
            r"^abort_flow$",
            r"^tick_flow$",
            r"^start_task$",
            r"^retry_task$",
            r"^verify_",
            r"^checkpoint_",
            r"^create_flow$",
            r"^delete_flow$",
            r"^complete_task$",
            r"^abort_task$",
            r"^close_task$",
            r"^list_attempts$",
            r"^fail_running_attempt$",
            r"^format_checkpoint_commit_message$",
            r"^create_checkpoint_commit$",
            r"^checkout_and_clean_worktree$",
            r"^merge_",
            r"^get_flow$",
            r"^maybe_autostart_dependent_flows$",
            r"^checkpoint_order$",
            r"^checkpoint_id_from_payload$",
            r"^acquire_flow_integration_lock$",
            r"^resolve_git_ref$",
            r"^resolve_task_frozen_commit_sha$",
            r"^emit_task_execution_frozen$",
            r"^emit_integration_lock_acquired$",
            r"^emit_merge_conflict$",
            r"^attempt_runtime_outcome$",
            r"^build_retry_context$",
            r"^truncate$",
            r"^inspect_task_worktree$",
            r"^resolve_latest_attempt_without_diff$",
            r"^normalized_checkpoint_ids$",
            r"^resolve_latest_attempt_with_diff$",
            r"^process_verifying_task$",
            r"^run_check_command$",
            r"^emit_task_execution_completion_events$",
            r"^capture_and_store_baseline$",
            r"^compute_and_store_diff$",
            r"^git_ref_exists$",
            r"^default_base_ref_for_repo$",
            r"^ensure_task_worktree_status$",
            r"^ensure_task_worktree$",
            r"^inspect_task_worktrees$",
            r"^tick_flow_once$",
            r"^auto_progress_flow$",
            r"^list_flows$",
            r"^list_checkpoints$",
            r"^unmet_flow_dependencies$",
            r"^can_auto_run_task$",
            r"^restart_flow$",
            r"^start_task_execution$",
            r"^complete_task_execution$",
            r"^get_attempt$",
            r"^get_attempt_diff$",
            r"^replay_flow$",
            r"^project_for_flow$",
        ],
    ),
    (
        "worktree",
        [
            r"^worktree_",
            r"^with_worktree_",
            r"^detect_git_operations$",
            r"^detect_commits_created$",
            r"^detect_branches_created$",
            r"^parse_git_status_paths$",
            r"^repo_git_head$",
            r"^repo_status_lines$",
            r"^list_tmp_entries$",
            r"^write_scope_baseline_artifact$",
            r"^read_scope_baseline_artifact$",
            r"^capture_scope_baseline_for_attempt$",
            r"^parse_scope_trace_written_paths$",
            r"^first_quoted_segment$",
            r"^scope_trace_is_ignored$",
            r"^artifacts_dir$",
            r"^baselines_dir$",
            r"^baseline_dir$",
            r"^baseline_json_path$",
            r"^baseline_files_dir$",
            r"^diffs_dir$",
            r"^diff_json_path$",
            r"^scope_traces_dir$",
            r"^scope_trace_path$",
            r"^scope_baselines_dir$",
            r"^scope_baseline_path$",
        ],
    ),
    (
        "context",
        [
            r"^build_attempt_context$",
            r"^get_context_window$",
            r"^update_context_window$",
            r"^get_context_budget_policy$",
            r"^set_context_budget_policy$",
            r"^attempt_context_",
            r"^assemble_attempt_context$",
        ],
    ),
    (
        "tasks",
        [
            r"^create_task$",
            r"^list_tasks$",
            r"^get_task$",
            r"^update_task$",
            r"^delete_task$",
            r"^assign_task$",
            r"^mark_task_",
            r"^attach_repo$",
            r"^detach_repo$",
        ],
    ),
    (
        "registry",
        [
            r"^open$",
            r"^open_with_config$",
            r"^with_store$",
            r"^state$",
            r"^with_store$",
            r"^list_events$",
            r"^list_projects$",
            r"^get_project$",
            r"^create_project$",
            r"^update_project$",
            r"^delete_project$",
            r"^config$",
            r"^data_dir$",
            r"^blobs_dir$",
            r"^blob_path_for_digest$",
            r"^runtime_start_flags$",
            r"^prepare_runtime_environment$",
            r"^runtime_env_build_error$",
            r"^has_model_flag$",
            r"^write_baseline_artifact$",
            r"^read_baseline_artifact$",
            r"^write_diff_artifact$",
            r"^read_diff_artifact$",
            r"^unified_diff_for_change$",
        ],
    ),
]


@dataclass
class Method:
    name: str
    vis: str
    start_line: int
    end_line: int
    text: str


@dataclass
class Block:
    name: str
    kind: str
    start_line: int
    end_line: int
    text: str


def _line_no(src: str, offset: int) -> int:
    return src.count("\n", 0, offset) + 1


def _find_matching_brace(src: str, open_idx: int) -> int:
    depth = 0
    i = open_idx
    in_str = False
    in_chr = False
    raw_hashes: int | None = None
    in_line_comment = False
    in_block_comment = 0
    escape = False

    while i < len(src):
        ch = src[i]
        nxt = src[i + 1] if i + 1 < len(src) else ""

        if in_line_comment:
            if ch == "\n":
                in_line_comment = False
            i += 1
            continue

        if in_block_comment > 0:
            if ch == "/" and nxt == "*":
                in_block_comment += 1
                i += 2
                continue
            if ch == "*" and nxt == "/":
                in_block_comment -= 1
                i += 2
                continue
            i += 1
            continue

        if raw_hashes is not None:
            if ch == '"':
                j = i + 1
                hashes = 0
                while j < len(src) and src[j] == "#":
                    hashes += 1
                    j += 1
                if hashes == raw_hashes:
                    raw_hashes = None
                    i = j
                    continue
            i += 1
            continue

        if not in_str and not in_chr:
            if ch == "/" and nxt == "/":
                in_line_comment = True
                i += 2
                continue
            if ch == "/" and nxt == "*":
                in_block_comment = 1
                i += 2
                continue

            # Raw strings: r"..." / r#"..."# / br#"..."# etc.
            if ch in ("r", "b"):
                start = i
                if ch == "b" and nxt == "r":
                    start = i + 1
                if src[start] == "r":
                    j = start + 1
                    hashes = 0
                    while j < len(src) and src[j] == "#":
                        hashes += 1
                        j += 1
                    if j < len(src) and src[j] == '"':
                        raw_hashes = hashes
                        i = j + 1
                        continue

        if in_str:
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == '"':
                in_str = False
            i += 1
            continue

        if in_chr:
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == "'":
                in_chr = False
            i += 1
            continue

        if ch == '"':
            in_str = True
            i += 1
            continue
        if ch == "'":
            in_chr = True
            i += 1
            continue

        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i

        i += 1

    raise ValueError("unmatched brace")


def _find_matching_brace_naive(src: str, open_idx: int) -> int:
    depth = 0
    for i in range(open_idx, len(src)):
        ch = src[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i
    raise ValueError("unmatched brace")


def extract_impl_registry_methods(src: str) -> List[Method]:
    methods: List[Method] = []
    impl_start = re.search(r"(?m)^\s*impl\s+Registry\s*\{", src)
    if not impl_start:
        return methods

    test_marker = re.search(r"(?m)^\s*#\[cfg\(test\)\]", src)
    slice_start = impl_start.start()
    slice_end = test_marker.start() if test_marker else len(src)
    body = src[slice_start:slice_end]
    body_offset = slice_start

    # Registry methods are at impl indentation level (4 spaces).
    method_pattern = re.compile(r"(?m)^ {4}(pub\s+)?fn\s+([a-zA-Z0-9_]+)[^\n]*\(")
    for m in method_pattern.finditer(body):
        name = m.group(2)
        vis = "pub" if m.group(1) else "private"
        sig_start = body_offset + m.start()

        block_start = sig_start
        while True:
            prev_nl = src.rfind("\n", 0, block_start - 1)
            if prev_nl < 0:
                break
            line_start = prev_nl + 1
            line = src[line_start:block_start].rstrip("\n")
            if re.match(r"\s*(///|#\[|//)", line):
                block_start = line_start
                continue
            break

        fn_open = src.find("{", sig_start)
        try:
            fn_close = _find_matching_brace(src, fn_open)
        except ValueError:
            try:
                fn_close = _find_matching_brace_naive(src, fn_open)
            except ValueError:
                continue
        fn_text = src[block_start : fn_close + 1]
        methods.append(
            Method(
                name=name,
                vis=vis,
                start_line=_line_no(src, block_start),
                end_line=_line_no(src, fn_close),
                text=fn_text,
            )
        )

    # dedupe by (name,start_line)
    methods.sort(key=lambda x: (x.start_line, x.name))

    # Safety net for methods missed by impl boundary parsing in very large files.
    required_fallbacks = [
        "get_flow",
        "maybe_autostart_dependent_flows",
        "checkpoint_order",
        "checkpoint_id_from_payload",
    ]
    existing = {m.name for m in methods}
    for name in required_fallbacks:
        if name in existing:
            continue
        pat = re.compile(rf"(?m)^\s*(pub\s+)?fn\s+{re.escape(name)}[^\n]*\(")
        m = pat.search(src)
        if not m:
            continue
        sig_start = m.start()
        block_start = sig_start
        while True:
            prev_nl = src.rfind("\n", 0, block_start - 1)
            if prev_nl < 0:
                break
            line_start = prev_nl + 1
            line = src[line_start:block_start].rstrip("\n")
            if re.match(r"\s*(///|#\[|//)", line):
                block_start = line_start
                continue
            break
        open_idx = src.find("{", sig_start)
        try:
            close_idx = _find_matching_brace(src, open_idx)
        except ValueError:
            try:
                close_idx = _find_matching_brace_naive(src, open_idx)
            except ValueError:
                continue
        methods.append(
            Method(
                name=name,
                vis="pub" if m.group(1) else "private",
                start_line=_line_no(src, block_start),
                end_line=_line_no(src, close_idx),
                text=src[block_start : close_idx + 1],
            )
        )

    methods.sort(key=lambda x: (x.start_line, x.name))
    return methods


def extract_private_type_blocks(src: str) -> List[Block]:
    blocks: List[Block] = []
    type_re = re.compile(r"^(\s*(?:#\[[^\n]+\]\s*\n)*)\s*(struct|enum)\s+([A-Za-z0-9_]+)\b", re.M)
    for m in type_re.finditer(src):
        name = m.group(3)
        # Skip public API types already moved to types.rs
        line_start = src.rfind("\n", 0, m.start()) + 1
        line = src[line_start : src.find("\n", line_start)]
        if "pub struct" in line or "pub enum" in line:
            continue
        semi_idx = src.find(";", m.end())
        open_idx = src.find("{", m.end())
        if open_idx < 0 or (semi_idx != -1 and semi_idx < open_idx):
            # tuple/unit struct or declaration without body braces
            if semi_idx == -1:
                continue
            close_idx = semi_idx
        else:
            try:
                close_idx = _find_matching_brace(src, open_idx)
            except ValueError:
                try:
                    close_idx = _find_matching_brace_naive(src, open_idx)
                except ValueError:
                    continue
        block_start = m.start()
        blocks.append(
            Block(
                name=name,
                kind=m.group(2),
                start_line=_line_no(src, block_start),
                end_line=_line_no(src, close_idx),
                text=src[block_start : close_idx + 1],
            )
        )
    return blocks


def extract_named_type_block(src: str, name: str) -> Block | None:
    m = re.search(rf"(?m)^(\s*(?:#\[[^\n]+\]\s*\n)*)\s*(struct|enum)\s+{re.escape(name)}\b", src)
    if not m:
        return None
    semi_idx = src.find(";", m.end())
    open_idx = src.find("{", m.end())
    if open_idx < 0 or (semi_idx != -1 and semi_idx < open_idx):
        if semi_idx == -1:
            return None
        close_idx = semi_idx
    else:
        try:
            close_idx = _find_matching_brace(src, open_idx)
        except ValueError:
            try:
                close_idx = _find_matching_brace_naive(src, open_idx)
            except ValueError:
                return None
    return Block(
        name=name,
        kind=m.group(2),
        start_line=_line_no(src, m.start()),
        end_line=_line_no(src, close_idx),
        text=src[m.start() : close_idx + 1],
    )


def extract_non_registry_impl_blocks(src: str) -> List[Block]:
    blocks: List[Block] = []
    impl_re = re.compile(r"\bimpl\s+([A-Za-z0-9_]+)\s*\{")
    allowlist = {"SelectedRuntimeAdapter"}
    max_span_lines = 600
    for m in impl_re.finditer(src):
        name = m.group(1)
        if name == "Registry":
            continue
        if name not in allowlist:
            continue
        open_idx = src.find("{", m.end() - 1)
        try:
            close_idx = _find_matching_brace(src, open_idx)
        except ValueError:
            continue
        start_line = _line_no(src, m.start())
        end_line = _line_no(src, close_idx)
        if end_line - start_line > max_span_lines:
            continue
        blocks.append(
            Block(
                name=name,
                kind="impl",
                start_line=start_line,
                end_line=end_line,
                text=src[m.start() : close_idx + 1],
            )
        )
    return blocks


def extract_registry_core_blocks(src: str) -> List[Block]:
    blocks: List[Block] = []

    # RegistryConfig struct
    for pat, kind, name in [
        (r"(?ms)^#\[derive\(Debug, Clone\)\]\s*pub struct RegistryConfig\s*\{.*?^\}", "struct", "RegistryConfig"),
        (r"(?ms)^/// The project registry manages projects via event sourcing\.\s*pub struct Registry\s*\{.*?^\}", "struct", "Registry"),
    ]:
        m = re.search(pat, src)
        if not m:
            continue
        text = m.group(0)
        if name == "Registry":
            text = text.replace("store: Arc<dyn EventStore>,", "pub(crate) store: Arc<dyn EventStore>,")
            text = text.replace("config: RegistryConfig,", "pub(crate) config: RegistryConfig,")
        blocks.append(
            Block(
                name=name,
                kind=kind,
                start_line=_line_no(src, m.start()),
                end_line=_line_no(src, m.end()),
                text=text,
            )
        )

    # impl RegistryConfig
    m = re.search(r"(?ms)^impl RegistryConfig\s*\{.*?^\}", src)
    if m:
        blocks.append(
            Block(
                name="RegistryConfig",
                kind="impl",
                start_line=_line_no(src, m.start()),
                end_line=_line_no(src, m.end()),
                text=m.group(0),
            )
        )
    return blocks


def extract_use_lines(src: str) -> List[str]:
    marker = "\n#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]"
    head = src.split(marker, 1)[0] if marker in src else src[:4000]
    return re.findall(r"(?ms)^use\s+.*?;\s*$", head)


def write_support_files(src: str, target_dir: Path) -> List[Path]:
    target_dir.mkdir(parents=True, exist_ok=True)
    out: List[Path] = []

    use_lines = extract_use_lines(src)
    prelude_lines = [
        "//! Auto-generated shared prelude for registry split modules.",
        "",
    ]
    for line in use_lines:
        prelude_lines.append(line.replace("use ", "pub use ", 1).rstrip())
    prelude_path = target_dir / "shared_prelude.rs"
    prelude_path.write_text("\n".join(prelude_lines) + "\n", encoding="utf-8")
    out.append(prelude_path)

    private_types = extract_private_type_blocks(src)
    types_rs = ROOT / "src/core/registry/types.rs"
    existing_type_names: set[str] = set()
    if types_rs.exists():
        types_text = types_rs.read_text(encoding="utf-8")
        existing_type_names = set(
            re.findall(r"(?m)^\s*(?:pub(?:\([^)]+\))?\s+)?(?:struct|enum)\s+([A-Za-z0-9_]+)\b", types_text)
        )
        private_types = [b for b in private_types if b.name not in existing_type_names]
    existing_names = {b.name for b in private_types}
    for required in ["CompletionArtifacts"]:
        if required not in existing_names:
            extra = extract_named_type_block(src, required)
            if extra is not None:
                private_types.append(extra)
    impl_blocks = extract_non_registry_impl_blocks(src)
    shared_types_lines = [
        "//! Auto-generated shared helper types for registry split modules.",
        "#![allow(",
        "    clippy::doc_markdown,",
        "    clippy::wildcard_imports,",
        "    unused_imports,",
        ")]",
        "",
        "use crate::core::registry::shared_prelude::*;",
        "use crate::core::registry::types::*;",
        "use crate::core::registry::{Registry, RegistryConfig};",
        "",
    ]
    for b in private_types:
        shared_types_lines.append(f"// {b.name} ({b.start_line}-{b.end_line})")
        text = re.sub(r"(?m)^(\s*)struct\s+", r"\1pub(crate) struct ", b.text)
        text = re.sub(r"(?m)^(\s*)enum\s+", r"\1pub(crate) enum ", text)
        if b.kind == "struct":
            text = re.sub(r"(?m)^(\s*)([a-zA-Z_][a-zA-Z0-9_]*)\s*:", r"\1pub(crate) \2:", text)
        shared_types_lines.append(text)
        shared_types_lines.append("")
    for b in impl_blocks:
        shared_types_lines.append(f"// {b.name} ({b.start_line}-{b.end_line})")
        impl_text = re.sub(r"(?m)^(\s*)fn\s+", r"\1pub(crate) fn ", b.text)
        shared_types_lines.append(impl_text)
        shared_types_lines.append("")
    shared_types_path = target_dir / "shared_types.rs"
    shared_types_path.write_text("\n".join(shared_types_lines), encoding="utf-8")
    out.append(shared_types_path)
    return out


def classify(name: str) -> str:
    for module, patterns in MODULE_RULES:
        for p in patterns:
            if re.search(p, name):
                return module
    return "unmapped"


def build_report(methods: List[Method]) -> Dict[str, object]:
    by_module: Dict[str, List[Method]] = {}
    for m in methods:
        by_module.setdefault(classify(m.name), []).append(m)

    summary = {
        "total_methods": len(methods),
        "modules": {k: len(v) for k, v in sorted(by_module.items())},
        "unmapped": [
            {
                "name": m.name,
                "line": m.start_line,
                "visibility": m.vis,
            }
            for m in by_module.get("unmapped", [])
        ],
    }
    return {"summary": summary, "by_module": by_module}


def write_outputs(report: Dict[str, object], methods: List[Method]) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    summary_path = OUT_DIR / "registry_methods_report.json"
    by_module_serializable = {
        module: [
            {
                "name": m.name,
                "start_line": m.start_line,
                "end_line": m.end_line,
                "visibility": m.vis,
            }
            for m in meths
        ]
        for module, meths in report["by_module"].items()
    }

    payload = {
        "summary": report["summary"],
        "methods": by_module_serializable,
    }
    summary_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    extracted_dir = OUT_DIR / "extracted_methods"
    extracted_dir.mkdir(parents=True, exist_ok=True)

    for module, meths in report["by_module"].items():
        out = extracted_dir / f"{module}.rs"
        chunks = [
            f"// AUTO-GENERATED from {SOURCE.relative_to(ROOT)}\n",
            f"// Module bucket: {module}\n\n",
        ]
        for m in meths:
            chunks.append(f"// {m.name} ({m.start_line}-{m.end_line})\n")
            chunks.append(m.text)
            chunks.append("\n\n")
        out.write_text("".join(chunks), encoding="utf-8")


def compare_with_split(report: Dict[str, object]) -> Dict[str, Dict[str, List[str]]]:
    split_dir = ROOT / "src/core/registry"
    fn_re = re.compile(r"^\s*(?:pub\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(", re.M)
    comparisons: Dict[str, Dict[str, List[str]]] = {}

    for module, meths in report["by_module"].items():
        target = split_dir / f"{module}.rs"
        expected = sorted({m.name for m in meths})

        if not target.exists():
            comparisons[module] = {
                "file": [str(target)],
                "missing": expected,
                "extra": [],
            }
            continue

        text = target.read_text(encoding="utf-8")
        present = sorted(set(fn_re.findall(text)))
        missing = sorted(set(expected) - set(present))
        extra = sorted(set(present) - set(expected))
        comparisons[module] = {
            "file": [str(target)],
            "missing": missing,
            "extra": extra,
        }

    return comparisons


def write_checklist(cmp: Dict[str, Dict[str, List[str]]], out_path: Path) -> None:
    lines: List[str] = ["# Registry Split Checklist", ""]
    for module in sorted(cmp.keys()):
        missing = cmp[module]["missing"]
        extra = cmp[module]["extra"]
        lines.append(f"## {module}")
        lines.append(f"- Missing: {len(missing)}")
        lines.append(f"- Extra: {len(extra)}")
        if missing:
            lines.append("- Methods to migrate:")
            for name in missing:
                lines.append(f"  - `{name}`")
        lines.append("")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def render_module_file(module: str, methods: List[Method], source_text: str) -> str:
    import_head = (
        "    Registry, CONSTITUTION_SCHEMA_VERSION, CONSTITUTION_VERSION,"
        if module != "registry"
        else "    CONSTITUTION_SCHEMA_VERSION, CONSTITUTION_VERSION,"
    )
    lines = [
        f"//! Auto-generated registry split module: {module}.",
        "//!",
        "//! Generated by `scripts/registry_split_migrator.py`.",
        "#![allow(",
        "    clippy::doc_markdown,",
        "    clippy::wildcard_imports,",
        "    clippy::type_complexity,",
        "    clippy::too_many_lines,",
        "    clippy::unnecessary_wraps,",
        "    unused_imports,",
        ")]",
        "",
        "use crate::core::registry::shared_prelude::*;",
        "use crate::core::registry::shared_types::*;",
        "use crate::core::registry::types::*;",
        "use crate::core::registry::{",
        import_head,
        "    GOVERNANCE_EXPORT_IMPORT_BOUNDARY, GOVERNANCE_FROM_LAYOUT, GOVERNANCE_PROJECTION_VERSION,",
        "    GOVERNANCE_RECOVERY_SNAPSHOT_SCHEMA_VERSION, GOVERNANCE_SCHEMA_VERSION, GOVERNANCE_TO_LAYOUT,",
        "    GRAPH_SNAPSHOT_SCHEMA_VERSION, GRAPH_SNAPSHOT_VERSION, ATTEMPT_CONTEXT_MAX_EXPAND_DEPTH,",
        "    ATTEMPT_CONTEXT_SCHEMA_VERSION, ATTEMPT_CONTEXT_SECTION_BUDGET_BYTES,",
        "    ATTEMPT_CONTEXT_TOTAL_BUDGET_BYTES, ATTEMPT_CONTEXT_TRUNCATION_POLICY, ATTEMPT_CONTEXT_VERSION,",
        "};",
        "",
    ]
    core_chunks: List[str] = []
    if module == "registry":
        core_blocks = extract_registry_core_blocks(source_text)
        for b in core_blocks:
            core_chunks.append(f"// {b.name} ({b.start_line}-{b.end_line})")
            core_chunks.append(b.text)
            core_chunks.append("")
    lines.extend(core_chunks)
    lines.append("impl Registry {")
    body: List[str] = []
    for m in sorted(methods, key=lambda x: x.start_line):
        method_text = re.sub(r"^(\s*)pub\s+fn\s+", r"\1pub fn ", m.text, count=1, flags=re.M)
        method_text = re.sub(r"^(\s*)fn\s+", r"\1pub(crate) fn ", method_text, count=1, flags=re.M)
        for line in method_text.splitlines():
            body.append(f"    {line}")
        body.append("")
    footer = ["}"]
    return "\n".join(lines + body + footer) + "\n"


def apply_modules(
    report: Dict[str, object],
    module_names: List[str],
    target_dir: Path,
    in_place: bool,
    run_fmt: bool,
    force_overwrite: bool,
) -> List[Path]:
    written: List[Path] = []
    split_dir = ROOT / "src/core/registry"
    target_dir.mkdir(parents=True, exist_ok=True)

    for module in module_names:
        if module not in report["by_module"]:
            raise ValueError(f"unknown module: {module}")
        methods = report["by_module"][module]
        rendered = render_module_file(module, methods, SOURCE.read_text(encoding="utf-8"))

        if in_place:
            out = split_dir / f"{module}.rs"
            if out.exists() and not force_overwrite:
                raise ValueError(
                    f"refusing to overwrite existing {out}; pass --force-overwrite to allow"
                )
        else:
            out = target_dir / f"{module}.rs"
        out.write_text(rendered, encoding="utf-8")
        written.append(out)

    if run_fmt and written:
        cmd = ["cargo", "fmt", "--all"]
        subprocess.run(cmd, cwd=ROOT, check=False)

    return written


def main() -> None:
    parser = argparse.ArgumentParser(description="Registry split migration helper")
    parser.add_argument("--write", action="store_true", help="Write report and extracted method files")
    parser.add_argument(
        "--compare-split",
        action="store_true",
        help="Compare extracted bucket methods against src/core/registry/<module>.rs",
    )
    parser.add_argument(
        "--apply-module",
        action="append",
        default=[],
        help="Generate module file for this bucket (repeatable)",
    )
    parser.add_argument(
        "--apply-all",
        action="store_true",
        help="Generate module files for all buckets",
    )
    parser.add_argument(
        "--in-place",
        action="store_true",
        help="Write generated files directly into src/core/registry/<module>.rs",
    )
    parser.add_argument(
        "--target-dir",
        default=str(OUT_DIR / "applied"),
        help="Output dir for generated files when not using --in-place",
    )
    parser.add_argument(
        "--fmt",
        action="store_true",
        help="Run cargo fmt after apply",
    )
    parser.add_argument(
        "--force-overwrite",
        action="store_true",
        help="Allow --in-place to overwrite existing module files",
    )
    parser.add_argument(
        "--write-checklist",
        action="store_true",
        help="Write markdown checklist from --compare-split results",
    )
    parser.add_argument(
        "--write-shared",
        action="store_true",
        help="Write extracted private helper types + non-Registry impls",
    )
    parser.add_argument(
        "--write-core",
        action="store_true",
        help="Write extracted core registry scaffolding blocks",
    )
    parser.add_argument(
        "--emit-support",
        action="store_true",
        help="Emit shared_prelude.rs and shared_types.rs support files",
    )
    parser.add_argument(
        "--support-target",
        default=str(ROOT / "src/core/registry"),
        help="Target directory for --emit-support",
    )
    args = parser.parse_args()

    src = SOURCE.read_text(encoding="utf-8")
    methods = extract_impl_registry_methods(src)
    report = build_report(methods)

    s = report["summary"]
    print(f"Total impl Registry methods: {s['total_methods']}")
    for module, count in s["modules"].items():
        print(f"- {module}: {count}")

    if s["unmapped"]:
        print("\nUnmapped methods:")
        for u in s["unmapped"]:
            print(f"- {u['name']} (line {u['line']})")

    if args.write:
        write_outputs(report, methods)
        print(f"\nWrote outputs under: {OUT_DIR}")

    if args.write_shared:
        shared_dir = OUT_DIR / "shared"
        shared_dir.mkdir(parents=True, exist_ok=True)
        types = extract_private_type_blocks(src)
        impls = extract_non_registry_impl_blocks(src)
        (shared_dir / "private_types.rs").write_text(
            "// AUTO-GENERATED private helper types\n\n"
            + "\n\n".join(
                f"// {b.name} ({b.start_line}-{b.end_line})\n{b.text}" for b in types
            )
            + "\n",
            encoding="utf-8",
        )
        (shared_dir / "impl_blocks.rs").write_text(
            "// AUTO-GENERATED non-Registry impl blocks\n\n"
            + "\n\n".join(
                f"// {b.name} ({b.start_line}-{b.end_line})\n{b.text}" for b in impls
            )
            + "\n",
            encoding="utf-8",
        )
        print(f"\nWrote shared helpers under: {shared_dir}")

    if args.write_core:
        core_dir = OUT_DIR / "core"
        core_dir.mkdir(parents=True, exist_ok=True)
        blocks = extract_registry_core_blocks(src)
        (core_dir / "registry_core.rs").write_text(
            "// AUTO-GENERATED core registry blocks\n\n"
            + "\n\n".join(
                f"// {b.name} ({b.start_line}-{b.end_line})\n{b.text}" for b in blocks
            )
            + "\n",
            encoding="utf-8",
        )
        print(f"\nWrote core scaffolding: {core_dir / 'registry_core.rs'}")

    if args.emit_support:
        support_paths = write_support_files(src, Path(args.support_target))
        print("\nWrote support files:")
        for p in support_paths:
            print(f"- {p}")

    if args.compare_split:
        cmp = compare_with_split(report)
        print("\nSplit comparison:")
        for module in sorted(cmp.keys()):
            missing = cmp[module]["missing"]
            extra = cmp[module]["extra"]
            print(f"- {module}: missing={len(missing)} extra={len(extra)}")
            if missing:
                print(f"  missing: {', '.join(missing[:8])}{' ...' if len(missing) > 8 else ''}")
            if extra:
                print(f"  extra: {', '.join(extra[:8])}{' ...' if len(extra) > 8 else ''}")
        if args.write_checklist:
            checklist_path = OUT_DIR / "split_checklist.md"
            write_checklist(cmp, checklist_path)
            print(f"\nWrote checklist: {checklist_path}")

    apply_modules_requested = bool(args.apply_module) or args.apply_all
    if apply_modules_requested:
        modules = sorted(report["by_module"].keys()) if args.apply_all else args.apply_module
        out_paths = apply_modules(
            report=report,
            module_names=modules,
            target_dir=Path(args.target_dir),
            in_place=args.in_place,
            run_fmt=args.fmt,
            force_overwrite=args.force_overwrite,
        )
        print("\nGenerated module files:")
        for p in out_paths:
            print(f"- {p}")


if __name__ == "__main__":
    main()
