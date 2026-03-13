//! Auto-generated shared prelude for registry split modules.

pub use crate::adapters::claude_code::{ClaudeCodeAdapter, ClaudeCodeConfig};
pub use crate::adapters::codex::{CodexAdapter, CodexConfig};
pub use crate::adapters::kilo::{KiloAdapter, KiloConfig};
pub use crate::adapters::opencode::OpenCodeConfig;
pub use crate::adapters::runtime::{
    build_protected_runtime_environment, format_execution_prompt, AttemptSummary, ExecutionInput,
    InteractiveAdapterEvent, InteractiveExecutionResult, NativeInvocationTrace,
    NativePayloadCaptureMode, RuntimeAdapter, RuntimeEnvBuildError, RuntimeEnvironmentProvenance,
    RuntimeError,
};
pub use crate::adapters::{runtime_descriptors, SUPPORTED_ADAPTERS};
pub use crate::core::context_window::{
    ContextBudgetPolicy, ContextEntry, ContextEntryCandidate, ContextOpRecord,
    ContextOperationActor, ContextWindow,
};
pub use crate::core::diff::{unified_diff, Baseline, ChangeType, Diff, FileChange};
pub use crate::core::enforcement::{ScopeEnforcer, VerificationResult};
pub use crate::core::error::{ErrorCategory, HivemindError, Result};
pub use crate::core::events::{
    CorrelationIds, Event, EventPayload, NativeBlobRef, NativeEventCorrelation,
    NativeEventPayloadCaptureMode, RuntimeOutputStream, RuntimeRole, RuntimeSelectionSource,
};
pub use crate::core::flow::{FlowState, RetryMode, RunMode, TaskExecState, TaskFlow};
pub use crate::core::graph::{GraphState, GraphTask, RetryPolicy, SuccessCriteria, TaskGraph};
pub use crate::core::graph_query::{
    load_partition_paths_from_constitution, GraphQueryBounds, GraphQueryIndex,
    GraphQueryRepository, GraphQueryRequest, GraphQueryResult, GRAPH_QUERY_ENV_CONSTITUTION_PATH,
    GRAPH_QUERY_ENV_GATE_ERROR, GRAPH_QUERY_ENV_SNAPSHOT_PATH, GRAPH_QUERY_REFRESH_HINT,
};
pub use crate::core::runtime_event_projection::{
    ProjectedRuntimeObservation, RuntimeEventProjector,
};
pub use crate::core::scope::{check_compatibility, RepoAccessMode, Scope, ScopeCompatibility};
pub use crate::core::state::{
    AppState, AttemptCheckpoint, AttemptCheckpointState, AttemptState, Project,
    ProjectRuntimeConfig, RuntimeRoleDefaults, Task, TaskRuntimeConfig, TaskState,
};
pub use crate::core::workflow::{
    WorkflowBagKeyField, WorkflowBagReducer, WorkflowBagSelector, WorkflowContextPatchBinding,
    WorkflowContextSnapshot, WorkflowContextState, WorkflowDataValue, WorkflowDefinition,
    WorkflowError, WorkflowInputBindingResolution, WorkflowOutputBag, WorkflowOutputBagEntry,
    WorkflowReduceFunction, WorkflowRun, WorkflowRunState, WorkflowSpecBinding, WorkflowSpecNode,
    WorkflowSpecNodeKind, WorkflowSpecVerification, WorkflowStepContextSnapshot,
    WorkflowStepDefinition, WorkflowStepInputBinding, WorkflowStepInputSource, WorkflowStepKind,
    WorkflowStepOutputBinding, WorkflowStepRun, WorkflowStepState, WorkflowValueSource,
};
pub use crate::core::worktree::{WorktreeConfig, WorktreeError, WorktreeManager, WorktreeStatus};
pub use crate::native::adapter::{NativeAdapterConfig, NativeRuntimeAdapter};
pub use crate::native::NativeRuntimeConfig;
pub use crate::storage::event_store::{EventFilter, EventStore, SqliteEventStore};
pub use chrono::Utc;
pub use fs2::FileExt;
pub use serde::{de::DeserializeOwned, Deserialize, Serialize};
pub use sha2::{Digest, Sha256};
pub use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
pub use std::env;
pub use std::fmt::Write as _;
pub use std::fs;
pub use std::fs::OpenOptions;
pub use std::hash::{Hash, Hasher};
pub use std::io::{Read, Write};
pub use std::path::{Path, PathBuf};
pub use std::process::{Command, Stdio};
pub use std::sync::Arc;
pub use std::time::{Duration, Instant};
pub use ucp_api::{
    build_code_graph, canonical_fingerprint, codegraph_prompt_projection,
    validate_code_graph_profile, CodeGraphBuildInput, PortableDocument,
    CODEGRAPH_EXTRACTOR_VERSION, CODEGRAPH_PROFILE_MARKER,
};
pub use uuid::Uuid;
