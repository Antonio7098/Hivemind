use super::*;

impl Registry {
    pub(crate) fn graph_facts_from_snapshot(
        snapshot: &GraphSnapshotArtifact,
        origin: &'static str,
    ) -> Result<GraphConstitutionFacts> {
        let mut facts = GraphConstitutionFacts::default();

        for repo in &snapshot.repositories {
            let document = repo.document.to_document().map_err(|e| {
                HivemindError::system(
                    "graph_snapshot_schema_invalid",
                    format!("Failed to load graph snapshot document: {e}"),
                    origin,
                )
            })?;

            let mut file_key_by_block_id: HashMap<String, String> = HashMap::new();

            for (block_id, block) in &document.blocks {
                let Some(class_name) = block
                    .metadata
                    .custom
                    .get("node_class")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };

                if class_name == "file" {
                    let logical_key = block
                        .metadata
                        .custom
                        .get("logical_key")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            HivemindError::system(
                                "graph_snapshot_schema_invalid",
                                "File node missing logical_key metadata",
                                origin,
                            )
                        })?;
                    let path =
                        Self::graph_block_path_value(&block.metadata.custom).ok_or_else(|| {
                            HivemindError::system(
                                "graph_snapshot_schema_invalid",
                                "File node missing path metadata",
                                origin,
                            )
                        })?;
                    let normalized_path = Self::normalize_graph_path(path);
                    let graph_key = format!("{}::{logical_key}", repo.repo_name);
                    file_key_by_block_id.insert(block_id.to_string(), graph_key.clone());
                    facts.files.insert(
                        graph_key,
                        GraphFileFact {
                            display_path: format!("{}/{}", repo.repo_name, normalized_path),
                            path: normalized_path.clone(),
                            symbol_key: format!("{}::{normalized_path}", repo.repo_name),
                            references: Vec::new(),
                        },
                    );
                } else if class_name == "symbol" {
                    if let Some(path) = Self::graph_block_path_value(&block.metadata.custom) {
                        let normalized_path = Self::normalize_graph_path(path);
                        facts
                            .symbol_file_keys
                            .insert(format!("{}::{normalized_path}", repo.repo_name));
                    }
                }
            }

            for (block_id, block) in &document.blocks {
                let Some(source_key) = file_key_by_block_id.get(&block_id.to_string()).cloned()
                else {
                    continue;
                };
                let Some(source) = facts.files.get_mut(&source_key) else {
                    continue;
                };
                for edge in &block.edges {
                    if edge.edge_type.as_str() != "references" {
                        continue;
                    }
                    let target_id = edge.target.to_string();
                    if let Some(target_key) = file_key_by_block_id.get(&target_id) {
                        source.references.push(target_key.clone());
                    }
                }
                source.references.sort();
                source.references.dedup();
            }
        }

        Ok(facts)
    }
}
