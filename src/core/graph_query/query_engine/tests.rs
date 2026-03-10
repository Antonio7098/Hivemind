use super::*;
use std::collections::{BTreeMap, HashMap};

#[allow(clippy::too_many_lines)]
fn sample_index() -> GraphQueryIndex {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        "repo::a".to_string(),
        IndexedNode {
            repo_name: "repo".to_string(),
            logical_key: "a".to_string(),
            node_class: "file".to_string(),
            path: Some("src/a.rs".to_string()),
            partition: Some("core".to_string()),
        },
    );
    nodes.insert(
        "repo::b".to_string(),
        IndexedNode {
            repo_name: "repo".to_string(),
            logical_key: "b".to_string(),
            node_class: "file".to_string(),
            path: Some("src/b.rs".to_string()),
            partition: Some("core".to_string()),
        },
    );
    nodes.insert(
        "repo::c".to_string(),
        IndexedNode {
            repo_name: "repo".to_string(),
            logical_key: "c".to_string(),
            node_class: "file".to_string(),
            path: Some("src/c.rs".to_string()),
            partition: Some("api".to_string()),
        },
    );
    nodes.insert(
        "repo::d".to_string(),
        IndexedNode {
            repo_name: "repo".to_string(),
            logical_key: "d".to_string(),
            node_class: "symbol".to_string(),
            path: Some("src/d.rs".to_string()),
            partition: Some("api".to_string()),
        },
    );

    let mut outgoing = BTreeMap::new();
    outgoing.insert(
        "repo::a".to_string(),
        vec![
            GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::c".to_string(),
                edge_type: "calls".to_string(),
            },
            GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::b".to_string(),
                edge_type: "imports".to_string(),
            },
            GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::d".to_string(),
                edge_type: "uses".to_string(),
            },
        ],
    );
    outgoing.insert(
        "repo::b".to_string(),
        vec![GraphQueryEdge {
            source: "repo::b".to_string(),
            target: "repo::c".to_string(),
            edge_type: "imports".to_string(),
        }],
    );

    let mut incoming = BTreeMap::new();
    incoming.insert(
        "repo::b".to_string(),
        vec![GraphQueryEdge {
            source: "repo::a".to_string(),
            target: "repo::b".to_string(),
            edge_type: "imports".to_string(),
        }],
    );
    incoming.insert(
        "repo::c".to_string(),
        vec![
            GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::c".to_string(),
                edge_type: "calls".to_string(),
            },
            GraphQueryEdge {
                source: "repo::b".to_string(),
                target: "repo::c".to_string(),
                edge_type: "imports".to_string(),
            },
        ],
    );
    incoming.insert(
        "repo::d".to_string(),
        vec![GraphQueryEdge {
            source: "repo::a".to_string(),
            target: "repo::d".to_string(),
            edge_type: "uses".to_string(),
        }],
    );

    let all_edges = vec![
        GraphQueryEdge {
            source: "repo::a".to_string(),
            target: "repo::c".to_string(),
            edge_type: "calls".to_string(),
        },
        GraphQueryEdge {
            source: "repo::a".to_string(),
            target: "repo::b".to_string(),
            edge_type: "imports".to_string(),
        },
        GraphQueryEdge {
            source: "repo::b".to_string(),
            target: "repo::c".to_string(),
            edge_type: "imports".to_string(),
        },
        GraphQueryEdge {
            source: "repo::a".to_string(),
            target: "repo::d".to_string(),
            edge_type: "uses".to_string(),
        },
    ];

    GraphQueryIndex {
        nodes,
        outgoing,
        incoming,
        all_edges,
        repositories: Vec::new(),
        node_to_repository_index: HashMap::new(),
        node_to_block_selector: HashMap::new(),
    }
}

#[test]
fn neighbors_is_deterministic_and_bounded() {
    let index = sample_index();
    let request = GraphQueryRequest::Neighbors {
        node: "repo::a".to_string(),
        edge_types: Vec::new(),
        max_results: Some(3),
    };
    let result = index
        .execute(&request, "fp", &GraphQueryBounds::default())
        .expect("query should execute");

    let ids = result
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["repo::a", "repo::b", "repo::c"]);
    assert!(result.truncated);
    assert_eq!(result.cost.visited_edges, 3);
    assert_eq!(result.edges.len(), 2);
}

#[test]
fn subgraph_depth_respects_upper_bound() {
    let index = sample_index();
    let request = GraphQueryRequest::Subgraph {
        seed: "repo::a".to_string(),
        depth: 5,
        edge_types: Vec::new(),
        max_results: Some(20),
    };
    let error = index
        .execute(&request, "fp", &GraphQueryBounds::default())
        .expect_err("depth over limit should fail");
    assert_eq!(error.code, "graph_query_depth_exceeded");
}

#[test]
fn filter_matches_partition_and_path_prefix() {
    let index = sample_index();
    let request = GraphQueryRequest::Filter {
        node_type: Some("file".to_string()),
        path_prefix: Some("src".to_string()),
        partition: Some("core".to_string()),
        max_results: Some(10),
    };
    let result = index
        .execute(&request, "fp", &GraphQueryBounds::default())
        .expect("query should execute");

    let ids = result
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["repo::a", "repo::b"]);
    assert!(!result.truncated);
}
