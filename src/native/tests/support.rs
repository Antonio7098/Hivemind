use crate::adapters::runtime::ExecutionInput;
use crate::core::scope::{ExecutionScope, FilePermission, FilesystemScope, PathRule, Scope};
use crate::native::tool_engine::{
    NativeApprovalCache, NativeApprovalPolicy, NativeCommandPolicy, NativeExecPolicyManager,
    NativeNetworkApprovalCache, NativeNetworkPolicy, NativeSandboxPolicy, ToolExecutionContext,
};
use std::cell::RefCell;
use std::collections::VecDeque;

pub(crate) use serde_json::json;
pub(crate) use std::collections::HashMap;
pub(crate) use std::fs;
pub(crate) use std::path::Path;
pub(crate) use std::sync::{Arc, Mutex};
pub(crate) use tempfile::tempdir;

pub(crate) use super::super::*;
pub(crate) use crate::adapters::runtime::{
    AttemptSummary, NativePromptMetadata, NativeToolCallFailure, NativeToolCallTrace,
};
pub(crate) use crate::native::tool_engine::{NativeToolAction, NativeToolEngine};
pub(crate) use crate::native::turn_items::{
    TurnItemCorrelation, TurnItemKind, TurnItemOutcome, TurnItemProvenance,
};

pub(crate) fn native_input(native_prompt_metadata: Option<NativePromptMetadata>) -> ExecutionInput {
    ExecutionInput {
        task_description: "test task".to_string(),
        success_criteria: "done".to_string(),
        context: Some("test context".to_string()),
        declared_checkpoint_ids: Vec::new(),
        prior_attempts: Vec::new(),
        verifier_feedback: None,
        native_prompt_metadata,
    }
}

pub(crate) fn allow_all_scope() -> Scope {
    Scope::new()
        .with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("*", FilePermission::Write)),
        )
        .with_execution(ExecutionScope::new().allow("*"))
}

pub(crate) fn test_tool_context<'a>(
    worktree: &'a Path,
    scope: &'a Scope,
    env: &'a HashMap<String, String>,
) -> ToolExecutionContext<'a> {
    let policy = NativeCommandPolicy::default();
    ToolExecutionContext {
        worktree,
        scope: Some(scope),
        sandbox_policy: NativeSandboxPolicy::default(),
        approval_policy: NativeApprovalPolicy::default(),
        network_policy: NativeNetworkPolicy::default(),
        command_policy: policy.clone(),
        exec_policy_manager: NativeExecPolicyManager {
            base: policy,
            ..NativeExecPolicyManager::default()
        },
        approval_cache: RefCell::new(NativeApprovalCache::default()),
        network_approval_cache: RefCell::new(NativeNetworkApprovalCache::default()),
        env,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RecordingModelClient {
    scripted: VecDeque<String>,
    prompts: Arc<Mutex<Vec<String>>>,
}

impl RecordingModelClient {
    pub(crate) fn new(scripted: Vec<String>, prompts: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            scripted: VecDeque::from(scripted),
            prompts,
        }
    }
}

impl ModelClient for RecordingModelClient {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        self.prompts
            .lock()
            .expect("prompts lock")
            .push(request.prompt.clone());
        Ok(self
            .scripted
            .pop_front()
            .unwrap_or_else(|| "DONE:recording model exhausted".to_string()))
    }
}
