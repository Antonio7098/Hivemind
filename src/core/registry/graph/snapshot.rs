use super::*;
mod artifact;
mod refresh;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use ucp_api::{build_code_graph, CodeGraphBuildInput, CodeGraphExtractorConfig};

    #[test]
    fn compute_snapshot_fingerprint_is_order_independent() {
        let empty_stats = ucp_api::CodeGraphStats {
            total_nodes: 0,
            repository_nodes: 0,
            directory_nodes: 0,
            file_nodes: 0,
            symbol_nodes: 0,
            total_edges: 0,
            reference_edges: 0,
            export_edges: 0,
            languages: BTreeMap::new(),
        };
        let empty_document: PortableDocument = serde_json::from_value(serde_json::json!({
            "id": "doc",
            "root": "root",
            "structure": {},
            "blocks": {},
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z",
                "modified_at": "2024-01-01T00:00:00Z"
            },
            "version": 1
        }))
        .expect("portable document fixture");
        let repo_a = GraphSnapshotRepositoryArtifact {
            repo_name: "a".to_string(),
            repo_path: "/tmp/a".to_string(),
            commit_hash: "111".to_string(),
            profile_version: "v1".to_string(),
            canonical_fingerprint: "fa".to_string(),
            stats: empty_stats.clone(),
            document: empty_document.clone(),
            structure_blocks_projection: String::new(),
        };
        let repo_b = GraphSnapshotRepositoryArtifact {
            repo_name: "b".to_string(),
            repo_path: "/tmp/b".to_string(),
            commit_hash: "222".to_string(),
            profile_version: "v1".to_string(),
            canonical_fingerprint: "fb".to_string(),
            stats: empty_stats,
            document: empty_document,
            structure_blocks_projection: String::new(),
        };

        let first = Registry::compute_snapshot_fingerprint(&[repo_a.clone(), repo_b.clone()]);
        let second = Registry::compute_snapshot_fingerprint(&[repo_b, repo_a]);
        assert_eq!(first, second);
    }

    #[test]
    fn aggregate_codegraph_stats_sums_language_counts() {
        let mut first_languages = BTreeMap::new();
        first_languages.insert("rust".to_string(), 2);
        let mut second_languages = BTreeMap::new();
        second_languages.insert("rust".to_string(), 3);
        second_languages.insert("python".to_string(), 1);

        let summary = Registry::aggregate_codegraph_stats(&[
            ucp_api::CodeGraphStats {
                total_nodes: 1,
                repository_nodes: 1,
                directory_nodes: 0,
                file_nodes: 0,
                symbol_nodes: 0,
                total_edges: 2,
                reference_edges: 1,
                export_edges: 1,
                languages: first_languages,
            },
            ucp_api::CodeGraphStats {
                total_nodes: 4,
                repository_nodes: 0,
                directory_nodes: 1,
                file_nodes: 1,
                symbol_nodes: 2,
                total_edges: 3,
                reference_edges: 2,
                export_edges: 1,
                languages: second_languages,
            },
        ]);

        assert_eq!(summary.total_nodes, 5);
        assert_eq!(summary.reference_edges, 3);
        assert_eq!(summary.languages.get("rust"), Some(&5));
        assert_eq!(summary.languages.get("python"), Some(&1));
    }

    #[test]
    fn codegraph_scope_contract_allows_custom_semantic_edges() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(repo.join("src")).expect("mkdir src");
        fs::write(
            repo.join("src/lib.rs"),
            "pub struct Thing;\nimpl Thing { pub fn helper(&self) {} }\n",
        )
        .expect("write lib");
        let built = build_code_graph(&CodeGraphBuildInput {
            repository_path: repo,
            commit_hash: "test".to_string(),
            config: CodeGraphExtractorConfig::default(),
        })
        .expect("build code graph");
        let document = PortableDocument::from_document(&built.document);
        let edge_types = document
            .blocks
            .values()
            .flat_map(|block| {
                block
                    .edges
                    .iter()
                    .map(|edge| edge.edge_type.as_str().clone())
            })
            .collect::<Vec<_>>();
        assert!(
            edge_types.iter().any(|edge| edge == "custom:for_type"),
            "{edge_types:?}"
        );

        Registry::ensure_codegraph_scope_contract(&document, "test")
            .expect("custom semantic edges should be allowed");
    }

    #[test]
    fn codegraph_scope_contract_accepts_coderef_paths_for_file_and_symbol_nodes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(repo.join("src")).expect("mkdir src");
        fs::write(
            repo.join("src/lib.rs"),
            "pub struct Thing;\nimpl Thing { pub fn helper(&self) {} }\n",
        )
        .expect("write lib");
        let built = build_code_graph(&CodeGraphBuildInput {
            repository_path: repo,
            commit_hash: "test".to_string(),
            config: CodeGraphExtractorConfig::default(),
        })
        .expect("build code graph");

        let document = PortableDocument::from_document(&built.document);
        Registry::ensure_codegraph_scope_contract(&document, "test")
            .expect("coderef.path should satisfy the path contract");
    }
}
