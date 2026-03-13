//! `Workflow` - Workflow-native execution definitions and runs.
//!
//! This module introduces the workflow domain alongside legacy `TaskFlow`.
//! Workflows are event-sourced, replayable execution definitions that can
//! evolve into the primary orchestration model over time.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

#[cfg(test)]
mod tests;

const WORKFLOW_CONTEXT_SCHEMA: &str = "workflow_context";
const WORKFLOW_CONTEXT_SCHEMA_VERSION: u32 = 1;
const WORKFLOW_STEP_CONTEXT_SCHEMA: &str = "workflow_step_context";
const WORKFLOW_STEP_CONTEXT_SCHEMA_VERSION: u32 = 1;
const WORKFLOW_OUTPUT_BAG_SCHEMA: &str = "workflow_output_bag";
const WORKFLOW_OUTPUT_BAG_SCHEMA_VERSION: u32 = 1;
const WORKFLOW_LIST_SCHEMA: &str = "workflow_output_list";
const WORKFLOW_KEYED_MAP_SCHEMA: &str = "workflow_output_map";
const WORKFLOW_REDUCED_TEXT_SCHEMA: &str = "workflow_output_text";

/// Supported workflow step kinds for the workflow engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepKind {
    Task,
    Workflow,
    Conditional,
    Wait,
    Join,
}

/// Lifecycle state of a workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowRunState {
    Created,
    Running,
    Paused,
    Completed,
    Aborted,
}

impl WorkflowRunState {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created | Self::Paused, Self::Running)
                | (Self::Created | Self::Running | Self::Paused, Self::Aborted)
                | (Self::Running, Self::Paused | Self::Completed)
        )
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Aborted)
    }
}

/// Lifecycle state of an individual workflow step run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStepState {
    Pending,
    Ready,
    Running,
    Verifying,
    Retry,
    Waiting,
    Succeeded,
    Failed,
    Skipped,
    Aborted,
}

impl WorkflowStepState {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Pending | Self::Waiting | Self::Failed | Self::Retry,
                Self::Ready
            ) | (Self::Pending | Self::Ready, Self::Skipped)
                | (
                    Self::Pending
                        | Self::Ready
                        | Self::Running
                        | Self::Verifying
                        | Self::Retry
                        | Self::Waiting
                        | Self::Failed,
                    Self::Aborted
                )
                | (Self::Ready, Self::Running)
                | (Self::Retry, Self::Running | Self::Failed)
                | (
                    Self::Running,
                    Self::Verifying | Self::Retry | Self::Waiting | Self::Succeeded | Self::Failed
                )
                | (Self::Waiting, Self::Succeeded | Self::Failed)
                | (
                    Self::Verifying,
                    Self::Retry | Self::Succeeded | Self::Failed
                )
        )
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Skipped | Self::Aborted
        )
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn canonical_json(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(flag) => flag.to_string(),
        JsonValue::Number(number) => number.to_string(),
        JsonValue::String(text) => {
            serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
        }
        JsonValue::Array(items) => {
            let rendered = items.iter().map(canonical_json).collect::<Vec<_>>();
            format!("[{}]", rendered.join(","))
        }
        JsonValue::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let rendered = keys
                .into_iter()
                .map(|key| {
                    let value = map.get(key).expect("object key present");
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string()),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", rendered.join(","))
        }
    }
}

fn snapshot_hash_for_values(values: &BTreeMap<String, WorkflowDataValue>) -> String {
    let json = serde_json::to_value(values).unwrap_or(JsonValue::Null);
    sha256_hex(canonical_json(&json).as_bytes())
}

/// A typed workflow value with explicit schema attribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowDataValue {
    pub schema: String,
    pub schema_version: u32,
    pub value: JsonValue,
    #[serde(default)]
    pub content_hash: String,
}

impl WorkflowDataValue {
    pub fn new(
        schema: impl Into<String>,
        schema_version: u32,
        value: JsonValue,
    ) -> Result<Self, WorkflowError> {
        let schema = schema.into();
        if schema.trim().is_empty() {
            return Err(WorkflowError::InvalidSchema(
                "Workflow value schema cannot be empty".to_string(),
            ));
        }
        if schema_version == 0 {
            return Err(WorkflowError::InvalidSchema(
                "Workflow value schema version must be >= 1".to_string(),
            ));
        }

        let content_hash = sha256_hex(
            canonical_json(&serde_json::json!({
                "schema": schema,
                "schema_version": schema_version,
                "value": value,
            }))
            .as_bytes(),
        );

        Ok(Self {
            schema,
            schema_version,
            value,
            content_hash,
        })
    }

    pub fn normalized(self) -> Result<Self, WorkflowError> {
        Self::new(self.schema, self.schema_version, self.value)
    }
}

/// The source for a workflow step input binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowStepInputSource {
    ContextKey {
        key: String,
    },
    Bag {
        selector: WorkflowBagSelector,
        reducer: WorkflowBagReducer,
    },
    Literal {
        value: WorkflowDataValue,
    },
}

/// Per-step input binding definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepInputBinding {
    pub name: String,
    pub source: WorkflowStepInputSource,
}

/// A reusable selector over the append-only workflow output bag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBagSelector {
    pub output_name: String,
    #[serde(default)]
    pub producer_step_ids: Vec<Uuid>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub expected_schema: Option<String>,
    #[serde(default)]
    pub expected_schema_version: Option<u32>,
}

/// Key strategy used for deterministic keyed-map reducers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowBagKeyField {
    ProducerStepId,
    ProducerStepName,
    OutputName,
}

/// Explicit reducer functions for bag fan-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowReduceFunction {
    ConcatTextNewline,
}

/// Deterministic reducers over selected output bag entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowBagReducer {
    Single,
    OrderedList,
    KeyedMap { key_field: WorkflowBagKeyField },
    ReduceFunction { function: WorkflowReduceFunction },
}

/// Allowed typed condition expressions for conditional workflow steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowConditionExpression {
    ContextKeyExists {
        key: String,
    },
    ContextValueEquals {
        key: String,
        value: WorkflowDataValue,
    },
    BagCountAtLeast {
        selector: WorkflowBagSelector,
        count: u32,
    },
    BagValueEquals {
        selector: WorkflowBagSelector,
        reducer: WorkflowBagReducer,
        value: WorkflowDataValue,
    },
}

/// A named branch for conditional workflow steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowConditionalBranch {
    pub name: String,
    pub condition: WorkflowConditionExpression,
    #[serde(default)]
    pub activate_step_ids: Vec<Uuid>,
}

/// Conditional-step execution config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowConditionalConfig {
    #[serde(default)]
    pub branches: Vec<WorkflowConditionalBranch>,
    #[serde(default)]
    pub default_branch: Option<String>,
}

/// Wait-step trigger model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowWaitCondition {
    Signal {
        signal_name: String,
        #[serde(default)]
        payload_schema: Option<String>,
        #[serde(default)]
        payload_schema_version: Option<u32>,
    },
    Timer {
        duration_secs: u64,
    },
    HumanSignal {
        signal_name: String,
    },
}

/// Wait-step execution config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowWaitConfig {
    pub condition: WorkflowWaitCondition,
}

/// Workflow signal emitted by operators or external systems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowSignal {
    pub signal_name: String,
    pub idempotency_key: String,
    #[serde(default)]
    pub payload: Option<WorkflowDataValue>,
    #[serde(default)]
    pub step_id: Option<Uuid>,
    pub emitted_at: DateTime<Utc>,
    pub emitted_by: String,
}

/// Durable wait metadata for a step that is currently blocked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowWaitStatus {
    pub condition: WorkflowWaitCondition,
    pub activated_at: DateTime<Utc>,
    #[serde(default)]
    pub resume_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completion_reason: Option<String>,
    #[serde(default)]
    pub signal: Option<WorkflowSignal>,
}

/// A reusable value source for step outputs and context patches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowValueSource {
    Literal { value: WorkflowDataValue },
    InputBinding { binding: String },
    ChildContextKey { key: String },
}

/// A declarative output emitted on successful step completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepOutputBinding {
    pub name: String,
    pub source: WorkflowValueSource,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A declarative workflow-context patch applied on successful step completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowContextPatchBinding {
    pub key: String,
    pub source: WorkflowValueSource,
}

/// Parent behavior when a child workflow step reaches a terminal outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowChildTerminalBehavior {
    Complete,
    FailStep,
    AbortParent,
}

/// Configurable policy for child workflow terminal outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowChildFailurePolicy {
    pub on_success: WorkflowChildTerminalBehavior,
    pub on_failure: WorkflowChildTerminalBehavior,
    pub on_cancellation: WorkflowChildTerminalBehavior,
    pub on_timeout: WorkflowChildTerminalBehavior,
}

impl Default for WorkflowChildFailurePolicy {
    fn default() -> Self {
        Self {
            on_success: WorkflowChildTerminalBehavior::Complete,
            on_failure: WorkflowChildTerminalBehavior::FailStep,
            on_cancellation: WorkflowChildTerminalBehavior::FailStep,
            on_timeout: WorkflowChildTerminalBehavior::FailStep,
        }
    }
}

/// Child-workflow launch configuration for workflow steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowChildConfig {
    pub workflow_id: Uuid,
    #[serde(default)]
    pub failure_policy: WorkflowChildFailurePolicy,
}

/// Static step definition within a workflow definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepDefinition {
    pub id: Uuid,
    pub name: String,
    pub kind: WorkflowStepKind,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<Uuid>,
    #[serde(default)]
    pub input_bindings: Vec<WorkflowStepInputBinding>,
    #[serde(default)]
    pub output_bindings: Vec<WorkflowStepOutputBinding>,
    #[serde(default)]
    pub context_patches: Vec<WorkflowContextPatchBinding>,
    #[serde(default)]
    pub child_workflow: Option<WorkflowChildConfig>,
    #[serde(default)]
    pub conditional: Option<WorkflowConditionalConfig>,
    #[serde(default)]
    pub wait: Option<WorkflowWaitConfig>,
}

impl WorkflowStepDefinition {
    #[must_use]
    pub fn new(name: impl Into<String>, kind: WorkflowStepKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            description: None,
            depends_on: Vec::new(),
            input_bindings: Vec::new(),
            output_bindings: Vec::new(),
            context_patches: Vec::new(),
            child_workflow: None,
            conditional: None,
            wait: None,
        }
    }
}

/// Immutable workflow definition registered under a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub steps: BTreeMap<Uuid, WorkflowStepDefinition>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowDefinition {
    #[must_use]
    pub fn new(project_id: Uuid, name: impl Into<String>, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            description,
            steps: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn add_step(&mut self, step: WorkflowStepDefinition) {
        self.steps.insert(step.id, step);
        self.updated_at = Utc::now();
    }

    pub fn update_metadata(&mut self, name: Option<String>, description: Option<Option<String>>) {
        if let Some(name) = name {
            self.name = name;
        }
        if let Some(description) = description {
            self.description = description;
        }
        self.updated_at = Utc::now();
    }

    #[must_use]
    pub fn has_step_named(&self, name: &str) -> bool {
        self.steps.values().any(|step| step.name == name)
    }

    #[must_use]
    pub fn root_step_ids(&self) -> Vec<Uuid> {
        self.steps
            .values()
            .filter(|step| step.depends_on.is_empty())
            .map(|step| step.id)
            .collect()
    }
}

/// A point-in-time workflow-context snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowContextSnapshot {
    pub revision: u64,
    pub snapshot_hash: String,
    #[serde(default)]
    pub values: BTreeMap<String, WorkflowDataValue>,
    pub created_at: DateTime<Utc>,
    pub reason: String,
    #[serde(default)]
    pub trigger_step_id: Option<Uuid>,
    #[serde(default)]
    pub trigger_step_run_id: Option<Uuid>,
}

impl WorkflowContextSnapshot {
    #[must_use]
    pub fn new(
        revision: u64,
        values: BTreeMap<String, WorkflowDataValue>,
        reason: impl Into<String>,
        trigger_step_id: Option<Uuid>,
        trigger_step_run_id: Option<Uuid>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            revision,
            snapshot_hash: snapshot_hash_for_values(&values),
            values,
            created_at,
            reason: reason.into(),
            trigger_step_id,
            trigger_step_run_id,
        }
    }
}

/// Workflow-scoped mutable data plane, mutated only by explicit events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowContextState {
    pub schema: String,
    pub schema_version: u32,
    #[serde(default)]
    pub initialization_inputs: BTreeMap<String, WorkflowDataValue>,
    pub current_snapshot: WorkflowContextSnapshot,
    #[serde(default)]
    pub snapshots: Vec<WorkflowContextSnapshot>,
}

impl Default for WorkflowContextState {
    fn default() -> Self {
        Self::initialize(
            WORKFLOW_CONTEXT_SCHEMA.to_string(),
            WORKFLOW_CONTEXT_SCHEMA_VERSION,
            BTreeMap::new(),
            Utc::now(),
            "workflow_run_initialized",
        )
    }
}

impl WorkflowContextState {
    #[must_use]
    pub fn initialize(
        schema: String,
        schema_version: u32,
        initialization_inputs: BTreeMap<String, WorkflowDataValue>,
        created_at: DateTime<Utc>,
        reason: impl Into<String>,
    ) -> Self {
        let snapshot = WorkflowContextSnapshot::new(
            1,
            initialization_inputs.clone(),
            reason,
            None,
            None,
            created_at,
        );

        Self {
            schema,
            schema_version,
            initialization_inputs,
            current_snapshot: snapshot.clone(),
            snapshots: vec![snapshot],
        }
    }

    pub fn apply_patches(
        &self,
        patches: BTreeMap<String, WorkflowDataValue>,
        created_at: DateTime<Utc>,
        reason: impl Into<String>,
        trigger_step_id: Option<Uuid>,
        trigger_step_run_id: Option<Uuid>,
    ) -> Result<Self, WorkflowError> {
        if self.schema.trim().is_empty() {
            return Err(WorkflowError::InvalidSchema(
                "Workflow context schema cannot be empty".to_string(),
            ));
        }
        if self.schema_version == 0 {
            return Err(WorkflowError::InvalidSchema(
                "Workflow context schema version must be >= 1".to_string(),
            ));
        }

        let mut values = self.current_snapshot.values.clone();
        for (key, value) in patches {
            if key.trim().is_empty() {
                return Err(WorkflowError::InvalidContextKey(
                    "Workflow context key cannot be empty".to_string(),
                ));
            }
            values.insert(key, value);
        }

        let snapshot = WorkflowContextSnapshot::new(
            self.current_snapshot.revision.saturating_add(1),
            values,
            reason,
            trigger_step_id,
            trigger_step_run_id,
            created_at,
        );

        let mut history = self.snapshots.clone();
        history.push(snapshot.clone());

        Ok(Self {
            schema: self.schema.clone(),
            schema_version: self.schema_version,
            initialization_inputs: self.initialization_inputs.clone(),
            current_snapshot: snapshot,
            snapshots: history,
        })
    }
}

/// An append-only workflow output bag entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowOutputBagEntry {
    pub entry_id: Uuid,
    pub workflow_run_id: Uuid,
    pub producer_step_id: Uuid,
    pub producer_step_run_id: Uuid,
    #[serde(default)]
    pub branch_step_id: Option<Uuid>,
    #[serde(default)]
    pub join_step_id: Option<Uuid>,
    pub output_name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub payload: WorkflowDataValue,
    pub event_sequence: u64,
    pub appended_at: DateTime<Utc>,
}

/// Append-only workflow output bag projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowOutputBag {
    pub schema: String,
    pub schema_version: u32,
    pub bag_hash: String,
    pub next_sequence: u64,
    #[serde(default)]
    pub entries: Vec<WorkflowOutputBagEntry>,
}

impl Default for WorkflowOutputBag {
    fn default() -> Self {
        Self {
            schema: WORKFLOW_OUTPUT_BAG_SCHEMA.to_string(),
            schema_version: WORKFLOW_OUTPUT_BAG_SCHEMA_VERSION,
            bag_hash: sha256_hex(b"[]"),
            next_sequence: 1,
            entries: Vec::new(),
        }
    }
}

impl WorkflowOutputBag {
    #[must_use]
    pub fn append(&self, entry: WorkflowOutputBagEntry) -> Self {
        let mut entries = self.entries.clone();
        entries.push(entry);
        let json = serde_json::to_value(&entries).unwrap_or(JsonValue::Array(Vec::new()));
        Self {
            schema: self.schema.clone(),
            schema_version: self.schema_version,
            bag_hash: sha256_hex(canonical_json(&json).as_bytes()),
            next_sequence: entries.last().map_or(self.next_sequence, |item| {
                item.event_sequence.saturating_add(1)
            }),
            entries,
        }
    }
}

/// Resolved metadata for a single step input binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowInputBindingResolution {
    pub binding: String,
    pub source: WorkflowStepInputSource,
    #[serde(default)]
    pub selected_entry_ids: Vec<Uuid>,
    pub value: WorkflowDataValue,
}

/// Deterministic step-scoped input snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepContextSnapshot {
    pub schema: String,
    pub schema_version: u32,
    pub workflow_run_id: Uuid,
    pub step_id: Uuid,
    pub step_run_id: Uuid,
    pub context_snapshot_hash: String,
    pub output_bag_hash: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, WorkflowDataValue>,
    #[serde(default)]
    pub resolutions: Vec<WorkflowInputBindingResolution>,
    pub snapshot_hash: String,
    pub resolved_at: DateTime<Utc>,
}

impl WorkflowStepContextSnapshot {
    #[must_use]
    pub fn new(
        workflow_run_id: Uuid,
        step_id: Uuid,
        step_run_id: Uuid,
        context_snapshot_hash: String,
        output_bag_hash: String,
        inputs: BTreeMap<String, WorkflowDataValue>,
        resolutions: Vec<WorkflowInputBindingResolution>,
        resolved_at: DateTime<Utc>,
    ) -> Self {
        let snapshot_hash = sha256_hex(
            canonical_json(&serde_json::json!({
                "workflow_run_id": workflow_run_id,
                "step_id": step_id,
                "step_run_id": step_run_id,
                "context_snapshot_hash": context_snapshot_hash,
                "output_bag_hash": output_bag_hash,
                "inputs": inputs,
                "resolutions": resolutions,
            }))
            .as_bytes(),
        );

        Self {
            schema: WORKFLOW_STEP_CONTEXT_SCHEMA.to_string(),
            schema_version: WORKFLOW_STEP_CONTEXT_SCHEMA_VERSION,
            workflow_run_id,
            step_id,
            step_run_id,
            context_snapshot_hash,
            output_bag_hash,
            inputs,
            resolutions,
            snapshot_hash,
            resolved_at,
        }
    }
}

/// Runtime state of a single step within a workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepRun {
    pub id: Uuid,
    pub step_id: Uuid,
    pub state: WorkflowStepState,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub wait_status: Option<WorkflowWaitStatus>,
}

/// Runtime execution record for a workflow definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub project_id: Uuid,
    pub root_workflow_run_id: Uuid,
    #[serde(default)]
    pub parent_workflow_run_id: Option<Uuid>,
    #[serde(default)]
    pub parent_step_id: Option<Uuid>,
    pub state: WorkflowRunState,
    #[serde(default)]
    pub step_runs: BTreeMap<Uuid, WorkflowStepRun>,
    #[serde(default)]
    pub step_contexts: BTreeMap<Uuid, WorkflowStepContextSnapshot>,
    #[serde(default)]
    pub context: WorkflowContextState,
    #[serde(default)]
    pub output_bag: WorkflowOutputBag,
    #[serde(default)]
    pub signals: Vec<WorkflowSignal>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// Recursive lineage summary for nested workflow inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunLineageNode {
    pub run_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub state: WorkflowRunState,
    pub root_workflow_run_id: Uuid,
    #[serde(default)]
    pub parent_workflow_run_id: Option<Uuid>,
    #[serde(default)]
    pub parent_step_id: Option<Uuid>,
    #[serde(default)]
    pub parent_step_name: Option<String>,
    #[serde(default)]
    pub children: Vec<Self>,
}

/// Workflow-run inspection payload with explicit nested lineage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunInspectView {
    #[serde(flatten)]
    pub run: WorkflowRun,
    pub workflow_name: String,
    #[serde(default)]
    pub parent_step_name: Option<String>,
    #[serde(default)]
    pub child_runs: Vec<WorkflowRunLineageNode>,
}

impl WorkflowRun {
    #[must_use]
    pub fn new_root(definition: &WorkflowDefinition) -> Self {
        let now = Utc::now();
        let run_id = Uuid::new_v4();
        let step_runs = definition
            .steps
            .keys()
            .map(|step_id| {
                (
                    *step_id,
                    WorkflowStepRun {
                        id: Uuid::new_v4(),
                        step_id: *step_id,
                        state: WorkflowStepState::Pending,
                        updated_at: now,
                        reason: None,
                        wait_status: None,
                    },
                )
            })
            .collect();

        Self {
            id: run_id,
            workflow_id: definition.id,
            project_id: definition.project_id,
            root_workflow_run_id: run_id,
            parent_workflow_run_id: None,
            parent_step_id: None,
            state: WorkflowRunState::Created,
            step_runs,
            step_contexts: BTreeMap::new(),
            context: WorkflowContextState::default(),
            output_bag: WorkflowOutputBag::default(),
            signals: Vec::new(),
            created_at: now,
            started_at: None,
            completed_at: None,
            updated_at: now,
        }
    }

    #[must_use]
    pub fn new_child(
        definition: &WorkflowDefinition,
        root_workflow_run_id: Uuid,
        parent_workflow_run_id: Uuid,
        parent_step_id: Uuid,
    ) -> Self {
        let mut run = Self::new_root(definition);
        run.root_workflow_run_id = root_workflow_run_id;
        run.parent_workflow_run_id = Some(parent_workflow_run_id);
        run.parent_step_id = Some(parent_step_id);
        run
    }

    #[must_use]
    pub fn can_complete(&self) -> bool {
        self.step_runs
            .values()
            .all(|step_run| step_run.state.is_terminal())
    }
}

/// Workflow domain error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowError {
    InvalidRunTransition {
        from: WorkflowRunState,
        to: WorkflowRunState,
    },
    InvalidStepTransition {
        from: WorkflowStepState,
        to: WorkflowStepState,
    },
    StepNotFound(Uuid),
    InvalidSchema(String),
    InvalidContextKey(String),
    MissingContextKey(String),
    UnknownInputBinding(String),
    InvalidReducer(String),
    ReducerConflict(String),
    SchemaMismatch {
        expected: String,
        actual: String,
    },
    InvalidCondition(String),
    SignalConflict(String),
}

impl std::fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRunTransition { from, to } => {
                write!(f, "Invalid workflow run transition from {from:?} to {to:?}")
            }
            Self::InvalidStepTransition { from, to } => {
                write!(
                    f,
                    "Invalid workflow step transition from {from:?} to {to:?}"
                )
            }
            Self::StepNotFound(id) => write!(f, "Workflow step not found: {id}"),
            Self::InvalidSchema(message)
            | Self::InvalidContextKey(message)
            | Self::MissingContextKey(message)
            | Self::UnknownInputBinding(message)
            | Self::InvalidReducer(message)
            | Self::ReducerConflict(message)
            | Self::InvalidCondition(message)
            | Self::SignalConflict(message) => write!(f, "{message}"),
            Self::SchemaMismatch { expected, actual } => {
                write!(
                    f,
                    "Workflow schema mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for WorkflowError {}

impl WorkflowBagReducer {
    pub fn reduce(
        &self,
        selector: &WorkflowBagSelector,
        entries: &[WorkflowOutputBagEntry],
        step_names: &BTreeMap<Uuid, String>,
    ) -> Result<WorkflowDataValue, WorkflowError> {
        match self {
            Self::Single => {
                if entries.len() != 1 {
                    return Err(WorkflowError::ReducerConflict(format!(
                        "single reducer expected exactly one output entry for '{}', found {}",
                        selector.output_name,
                        entries.len()
                    )));
                }
                Ok(entries[0].payload.clone())
            }
            Self::OrderedList => WorkflowDataValue::new(
                WORKFLOW_LIST_SCHEMA,
                1,
                JsonValue::Array(
                    entries
                        .iter()
                        .map(|item| item.payload.value.clone())
                        .collect(),
                ),
            ),
            Self::KeyedMap { key_field } => {
                let mut map = serde_json::Map::new();
                for entry in entries {
                    let key = match key_field {
                        WorkflowBagKeyField::ProducerStepId => entry.producer_step_id.to_string(),
                        WorkflowBagKeyField::ProducerStepName => step_names
                            .get(&entry.producer_step_id)
                            .cloned()
                            .unwrap_or_else(|| entry.producer_step_id.to_string()),
                        WorkflowBagKeyField::OutputName => entry.output_name.clone(),
                    };
                    map.insert(key, entry.payload.value.clone());
                }
                WorkflowDataValue::new(WORKFLOW_KEYED_MAP_SCHEMA, 1, JsonValue::Object(map))
            }
            Self::ReduceFunction { function } => match function {
                WorkflowReduceFunction::ConcatTextNewline => {
                    let mut chunks = Vec::with_capacity(entries.len());
                    for entry in entries {
                        match &entry.payload.value {
                            JsonValue::String(text) => chunks.push(text.clone()),
                            other => {
                                return Err(WorkflowError::InvalidReducer(format!(
                                    "concat_text_newline requires string payloads, got {}",
                                    canonical_json(other)
                                )));
                            }
                        }
                    }
                    WorkflowDataValue::new(
                        WORKFLOW_REDUCED_TEXT_SCHEMA,
                        1,
                        JsonValue::String(chunks.join("\n")),
                    )
                }
            },
        }
    }
}

impl WorkflowWaitCondition {
    #[must_use]
    pub fn to_wait_status(&self, activated_at: DateTime<Utc>) -> WorkflowWaitStatus {
        let resume_at = match self {
            Self::Timer { duration_secs } => {
                chrono::Duration::try_seconds((*duration_secs).min(i64::MAX as u64) as i64)
                    .map(|delta| activated_at + delta)
            }
            Self::Signal { .. } | Self::HumanSignal { .. } => None,
        };
        WorkflowWaitStatus {
            condition: self.clone(),
            activated_at,
            resume_at,
            completed_at: None,
            completion_reason: None,
            signal: None,
        }
    }
}
