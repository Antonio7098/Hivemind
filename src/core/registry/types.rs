//! Type definitions for the registry module.
//!
//! This module contains all the DTOs, result structs, and enums
//! used throughout the registry functionality.

mod constitution;
mod globals;
mod governance;
mod graph;
mod runtime;

pub use constitution::*;
pub use globals::*;
pub use governance::*;
pub use graph::*;
pub use runtime::*;

pub(crate) use globals::{GlobalSkillArtifact, GlobalSystemPromptArtifact, GlobalTemplateArtifact};
pub(crate) use governance::ProjectDocumentArtifact;
