use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[must_use]
pub fn load_partition_paths_from_constitution(path: &Path) -> BTreeMap<String, String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&raw) else {
        return BTreeMap::new();
    };
    let Some(partitions) = value
        .get("partitions")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for item in partitions {
        let Some(id) = item
            .get("id")
            .and_then(serde_yaml::Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        let Some(path_value) = item
            .get("path")
            .and_then(serde_yaml::Value::as_str)
            .map(normalize_graph_path)
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        out.insert(id.to_string(), path_value);
    }
    out
}

#[must_use]
pub fn compute_snapshot_fingerprint(entries: &[SnapshotFingerprintEntry]) -> String {
    let mut ordered = entries
        .iter()
        .map(|entry| {
            format!(
                "{}|{}|{}|{}",
                entry.repo_name, entry.repo_path, entry.commit_hash, entry.canonical_fingerprint
            )
        })
        .collect::<Vec<_>>();
    ordered.sort();
    digest_hex(ordered.join("\n").as_bytes())
}

pub(super) fn request_max_results(
    request: &GraphQueryRequest,
    bounds: &GraphQueryBounds,
) -> Result<usize, GraphQueryError> {
    let raw = match request {
        GraphQueryRequest::Neighbors { max_results, .. }
        | GraphQueryRequest::Dependents { max_results, .. }
        | GraphQueryRequest::Subgraph { max_results, .. }
        | GraphQueryRequest::Filter { max_results, .. } => *max_results,
    }
    .unwrap_or(bounds.default_max_results);

    if raw == 0 {
        return Err(GraphQueryError::new(
            "graph_query_bounds_invalid",
            "max_results must be > 0",
        ));
    }
    if raw > bounds.max_results_limit {
        return Err(GraphQueryError::new(
            "graph_query_bounds_exceeded",
            format!(
                "max_results {} exceeds limit {}",
                raw, bounds.max_results_limit
            ),
        ));
    }
    Ok(raw)
}

pub(super) fn truncate_node_ids(
    node_ids: BTreeSet<String>,
    max_results: usize,
) -> (BTreeSet<String>, bool) {
    if node_ids.len() <= max_results {
        return (node_ids, false);
    }
    let retained = node_ids.into_iter().take(max_results).collect();
    (retained, true)
}

pub(super) fn edge_filter_set(edge_types: &[String]) -> Option<BTreeSet<String>> {
    let mut normalized = edge_types
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    if normalized.is_empty() {
        return None;
    }
    if normalized.remove("*") {
        return None;
    }
    Some(normalized)
}

pub(super) fn normalize_graph_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

pub(super) fn match_partition(path: &str, partitions: &BTreeMap<String, String>) -> Option<String> {
    let mut candidates = partitions
        .iter()
        .filter_map(|(partition_id, partition_path)| {
            if path == partition_path || path.starts_with(&format!("{partition_path}/")) {
                Some((partition_path.len(), partition_id.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    candidates.first().map(|(_, id)| id.clone())
}

fn digest_hex(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let digest = hasher.finalize();
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use ucp_codegraph::{
        CodeGraphBuildInput, CodeGraphExpandMode, CodeGraphExportConfig, CodeGraphNavigator,
        CodeGraphRenderConfig, CodeGraphTraversalConfig,
    };

    fn build_eval_graph() -> CodeGraphNavigator {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(tmp.path().join("src")).expect("mkdir src");
        fs::write(
            tmp.path().join("src/util.rs"),
            "pub fn util() -> i32 { 1 }\n",
        )
        .expect("write util");
        fs::write(
            tmp.path().join("src/lib.rs"),
            "mod util;\npub fn add(a: i32, b: i32) -> i32 { util::util() + a + b }\n",
        )
        .expect("write lib");
        let repository_path = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        CodeGraphNavigator::build(&CodeGraphBuildInput {
            repository_path,
            commit_hash: "HEAD".to_string(),
            config: Default::default(),
        })
        .expect("build eval graph")
    }

    #[test]
    fn load_partition_paths_ignores_invalid_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let constitution_path = tmp.path().join("constitution.yaml");
        fs::write(
            &constitution_path,
            r#"
partitions:
  - id: core
    path: src/core
  - id: empty
    path: ""
  - id: missing
"#,
        )
        .expect("write constitution");

        let partitions = load_partition_paths_from_constitution(&constitution_path);
        assert_eq!(partitions.len(), 1);
        assert_eq!(partitions.get("core"), Some(&"src/core".to_string()));
    }

    #[test]
    fn snapshot_fingerprint_is_order_invariant() {
        let entries = vec![
            SnapshotFingerprintEntry {
                repo_name: "a".to_string(),
                repo_path: "/tmp/a".to_string(),
                commit_hash: "aaa".to_string(),
                canonical_fingerprint: "111".to_string(),
            },
            SnapshotFingerprintEntry {
                repo_name: "b".to_string(),
                repo_path: "/tmp/b".to_string(),
                commit_hash: "bbb".to_string(),
                canonical_fingerprint: "222".to_string(),
            },
        ];
        let mut reversed = entries.clone();
        reversed.reverse();
        assert_eq!(
            compute_snapshot_fingerprint(&entries),
            compute_snapshot_fingerprint(&reversed)
        );
        assert_ne!(
            compute_snapshot_fingerprint(&entries),
            compute_snapshot_fingerprint(&[
                SnapshotFingerprintEntry {
                    repo_name: "a".to_string(),
                    repo_path: PathBuf::from("/tmp/a").display().to_string(),
                    commit_hash: "changed".to_string(),
                    canonical_fingerprint: "111".to_string(),
                },
                SnapshotFingerprintEntry {
                    repo_name: "b".to_string(),
                    repo_path: "/tmp/b".to_string(),
                    commit_hash: "bbb".to_string(),
                    canonical_fingerprint: "222".to_string(),
                }
            ])
        );
    }

    #[test]
    fn codegraph_session_covers_canonical_pathing_and_explanation_workflows() {
        let graph = build_eval_graph();
        let add = graph
            .resolve_selector("symbol:src/lib.rs::add")
            .expect("resolve add symbol");
        let util = graph
            .resolve_selector("symbol:src/util.rs::util")
            .expect("resolve util symbol");

        let path = graph
            .path_between(add, util, 4)
            .expect("path between symbols");
        assert!(!path.hops.is_empty());

        let mut session = graph.session();
        session.seed_overview(Some(4));
        session
            .expand(
                "src/lib.rs",
                CodeGraphExpandMode::File,
                &CodeGraphTraversalConfig::default(),
            )
            .expect("expand file");
        session
            .expand(
                "symbol:src/lib.rs::add",
                CodeGraphExpandMode::Dependencies,
                &CodeGraphTraversalConfig::default(),
            )
            .expect("expand dependencies");

        let explanation = session
            .why_selected("symbol:src/util.rs::util")
            .expect("selection explanation");
        assert!(explanation.selected);
        assert!(explanation.explanation.contains("dependency"));

        let export = session.export(
            &CodeGraphRenderConfig::default(),
            &CodeGraphExportConfig::compact(),
        );
        assert!(export.nodes.iter().any(|node| node.label.contains("add")));
        assert!(export.nodes.iter().any(|node| node.label.contains("util")));
    }
}
