//! Auto-generated shared helper types for registry split modules.
#![allow(clippy::doc_markdown, clippy::wildcard_imports, unused_imports)]

use crate::core::registry::shared_prelude::*;
use crate::core::registry::types::*;
use crate::core::registry::{Registry, RegistryConfig};

// DiffArtifact (158-163)
mod diff_scope;
pub(crate) use diff_scope::*;
mod graph_governance;
pub(crate) use graph_governance::*;
mod attempt_context;
pub(crate) use attempt_context::*;
mod runtime;
pub(crate) use runtime::*;
mod runtime_projection;
pub(crate) use runtime_projection::*;
