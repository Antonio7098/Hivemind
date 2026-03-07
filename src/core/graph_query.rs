//! Deterministic graph-query substrate over UCP graph snapshot artifacts.
//!
//! Sprint 46 introduces bounded graph query primitives that can be reused by
//! CLI inspection commands and native runtime tools.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use ucp_api::PortableDocument;

mod index;
mod query_engine;
mod support;
mod types;

pub use index::GraphQueryIndex;
pub(crate) use index::IndexedNode;
pub use support::{compute_snapshot_fingerprint, load_partition_paths_from_constitution};
use support::{match_partition, normalize_graph_path};
pub use types::*;

const DEFAULT_MAX_RESULTS: usize = 200;
const MAX_RESULTS_LIMIT: usize = 500;
const MAX_SUBGRAPH_DEPTH: usize = 4;
pub const GRAPH_QUERY_ENV_SNAPSHOT_PATH: &str = "HIVEMIND_GRAPH_SNAPSHOT_PATH";
pub const GRAPH_QUERY_ENV_CONSTITUTION_PATH: &str = "HIVEMIND_PROJECT_CONSTITUTION_PATH";
pub const GRAPH_QUERY_ENV_GATE_ERROR: &str = "HIVEMIND_GRAPH_QUERY_GATE_ERROR";
pub const GRAPH_QUERY_REFRESH_HINT: &str = "Run: hivemind graph snapshot refresh <project>";
