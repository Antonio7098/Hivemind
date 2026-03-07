//! Global command handlers.

use crate::cli::commands::{
    GlobalCommands, GlobalNotepadCommands, GlobalSkillCommands, GlobalSkillRegistryCommands,
    GlobalSystemPromptCommands, GlobalTemplateCommands,
};
use crate::cli::handlers::common::{get_registry, print_structured};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::registry::Registry;
use crate::core::skill_registry;
mod skills;
pub use skills::*;
mod artifacts;
pub use artifacts::*;
mod dispatch;
pub use dispatch::*;
