use super::*;
mod artifact;
mod refresh;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::registry::RegistryConfig;
    use crate::core::scope::RepoAccessMode;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;
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

    #[test]
    fn graph_snapshot_refresh_marks_attempt_scoped_graphcode_stale_when_authoritative_changes() {
        let tmp = tempdir().expect("tempdir");
        let data_dir = tmp.path().join("data");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).expect("create repo dir");
        fs::write(repo.join("README.md"), "seed\n").expect("write readme");
        init_git_repo(&repo);

        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(data_dir)).expect("registry");
        let project = registry
            .create_project("graph-refresh-test", None)
            .expect("project");
        let project_id = project.id;
        registry
            .attach_repo(
                &project_id.to_string(),
                repo.to_str().expect("repo path"),
                Some("main"),
                RepoAccessMode::ReadWrite,
            )
            .expect("attach repo");
        registry
            .graph_snapshot_refresh(&project_id.to_string(), "test")
            .expect("initial graph snapshot refresh");

        let store = registry
            .open_native_runtime_state_store("test")
            .expect("open runtime store");
        let attempt_id = Uuid::new_v4();
        let registry_key = Registry::graphcode_attempt_registry_key(project_id, attempt_id);
        let session_ref = format!("{registry_key}:session:default");
        store
            .upsert_graphcode_artifact(&crate::native::runtime_hardening::GraphCodeArtifactUpsert {
                registry_key: registry_key.clone(),
                project_id: project_id.to_string(),
                substrate_kind: "codegraph".to_string(),
                storage_backend: "sqlite".to_string(),
                storage_reference: "graphcode_artifacts/attempt".to_string(),
                derivative_snapshot_path: None,
                constitution_path: None,
                canonical_fingerprint: "attempt-fp".to_string(),
                profile_version: ucp_api::CODEGRAPH_PROFILE_MARKER.to_string(),
                ucp_engine_version: ucp_api::CODEGRAPH_EXTRACTOR_VERSION.to_string(),
                extractor_version: ucp_api::CODEGRAPH_PROFILE_MARKER.to_string(),
                runtime_version: env!("CARGO_PKG_VERSION").to_string(),
                freshness_state: "current".to_string(),
                repo_manifest_json: "[]".to_string(),
                active_session_ref: Some(session_ref.clone()),
                snapshot_json: "{}".to_string(),
            })
            .expect("attempt artifact upsert");
        store
            .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
                session_ref: session_ref.clone(),
                registry_key: registry_key.clone(),
                substrate_kind: "codegraph".to_string(),
                current_focus_json: "[]".to_string(),
                pinned_nodes_json: "[]".to_string(),
                recent_traversals_json: "[]".to_string(),
                working_set_refs_json: "[]".to_string(),
                hydrated_excerpts_json: "[]".to_string(),
                path_artifacts_json: "[]".to_string(),
                snapshot_fingerprint: "attempt-fp".to_string(),
                freshness_state: "current".to_string(),
            })
            .expect("attempt session upsert");

        fs::write(repo.join("README.md"), "updated\n").expect("update readme");
        git_commit_all(&repo, "update repo head");

        let refresh = registry
            .graph_snapshot_refresh(&project_id.to_string(), "test")
            .expect("graph snapshot refresh after repo change");
        assert!(refresh.diff_detected);

        let attempt_artifact = store
            .graphcode_artifact_by_registry_key(&registry_key)
            .expect("attempt artifact by registry key")
            .expect("attempt artifact");
        assert_eq!(attempt_artifact.freshness_state, "stale");
        let attempt_session = store
            .graphcode_session_by_ref(&session_ref)
            .expect("attempt session by ref")
            .expect("attempt session");
        assert_eq!(attempt_session.freshness_state, "stale");
    }

    fn init_git_repo(repo_dir: &std::path::Path) {
        run_git(repo_dir, &["init", "-q"]);
        run_git(repo_dir, &["config", "user.name", "Augment Agent"]);
        run_git(repo_dir, &["config", "user.email", "augment@example.com"]);
        run_git(repo_dir, &["add", "README.md"]);
        run_git(repo_dir, &["commit", "-qm", "seed"]);
    }

    fn git_commit_all(repo_dir: &std::path::Path, message: &str) {
        run_git(repo_dir, &["add", "-A"]);
        run_git(repo_dir, &["commit", "-qm", message]);
    }

    fn run_git(repo_dir: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo_dir)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }
}
