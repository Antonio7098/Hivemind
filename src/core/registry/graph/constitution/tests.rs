use super::*;
use std::fs;
use tempfile::tempdir;
use ucp_api::{build_code_graph, CodeGraphBuildInput, CodeGraphExtractorConfig, PortableDocument};

#[test]
fn normalize_graph_path_trims_and_normalizes_separators() {
    assert_eq!(
        Registry::normalize_graph_path(" ./src\\core/file.rs "),
        "src/core/file.rs"
    );
}

#[test]
fn path_in_partition_matches_directories_only() {
    assert!(Registry::path_in_partition("src/core/file.rs", "src/core"));
    assert!(Registry::path_in_partition("src/core", "src/core"));
    assert!(!Registry::path_in_partition(
        "src/core-utils/file.rs",
        "src/core"
    ));
    assert!(!Registry::path_in_partition("src/core/file.rs", ""));
}

#[test]
fn graph_facts_accept_coderef_paths_from_real_ucp_snapshot_nodes() {
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(
        repo.join("src/lib.rs"),
        "pub struct Thing;\nimpl Thing { pub fn helper(&self) {} }\n",
    )
    .expect("write lib");

    let built = build_code_graph(&CodeGraphBuildInput {
        repository_path: repo.clone(),
        commit_hash: "test".to_string(),
        config: CodeGraphExtractorConfig::default(),
    })
    .expect("build code graph");

    let snapshot = GraphSnapshotArtifact {
        schema_version: GRAPH_SNAPSHOT_SCHEMA_VERSION.to_string(),
        snapshot_version: GRAPH_SNAPSHOT_VERSION,
        provenance: GraphSnapshotProvenance {
            project_id: Uuid::new_v4(),
            head_commits: vec![GraphSnapshotRepositoryCommit {
                repo_name: "repo".to_string(),
                repo_path: repo.to_string_lossy().to_string(),
                commit_hash: "test".to_string(),
            }],
            generated_at: Utc::now(),
        },
        ucp_engine_version: "test-engine".to_string(),
        profile_version: "test-profile".to_string(),
        canonical_fingerprint: "fingerprint".to_string(),
        summary: Registry::aggregate_codegraph_stats(&[built.stats.clone()]),
        repositories: vec![GraphSnapshotRepositoryArtifact {
            repo_name: "repo".to_string(),
            repo_path: repo.to_string_lossy().to_string(),
            commit_hash: "test".to_string(),
            profile_version: "test-profile".to_string(),
            canonical_fingerprint: "fingerprint".to_string(),
            stats: built.stats,
            document: PortableDocument::from_document(&built.document),
            structure_blocks_projection: String::new(),
        }],
        static_projection: String::new(),
    };

    let facts = Registry::graph_facts_from_snapshot(&snapshot, "test")
        .expect("coderef.path-backed nodes should be accepted");

    assert!(
        facts.files.contains_key("repo::file:src/lib.rs"),
        "{facts:?}"
    );
    assert!(
        facts.symbol_file_keys.contains("repo::src/lib.rs"),
        "{facts:?}"
    );
}
