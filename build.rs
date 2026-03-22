use std::{env, fs, path::PathBuf};

// Keep this file content-changing when fragment membership changes so Cargo
// rebuilds the event payload generator deterministically across cached targets.

const FRAGMENTS: &[&str] = &[
    "src/core/events/payload/fragments/chat_sessions.rs",
    "src/core/events/payload/fragments/project_task_repo_runtime.rs",
    "src/core/events/payload/fragments/governance_graph_context.rs",
    "src/core/events/payload/fragments/graph_flow_execution.rs",
    "src/core/events/payload/fragments/workflow_execution.rs",
    "src/core/events/payload/fragments/attempt_merge_retry.rs",
    "src/core/events/payload/fragments/runtime_native_unknown.rs",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/core/events/payload.rs");
    for fragment in FRAGMENTS {
        println!("cargo:rerun-if-changed={fragment}");
    }

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set"));
    let destination = out_dir.join("event_payload_generated.rs");

    let mut generated = String::from(
        "/// Payload types for different events.\n\
         #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n\
         #[serde(tag = \"type\", rename_all = \"snake_case\")]\n\
         pub enum EventPayload {\n",
    );

    for fragment in FRAGMENTS {
        let path = manifest_dir.join(fragment);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        generated.push_str(&content);
        if !content.ends_with('\n') {
            generated.push('\n');
        }
        generated.push('\n');
    }

    generated.push_str("}\n");
    fs::write(&destination, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", destination.display()));
}
