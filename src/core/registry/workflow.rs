//! Workflow registry operations.

#![allow(clippy::wildcard_imports, unused_imports)]

use crate::core::registry::shared_prelude::*;
use crate::core::registry::shared_types::{
    AttemptContextWorkflowManifest, RuntimeStreamDetailLevel, RuntimeStreamItemView,
};
use crate::core::registry::MergeExecuteOptions;
use crate::core::registry::Registry;
use crate::core::workflow::{
    WorkflowChildConfig, WorkflowChildTerminalBehavior, WorkflowConditionExpression,
    WorkflowConditionalBranch, WorkflowConditionalConfig, WorkflowDefinition, WorkflowRun,
    WorkflowRunState, WorkflowSignal, WorkflowStepDefinition, WorkflowStepKind, WorkflowStepState,
    WorkflowWaitCondition, WorkflowWaitConfig, WorkflowWaitStatus,
};
use crate::core::{
    flow::{FlowState, TaskExecState, TaskFlow},
    graph::{GraphTask, RetryPolicy, SuccessCriteria},
    scope::RepoAccessMode,
    state::AppState,
};
use sha2::{Digest, Sha256};

impl Registry {
    fn child_run_events(&self, workflow_run_id: Uuid) -> Result<Vec<Event>> {
        self.read_events(&EventFilter {
            workflow_run_id: Some(workflow_run_id),
            ..EventFilter::default()
        })
    }

    fn child_run_aborted_reason(events: &[Event], workflow_run_id: Uuid) -> Option<String> {
        events.iter().find_map(|event| match &event.payload {
            EventPayload::WorkflowRunAborted {
                workflow_run_id: id,
                reason,
                ..
            } if *id == workflow_run_id => reason.clone(),
            _ => None,
        })
    }

    fn child_run_has_timeout_signal(events: &[Event]) -> bool {
        events.iter().any(|event| match &event.payload {
            EventPayload::RuntimeTerminated { reason, .. } => {
                reason.contains("timeout") || reason.contains("no_observable_progress_timeout")
            }
            EventPayload::RuntimeErrorClassified { category, code, .. } => {
                category == "timeout"
                    || code == "timeout"
                    || code == "no_observable_progress_timeout"
            }
            _ => false,
        })
    }

    fn child_run_terminal_behavior(
        &self,
        child_run: &WorkflowRun,
    ) -> Result<WorkflowChildTerminalBehavior> {
        let events = self.child_run_events(child_run.id)?;
        if Self::child_workflow_succeeded(child_run) {
            let step = child_run.parent_step_id.ok_or_else(|| {
                HivemindError::system(
                    "workflow_parent_step_missing",
                    format!(
                        "Child workflow run '{}' is missing parent step id",
                        child_run.id
                    ),
                    "registry:child_run_terminal_behavior",
                )
            })?;
            let parent_run_id = child_run.parent_workflow_run_id.ok_or_else(|| {
                HivemindError::system(
                    "workflow_parent_run_missing",
                    format!(
                        "Child workflow run '{}' is missing parent workflow run id",
                        child_run.id
                    ),
                    "registry:child_run_terminal_behavior",
                )
            })?;
            let parent_run = self.get_workflow_run(&parent_run_id.to_string())?;
            let parent_workflow = self.get_workflow(&parent_run.workflow_id.to_string())?;
            let parent_step = Self::workflow_step_for_definition(
                &parent_workflow,
                step,
                "registry:child_run_terminal_behavior",
            )?;
            let policy = parent_step
                .child_workflow
                .as_ref()
                .map(|config| config.failure_policy.clone())
                .unwrap_or_default();
            return Ok(policy.on_success);
        }

        let step = child_run.parent_step_id.ok_or_else(|| {
            HivemindError::system(
                "workflow_parent_step_missing",
                format!(
                    "Child workflow run '{}' is missing parent step id",
                    child_run.id
                ),
                "registry:child_run_terminal_behavior",
            )
        })?;
        let parent_run_id = child_run.parent_workflow_run_id.ok_or_else(|| {
            HivemindError::system(
                "workflow_parent_run_missing",
                format!(
                    "Child workflow run '{}' is missing parent workflow run id",
                    child_run.id
                ),
                "registry:child_run_terminal_behavior",
            )
        })?;
        let parent_run = self.get_workflow_run(&parent_run_id.to_string())?;
        let parent_workflow = self.get_workflow(&parent_run.workflow_id.to_string())?;
        let parent_step = Self::workflow_step_for_definition(
            &parent_workflow,
            step,
            "registry:child_run_terminal_behavior",
        )?;
        let policy = parent_step
            .child_workflow
            .as_ref()
            .map(|config| config.failure_policy.clone())
            .unwrap_or_default();

        if Self::child_run_has_timeout_signal(&events) {
            return Ok(policy.on_timeout);
        }
        if child_run.state == WorkflowRunState::Aborted
            && !Self::child_run_aborted_reason(&events, child_run.id)
                .as_deref()
                .is_some_and(|reason| reason.contains("timeout"))
        {
            return Ok(policy.on_cancellation);
        }
        Ok(policy.on_failure)
    }

    pub fn create_workflow(
        &self,
        project_id_or_name: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<WorkflowDefinition> {
        let project = self
            .get_project(project_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        if name.trim().is_empty() {
            let err = HivemindError::user(
                "invalid_workflow_name",
                "Workflow name cannot be empty",
                "registry:create_workflow",
            )
            .with_hint("Provide a non-empty workflow name");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let state = self.state()?;
        if state
            .workflows
            .values()
            .any(|workflow| workflow.project_id == project.id && workflow.name == name)
        {
            let err = HivemindError::user(
                "workflow_exists",
                format!(
                    "Workflow '{name}' already exists in project '{}'",
                    project.name
                ),
                "registry:create_workflow",
            )
            .with_hint("Choose a different workflow name");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let definition = WorkflowDefinition::new(project.id, name, description.map(str::to_string));
        self.append_event(
            Event::new(
                EventPayload::WorkflowDefinitionCreated {
                    definition: definition.clone(),
                },
                CorrelationIds::for_workflow(project.id, definition.id),
            ),
            "registry:create_workflow",
        )?;

        self.get_workflow(&definition.id.to_string())
    }

    pub fn update_workflow(
        &self,
        workflow_id_or_name: &str,
        name: Option<&str>,
        description: Option<&str>,
        clear_description: bool,
    ) -> Result<WorkflowDefinition> {
        let workflow = self
            .get_workflow(workflow_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let next_name = name.map(str::trim);
        if next_name.is_some_and(str::is_empty) {
            let err = HivemindError::user(
                "invalid_workflow_name",
                "Workflow name cannot be empty",
                "registry:update_workflow",
            )
            .with_hint("Provide a non-empty workflow name");
            self.record_error_event(
                &err,
                CorrelationIds::for_workflow(workflow.project_id, workflow.id),
            );
            return Err(err);
        }

        if let Some(next_name) = next_name {
            let state = self.state()?;
            if state.workflows.values().any(|candidate| {
                candidate.project_id == workflow.project_id
                    && candidate.id != workflow.id
                    && candidate.name == next_name
            }) {
                let err = HivemindError::user(
                    "workflow_exists",
                    format!("Workflow '{next_name}' already exists in this project"),
                    "registry:update_workflow",
                )
                .with_hint("Choose a different workflow name");
                self.record_error_event(
                    &err,
                    CorrelationIds::for_workflow(workflow.project_id, workflow.id),
                );
                return Err(err);
            }
        }

        let mut updated = workflow.clone();
        updated.update_metadata(
            next_name.map(str::to_string),
            if clear_description {
                Some(None)
            } else {
                description.map(|value| Some(value.to_string()))
            },
        );

        self.append_event(
            Event::new(
                EventPayload::WorkflowDefinitionUpdated {
                    definition: updated,
                },
                CorrelationIds::for_workflow(workflow.project_id, workflow.id),
            ),
            "registry:update_workflow",
        )?;

        self.get_workflow(&workflow.id.to_string())
    }

    pub fn workflow_add_step(
        &self,
        workflow_id_or_name: &str,
        name: &str,
        kind: WorkflowStepKind,
        description: Option<&str>,
        depends_on: &[String],
        input_bindings: Vec<WorkflowStepInputBinding>,
        output_bindings: Vec<WorkflowStepOutputBinding>,
        context_patches: Vec<WorkflowContextPatchBinding>,
        child_workflow_id: Option<&str>,
        conditional: Option<WorkflowConditionalConfig>,
        wait: Option<WorkflowWaitConfig>,
    ) -> Result<WorkflowDefinition> {
        let workflow = self
            .get_workflow(workflow_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let step_name = name.trim();
        if step_name.is_empty() {
            let err = HivemindError::user(
                "invalid_workflow_step_name",
                "Workflow step name cannot be empty",
                "registry:workflow_add_step",
            )
            .with_hint("Provide a non-empty workflow step name");
            self.record_error_event(
                &err,
                CorrelationIds::for_workflow(workflow.project_id, workflow.id),
            );
            return Err(err);
        }
        if workflow.has_step_named(step_name) {
            let err = HivemindError::user(
                "workflow_step_exists",
                format!("Workflow step '{step_name}' already exists"),
                "registry:workflow_add_step",
            )
            .with_hint("Choose a different workflow step name");
            self.record_error_event(
                &err,
                CorrelationIds::for_workflow(workflow.project_id, workflow.id),
            );
            return Err(err);
        }

        let mut dependency_ids = Vec::with_capacity(depends_on.len());
        for dependency in depends_on {
            let dependency_id = Uuid::parse_str(dependency).map_err(|_| {
                HivemindError::user(
                    "invalid_workflow_step_dependency",
                    format!("Invalid workflow step dependency id '{dependency}'"),
                    "registry:workflow_add_step",
                )
            })?;
            if !workflow.steps.contains_key(&dependency_id) {
                let err = HivemindError::user(
                    "workflow_step_dependency_not_found",
                    format!("Workflow dependency step '{dependency_id}' does not exist"),
                    "registry:workflow_add_step",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_workflow(workflow.project_id, workflow.id),
                );
                return Err(err);
            }
            dependency_ids.push(dependency_id);
        }
        dependency_ids.sort_unstable();
        dependency_ids.dedup();

        let input_bindings =
            Self::normalize_workflow_input_bindings(input_bindings, "registry:workflow_add_step")?;
        let output_bindings = Self::normalize_workflow_output_bindings(
            kind,
            output_bindings,
            &input_bindings,
            "registry:workflow_add_step",
        )?;
        let context_patches = Self::normalize_workflow_context_patches(
            kind,
            context_patches,
            &input_bindings,
            "registry:workflow_add_step",
        )?;
        let child_workflow = self.normalize_child_workflow_config(
            &workflow,
            kind,
            child_workflow_id,
            "registry:workflow_add_step",
        )?;
        let conditional = Self::normalize_workflow_conditional_config(
            kind,
            conditional,
            "registry:workflow_add_step",
        )?;
        let wait = Self::normalize_workflow_wait_config(kind, wait, "registry:workflow_add_step")?;

        let mut step = WorkflowStepDefinition::new(step_name, kind);
        step.description = description.map(str::to_string);
        step.depends_on = dependency_ids;
        step.input_bindings = input_bindings;
        step.output_bindings = output_bindings;
        step.context_patches = context_patches;
        step.child_workflow = child_workflow;
        step.conditional = conditional;
        step.wait = wait;

        let mut updated = workflow.clone();
        updated.add_step(step);

        self.append_event(
            Event::new(
                EventPayload::WorkflowDefinitionUpdated {
                    definition: updated,
                },
                CorrelationIds::for_workflow(workflow.project_id, workflow.id),
            ),
            "registry:workflow_add_step",
        )?;

        self.get_workflow(&workflow.id.to_string())
    }

    fn normalize_child_workflow_config(
        &self,
        workflow: &WorkflowDefinition,
        kind: WorkflowStepKind,
        child_workflow_id: Option<&str>,
        origin: &'static str,
    ) -> Result<Option<WorkflowChildConfig>> {
        let Some(child_workflow_id) = child_workflow_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            if kind == WorkflowStepKind::Workflow {
                return Err(HivemindError::user(
                    "workflow_child_workflow_required",
                    "Workflow steps require an explicit child workflow id",
                    origin,
                ));
            }
            return Ok(None);
        };
        if kind != WorkflowStepKind::Workflow {
            return Err(HivemindError::user(
                "workflow_child_workflow_invalid_kind",
                "Only workflow steps may declare a child workflow id",
                origin,
            ));
        }
        let child = self.get_workflow(child_workflow_id)?;
        if child.project_id != workflow.project_id {
            return Err(HivemindError::user(
                "workflow_child_project_mismatch",
                "Child workflow must belong to the same project as the parent workflow",
                origin,
            ));
        }
        if child.id == workflow.id {
            return Err(HivemindError::user(
                "workflow_child_self_reference",
                "Workflow step cannot launch its own definition as a direct child",
                origin,
            ));
        }
        Ok(Some(WorkflowChildConfig {
            workflow_id: child.id,
            failure_policy: Default::default(),
        }))
    }

    fn normalize_selector(
        selector: WorkflowBagSelector,
        origin: &'static str,
    ) -> Result<WorkflowBagSelector> {
        let output_name = selector.output_name.trim().to_string();
        if output_name.is_empty() {
            return Err(HivemindError::user(
                "invalid_workflow_output_selector",
                "Workflow bag selector output_name cannot be empty",
                origin,
            ));
        }
        if selector.expected_schema_version == Some(0) {
            return Err(HivemindError::user(
                "invalid_workflow_output_selector_schema_version",
                "Workflow bag selector expected_schema_version must be >= 1",
                origin,
            ));
        }
        Ok(WorkflowBagSelector {
            output_name,
            producer_step_ids: selector.producer_step_ids,
            tags: selector.tags,
            expected_schema: selector
                .expected_schema
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            expected_schema_version: selector.expected_schema_version,
        })
    }

    fn normalize_workflow_conditional_config(
        kind: WorkflowStepKind,
        conditional: Option<WorkflowConditionalConfig>,
        origin: &'static str,
    ) -> Result<Option<WorkflowConditionalConfig>> {
        let Some(conditional) = conditional else {
            if kind == WorkflowStepKind::Conditional {
                return Err(HivemindError::user(
                    "workflow_conditional_config_required",
                    "Conditional steps require explicit conditional config",
                    origin,
                ));
            }
            return Ok(None);
        };
        if kind != WorkflowStepKind::Conditional {
            return Err(HivemindError::user(
                "workflow_conditional_config_invalid_kind",
                "Conditional config is allowed only on conditional steps",
                origin,
            ));
        }
        if conditional.branches.is_empty() {
            return Err(HivemindError::user(
                "workflow_conditional_branch_required",
                "Conditional steps require at least one branch",
                origin,
            ));
        }
        let mut names = BTreeSet::new();
        let mut branches = Vec::with_capacity(conditional.branches.len());
        for branch in conditional.branches {
            let name = branch.name.trim().to_string();
            if name.is_empty() {
                return Err(HivemindError::user(
                    "workflow_conditional_branch_invalid",
                    "Conditional branch name cannot be empty",
                    origin,
                ));
            }
            if !names.insert(name.clone()) {
                return Err(HivemindError::user(
                    "workflow_conditional_branch_duplicate",
                    format!("Conditional branch '{name}' is duplicated"),
                    origin,
                ));
            }
            let condition = match branch.condition {
                WorkflowConditionExpression::ContextKeyExists { key } => {
                    let key = key.trim().to_string();
                    if key.is_empty() {
                        return Err(HivemindError::user(
                            "workflow_condition_invalid_key",
                            "Conditional context key cannot be empty",
                            origin,
                        ));
                    }
                    WorkflowConditionExpression::ContextKeyExists { key }
                }
                WorkflowConditionExpression::ContextValueEquals { key, value } => {
                    let key = key.trim().to_string();
                    if key.is_empty() {
                        return Err(HivemindError::user(
                            "workflow_condition_invalid_key",
                            "Conditional context key cannot be empty",
                            origin,
                        ));
                    }
                    WorkflowConditionExpression::ContextValueEquals {
                        key,
                        value: Self::normalize_workflow_value(value, origin)?,
                    }
                }
                WorkflowConditionExpression::BagCountAtLeast { selector, count } => {
                    if count == 0 {
                        return Err(HivemindError::user(
                            "workflow_condition_invalid_count",
                            "Conditional bag count must be >= 1",
                            origin,
                        ));
                    }
                    WorkflowConditionExpression::BagCountAtLeast {
                        selector: Self::normalize_selector(selector, origin)?,
                        count,
                    }
                }
                WorkflowConditionExpression::BagValueEquals {
                    selector,
                    reducer,
                    value,
                } => WorkflowConditionExpression::BagValueEquals {
                    selector: Self::normalize_selector(selector, origin)?,
                    reducer,
                    value: Self::normalize_workflow_value(value, origin)?,
                },
            };
            branches.push(WorkflowConditionalBranch {
                name,
                condition,
                activate_step_ids: branch.activate_step_ids,
            });
        }
        let default_branch = conditional
            .default_branch
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(default_branch) = &default_branch {
            if !names.contains(default_branch) {
                return Err(HivemindError::user(
                    "workflow_conditional_default_branch_unknown",
                    format!("Default branch '{default_branch}' does not exist"),
                    origin,
                ));
            }
        }
        Ok(Some(WorkflowConditionalConfig {
            branches,
            default_branch,
        }))
    }

    fn normalize_workflow_wait_config(
        kind: WorkflowStepKind,
        wait: Option<WorkflowWaitConfig>,
        origin: &'static str,
    ) -> Result<Option<WorkflowWaitConfig>> {
        let Some(wait) = wait else {
            if kind == WorkflowStepKind::Wait {
                return Err(HivemindError::user(
                    "workflow_wait_config_required",
                    "Wait steps require explicit wait config",
                    origin,
                ));
            }
            return Ok(None);
        };
        if kind != WorkflowStepKind::Wait {
            return Err(HivemindError::user(
                "workflow_wait_config_invalid_kind",
                "Wait config is allowed only on wait steps",
                origin,
            ));
        }
        let condition = match wait.condition {
            WorkflowWaitCondition::Signal {
                signal_name,
                payload_schema,
                payload_schema_version,
            } => {
                let signal_name = signal_name.trim().to_string();
                if signal_name.is_empty() {
                    return Err(HivemindError::user(
                        "workflow_signal_name_invalid",
                        "Signal wait requires a non-empty signal name",
                        origin,
                    ));
                }
                if payload_schema_version == Some(0) {
                    return Err(HivemindError::user(
                        "workflow_signal_schema_version_invalid",
                        "Signal payload schema version must be >= 1",
                        origin,
                    ));
                }
                WorkflowWaitCondition::Signal {
                    signal_name,
                    payload_schema: payload_schema.filter(|value| !value.trim().is_empty()),
                    payload_schema_version,
                }
            }
            WorkflowWaitCondition::Timer { duration_secs } => {
                if duration_secs == 0 {
                    return Err(HivemindError::user(
                        "workflow_wait_duration_invalid",
                        "Timer waits require duration_secs >= 1",
                        origin,
                    ));
                }
                WorkflowWaitCondition::Timer { duration_secs }
            }
            WorkflowWaitCondition::HumanSignal { signal_name } => {
                let signal_name = signal_name.trim().to_string();
                if signal_name.is_empty() {
                    return Err(HivemindError::user(
                        "workflow_signal_name_invalid",
                        "Human-signal wait requires a non-empty signal name",
                        origin,
                    ));
                }
                WorkflowWaitCondition::HumanSignal { signal_name }
            }
        };
        Ok(Some(WorkflowWaitConfig { condition }))
    }

    pub fn list_workflows(
        &self,
        project_id_or_name: Option<&str>,
    ) -> Result<Vec<WorkflowDefinition>> {
        let state = self.state()?;
        let project_filter = project_id_or_name
            .map(|value| self.get_project(value))
            .transpose()?;
        let mut workflows: Vec<_> = state
            .workflows
            .into_values()
            .filter(|workflow| {
                project_filter
                    .as_ref()
                    .is_none_or(|project| workflow.project_id == project.id)
            })
            .collect();
        workflows.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        Ok(workflows)
    }

    pub fn get_workflow(&self, workflow_id_or_name: &str) -> Result<WorkflowDefinition> {
        let state = self.state()?;
        if let Ok(id) = Uuid::parse_str(workflow_id_or_name) {
            if let Some(workflow) = state.workflows.get(&id) {
                return Ok(workflow.clone());
            }
        }

        let matches: Vec<_> = state
            .workflows
            .values()
            .filter(|workflow| workflow.name == workflow_id_or_name)
            .cloned()
            .collect();
        match matches.as_slice() {
            [workflow] => Ok(workflow.clone()),
            [] => Err(HivemindError::user(
                "workflow_not_found",
                format!("Workflow '{workflow_id_or_name}' not found"),
                "registry:get_workflow",
            )
            .with_hint("Use 'hivemind workflow list' to inspect available workflows")),
            _ => Err(HivemindError::user(
                "workflow_name_ambiguous",
                format!("Workflow name '{workflow_id_or_name}' matches multiple workflows"),
                "registry:get_workflow",
            )
            .with_hint("Use the workflow UUID instead of the name")),
        }
    }

    fn normalize_workflow_value(
        value: WorkflowDataValue,
        origin: &'static str,
    ) -> Result<WorkflowDataValue> {
        value
            .normalized()
            .map_err(|err| HivemindError::user("invalid_workflow_value", err.to_string(), origin))
    }

    fn normalize_workflow_context_inputs(
        context_inputs: BTreeMap<String, WorkflowDataValue>,
        origin: &'static str,
    ) -> Result<BTreeMap<String, WorkflowDataValue>> {
        let mut normalized = BTreeMap::new();
        for (key, value) in context_inputs {
            let key = key.trim().to_string();
            if key.is_empty() {
                return Err(HivemindError::user(
                    "invalid_workflow_context_key",
                    "Workflow context key cannot be empty",
                    origin,
                ));
            }
            normalized.insert(key, Self::normalize_workflow_value(value, origin)?);
        }
        Ok(normalized)
    }

    fn normalize_workflow_input_bindings(
        input_bindings: Vec<WorkflowStepInputBinding>,
        origin: &'static str,
    ) -> Result<Vec<WorkflowStepInputBinding>> {
        let mut names = BTreeSet::new();
        let mut normalized = Vec::with_capacity(input_bindings.len());
        for binding in input_bindings {
            let name = binding.name.trim().to_string();
            if name.is_empty() {
                return Err(HivemindError::user(
                    "invalid_workflow_step_input_name",
                    "Workflow step input binding name cannot be empty",
                    origin,
                ));
            }
            if !names.insert(name.clone()) {
                return Err(HivemindError::user(
                    "workflow_step_input_duplicate",
                    format!("Workflow step input binding '{name}' is duplicated"),
                    origin,
                ));
            }

            let source = match binding.source {
                WorkflowStepInputSource::ContextKey { key } => {
                    let key = key.trim().to_string();
                    if key.is_empty() {
                        return Err(HivemindError::user(
                            "invalid_workflow_context_key",
                            "Workflow context key cannot be empty",
                            origin,
                        ));
                    }
                    WorkflowStepInputSource::ContextKey { key }
                }
                WorkflowStepInputSource::Literal { value } => WorkflowStepInputSource::Literal {
                    value: Self::normalize_workflow_value(value, origin)?,
                },
                WorkflowStepInputSource::Bag { selector, reducer } => {
                    let output_name = selector.output_name.trim().to_string();
                    if output_name.is_empty() {
                        return Err(HivemindError::user(
                            "invalid_workflow_output_selector",
                            "Workflow bag selector output_name cannot be empty",
                            origin,
                        ));
                    }
                    if selector.expected_schema_version == Some(0) {
                        return Err(HivemindError::user(
                            "invalid_workflow_output_selector_schema_version",
                            "Workflow bag selector expected_schema_version must be >= 1",
                            origin,
                        ));
                    }
                    WorkflowStepInputSource::Bag {
                        selector: WorkflowBagSelector {
                            output_name,
                            producer_step_ids: selector.producer_step_ids,
                            tags: selector.tags,
                            expected_schema: selector
                                .expected_schema
                                .map(|value| value.trim().to_string())
                                .filter(|value| !value.is_empty()),
                            expected_schema_version: selector.expected_schema_version,
                        },
                        reducer,
                    }
                }
            };

            normalized.push(WorkflowStepInputBinding { name, source });
        }
        Ok(normalized)
    }

    fn normalize_workflow_output_bindings(
        kind: WorkflowStepKind,
        output_bindings: Vec<WorkflowStepOutputBinding>,
        input_bindings: &[WorkflowStepInputBinding],
        origin: &'static str,
    ) -> Result<Vec<WorkflowStepOutputBinding>> {
        let available_bindings: BTreeSet<_> = input_bindings
            .iter()
            .map(|binding| binding.name.as_str())
            .collect();
        let mut names = BTreeSet::new();
        let mut normalized = Vec::with_capacity(output_bindings.len());
        for binding in output_bindings {
            let name = binding.name.trim().to_string();
            if name.is_empty() {
                return Err(HivemindError::user(
                    "invalid_workflow_output_name",
                    "Workflow output binding name cannot be empty",
                    origin,
                ));
            }
            if !names.insert(name.clone()) {
                return Err(HivemindError::user(
                    "workflow_output_duplicate",
                    format!("Workflow output binding '{name}' is duplicated"),
                    origin,
                ));
            }
            let source = match binding.source {
                WorkflowValueSource::Literal { value } => WorkflowValueSource::Literal {
                    value: Self::normalize_workflow_value(value, origin)?,
                },
                WorkflowValueSource::InputBinding { binding } => {
                    if !available_bindings.contains(binding.as_str()) {
                        return Err(HivemindError::user(
                            "workflow_output_binding_not_found",
                            format!("Workflow output binding references unknown input '{binding}'"),
                            origin,
                        ));
                    }
                    WorkflowValueSource::InputBinding { binding }
                }
                WorkflowValueSource::ChildContextKey { key } => {
                    if kind != WorkflowStepKind::Workflow {
                        return Err(HivemindError::user(
                            "workflow_child_context_output_invalid_kind",
                            "Child context output bindings are allowed only on workflow steps",
                            origin,
                        ));
                    }
                    WorkflowValueSource::ChildContextKey {
                        key: key.trim().to_string(),
                    }
                }
            };
            normalized.push(WorkflowStepOutputBinding {
                name,
                source,
                tags: binding.tags,
            });
        }
        Ok(normalized)
    }

    fn normalize_workflow_context_patches(
        kind: WorkflowStepKind,
        context_patches: Vec<WorkflowContextPatchBinding>,
        input_bindings: &[WorkflowStepInputBinding],
        origin: &'static str,
    ) -> Result<Vec<WorkflowContextPatchBinding>> {
        if !matches!(kind, WorkflowStepKind::Join | WorkflowStepKind::Workflow)
            && !context_patches.is_empty()
        {
            return Err(HivemindError::user(
                "workflow_context_patch_requires_join_step",
                "Workflow context patches are allowed only on join or workflow steps",
                origin,
            ));
        }

        let available_bindings: BTreeSet<_> = input_bindings
            .iter()
            .map(|binding| binding.name.as_str())
            .collect();
        let mut keys = BTreeSet::new();
        let mut normalized = Vec::with_capacity(context_patches.len());
        for patch in context_patches {
            let key = patch.key.trim().to_string();
            if key.is_empty() {
                return Err(HivemindError::user(
                    "invalid_workflow_context_key",
                    "Workflow context key cannot be empty",
                    origin,
                ));
            }
            if !keys.insert(key.clone()) {
                return Err(HivemindError::user(
                    "workflow_context_patch_duplicate",
                    format!("Workflow context patch for key '{key}' is duplicated"),
                    origin,
                ));
            }
            let source = match patch.source {
                WorkflowValueSource::Literal { value } => WorkflowValueSource::Literal {
                    value: Self::normalize_workflow_value(value, origin)?,
                },
                WorkflowValueSource::InputBinding { binding } => {
                    if !available_bindings.contains(binding.as_str()) {
                        return Err(HivemindError::user(
                            "workflow_context_patch_binding_not_found",
                            format!("Workflow context patch references unknown input '{binding}'"),
                            origin,
                        ));
                    }
                    WorkflowValueSource::InputBinding { binding }
                }
                WorkflowValueSource::ChildContextKey { key } => {
                    if kind != WorkflowStepKind::Workflow {
                        return Err(HivemindError::user(
                            "workflow_child_context_patch_invalid_kind",
                            "Child context patches are allowed only on workflow steps",
                            origin,
                        ));
                    }
                    WorkflowValueSource::ChildContextKey {
                        key: key.trim().to_string(),
                    }
                }
            };
            normalized.push(WorkflowContextPatchBinding { key, source });
        }
        Ok(normalized)
    }

    pub fn create_workflow_run(
        &self,
        workflow_id_or_name: &str,
        context_schema: Option<&str>,
        context_schema_version: Option<u32>,
        context_inputs: BTreeMap<String, WorkflowDataValue>,
    ) -> Result<WorkflowRun> {
        let workflow = self
            .get_workflow(workflow_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.create_workflow_run_internal(
            &workflow,
            context_schema,
            context_schema_version,
            context_inputs,
            None,
            None,
            None,
            "registry:create_workflow_run",
        )
    }

    fn create_workflow_run_internal(
        &self,
        workflow: &WorkflowDefinition,
        context_schema: Option<&str>,
        context_schema_version: Option<u32>,
        context_inputs: BTreeMap<String, WorkflowDataValue>,
        root_workflow_run_id: Option<Uuid>,
        parent_workflow_run_id: Option<Uuid>,
        parent_step_id: Option<Uuid>,
        origin: &'static str,
    ) -> Result<WorkflowRun> {
        let run = match (root_workflow_run_id, parent_workflow_run_id, parent_step_id) {
            (Some(root), Some(parent_run), Some(parent_step)) => {
                WorkflowRun::new_child(workflow, root, parent_run, parent_step)
            }
            _ => WorkflowRun::new_root(workflow),
        };
        let schema = context_schema.unwrap_or("workflow_context").trim();
        if schema.is_empty() {
            let err = HivemindError::user(
                "invalid_workflow_context_schema",
                "Workflow context schema cannot be empty",
                origin,
            );
            self.record_error_event(&err, Self::workflow_run_corr(&run));
            return Err(err);
        }
        let schema_version = context_schema_version.unwrap_or(1);
        if schema_version == 0 {
            let err = HivemindError::user(
                "invalid_workflow_context_schema_version",
                "Workflow context schema version must be >= 1",
                origin,
            );
            self.record_error_event(&err, Self::workflow_run_corr(&run));
            return Err(err);
        }
        let normalized_inputs = Self::normalize_workflow_context_inputs(context_inputs, origin)?;
        let context = WorkflowContextState::initialize(
            schema.to_string(),
            schema_version,
            normalized_inputs,
            Utc::now(),
            "workflow_run_initialized",
        );
        self.append_event(
            Event::new(
                EventPayload::WorkflowRunCreated { run: run.clone() },
                Self::workflow_run_corr(&run),
            ),
            origin,
        )?;
        self.append_event(
            Event::new(
                EventPayload::WorkflowContextInitialized {
                    workflow_run_id: run.id,
                    context: context.clone(),
                },
                Self::workflow_run_corr(&run),
            ),
            origin,
        )?;
        self.append_event(
            Event::new(
                EventPayload::WorkflowContextSnapshotCaptured {
                    workflow_run_id: run.id,
                    snapshot: context.current_snapshot.clone(),
                },
                Self::workflow_run_corr(&run),
            ),
            origin,
        )?;
        self.get_workflow_run(&run.id.to_string())
    }

    pub fn list_workflow_runs(
        &self,
        project_id_or_name: Option<&str>,
        workflow_id_or_name: Option<&str>,
    ) -> Result<Vec<WorkflowRun>> {
        let state = self.state()?;
        let project_filter = project_id_or_name
            .map(|value| self.get_project(value))
            .transpose()?;
        let workflow_filter = workflow_id_or_name
            .map(|value| self.get_workflow(value))
            .transpose()?;

        let mut runs: Vec<_> = state
            .workflow_runs
            .into_values()
            .filter(|run| {
                project_filter
                    .as_ref()
                    .is_none_or(|project| run.project_id == project.id)
            })
            .filter(|run| {
                workflow_filter
                    .as_ref()
                    .is_none_or(|workflow| run.workflow_id == workflow.id)
            })
            .collect();
        runs.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(runs)
    }

    pub fn get_workflow_run(&self, workflow_run_id: &str) -> Result<WorkflowRun> {
        let workflow_run_id = Uuid::parse_str(workflow_run_id).map_err(|_| {
            HivemindError::user(
                "invalid_workflow_run_id",
                format!("Invalid workflow run id '{workflow_run_id}'"),
                "registry:get_workflow_run",
            )
            .with_hint("Provide a workflow run UUID")
        })?;
        self.state()?
            .workflow_runs
            .get(&workflow_run_id)
            .cloned()
            .ok_or_else(|| {
                HivemindError::user(
                    "workflow_run_not_found",
                    format!("Workflow run '{workflow_run_id}' not found"),
                    "registry:get_workflow_run",
                )
                .with_hint("Use 'hivemind workflow run-list' to inspect available workflow runs")
            })
    }

    pub fn start_workflow_run(&self, workflow_run_id: &str) -> Result<WorkflowRun> {
        self.transition_workflow_run(workflow_run_id, WorkflowRunState::Running, None, false)
    }

    fn child_runs_for_parent(&self, workflow_run_id: Uuid) -> Result<Vec<WorkflowRun>> {
        let mut children = self
            .state()?
            .workflow_runs
            .values()
            .filter(|run| run.parent_workflow_run_id == Some(workflow_run_id))
            .cloned()
            .collect::<Vec<_>>();
        children.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(children)
    }

    pub fn tick_workflow_run(
        &self,
        workflow_run_id: &str,
        interactive: bool,
        max_parallel: Option<u16>,
    ) -> Result<WorkflowRun> {
        let mut run = self
            .get_workflow_run(workflow_run_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        if run.state != WorkflowRunState::Running {
            let err = HivemindError::user(
                "workflow_run_not_running",
                format!("Workflow run '{}' is not in running state", run.id),
                "registry:tick_workflow_run",
            )
            .with_hint("Start or resume the workflow run before ticking it");
            self.record_error_event(
                &err,
                CorrelationIds::for_workflow_run(run.project_id, run.workflow_id, run.id),
            );
            return Err(err);
        }

        let workflow = self.get_workflow(&run.workflow_id.to_string())?;
        if workflow.steps.is_empty() {
            return self.complete_workflow_run(workflow_run_id);
        }
        let _ = self.apply_workflow_dependency_releases(workflow_run_id, &workflow)?;
        self.process_ready_conditional_steps(
            workflow_run_id,
            &workflow,
            "registry:tick_workflow_run",
        )?;
        self.process_wait_steps(workflow_run_id, &workflow, "registry:tick_workflow_run")?;
        let _ = self.process_workflow_child_steps(
            workflow_run_id,
            &workflow,
            "registry:tick_workflow_run",
        )?;
        let _ = self.apply_workflow_dependency_releases(workflow_run_id, &workflow)?;
        self.process_ready_conditional_steps(
            workflow_run_id,
            &workflow,
            "registry:tick_workflow_run",
        )?;
        self.process_wait_steps(workflow_run_id, &workflow, "registry:tick_workflow_run")?;
        run = self.get_workflow_run(workflow_run_id)?;
        let has_task_bridge_steps = workflow
            .steps
            .values()
            .any(|step| matches!(step.kind, WorkflowStepKind::Task | WorkflowStepKind::Join));
        if !has_task_bridge_steps {
            if run.state == WorkflowRunState::Running && run.can_complete() {
                return self.complete_workflow_run(workflow_run_id);
            }
            return Ok(run);
        }

        let flow_id = self.ensure_synthetic_flow_for_workflow_run(&run, &workflow)?;
        let flow_id_str = flow_id.to_string();
        let flow = self.get_flow(&flow_id_str)?;
        match flow.state {
            FlowState::Created => {
                self.start_flow(&flow_id_str)?;
            }
            FlowState::Paused => {
                self.resume_flow(&flow_id_str)?;
            }
            FlowState::Running | FlowState::Completed => {}
            FlowState::FrozenForMerge | FlowState::Merged | FlowState::Aborted => {
                return self.sync_workflow_run_from_synthetic_flow(workflow_run_id, &workflow);
            }
        }
        self.process_ready_join_steps(workflow_run_id, &workflow, "registry:tick_workflow_run")?;

        let tick_result = self.tick_flow(&flow_id_str, interactive, max_parallel);
        let _ = self.sync_workflow_run_from_synthetic_flow(workflow_run_id, &workflow)?;
        let _ = self.apply_workflow_dependency_releases(workflow_run_id, &workflow)?;
        self.process_ready_conditional_steps(
            workflow_run_id,
            &workflow,
            "registry:tick_workflow_run",
        )?;
        self.process_wait_steps(workflow_run_id, &workflow, "registry:tick_workflow_run")?;
        let _ = self.process_workflow_child_steps(
            workflow_run_id,
            &workflow,
            "registry:tick_workflow_run",
        )?;
        self.process_ready_join_steps(workflow_run_id, &workflow, "registry:tick_workflow_run")?;
        self.process_ready_conditional_steps(
            workflow_run_id,
            &workflow,
            "registry:tick_workflow_run",
        )?;
        self.process_wait_steps(workflow_run_id, &workflow, "registry:tick_workflow_run")?;
        let synced = self.sync_workflow_run_from_synthetic_flow(workflow_run_id, &workflow)?;
        tick_result.map(|_| synced)
    }

    pub fn complete_workflow_run(&self, workflow_run_id: &str) -> Result<WorkflowRun> {
        let run = self
            .get_workflow_run(workflow_run_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        if !run.can_complete() {
            let err = HivemindError::user(
                "workflow_run_not_completeable",
                format!("Workflow run '{}' still has non-terminal steps", run.id),
                "registry:complete_workflow_run",
            )
            .with_hint("Move all workflow steps into terminal states before completing the run");
            self.record_error_event(
                &err,
                CorrelationIds::for_workflow_run(run.project_id, run.workflow_id, run.id),
            );
            return Err(err);
        }
        self.transition_workflow_run(workflow_run_id, WorkflowRunState::Completed, None, false)
    }

    pub fn pause_workflow_run(&self, workflow_run_id: &str) -> Result<WorkflowRun> {
        let run = self.get_workflow_run(workflow_run_id)?;
        let flow_id = Self::workflow_bridge_flow_id(run.id);
        if let Some(flow) = self.state()?.flows.get(&flow_id).cloned() {
            match flow.state {
                FlowState::Running => {
                    self.pause_flow(&flow_id.to_string())?;
                }
                FlowState::Created | FlowState::Paused => {}
                FlowState::Completed
                | FlowState::FrozenForMerge
                | FlowState::Merged
                | FlowState::Aborted => {
                    let workflow = self.get_workflow(&run.workflow_id.to_string())?;
                    return self.sync_workflow_run_from_synthetic_flow(workflow_run_id, &workflow);
                }
            }
        }
        let paused =
            self.transition_workflow_run(workflow_run_id, WorkflowRunState::Paused, None, false)?;
        for child in self.child_runs_for_parent(paused.id)? {
            if !child.state.is_terminal() && child.state != WorkflowRunState::Paused {
                let _ = self.pause_workflow_run(&child.id.to_string())?;
            }
        }
        Ok(paused)
    }

    pub fn resume_workflow_run(&self, workflow_run_id: &str) -> Result<WorkflowRun> {
        let run = self.get_workflow_run(workflow_run_id)?;
        let flow_id = Self::workflow_bridge_flow_id(run.id);
        if let Some(flow) = self.state()?.flows.get(&flow_id).cloned() {
            match flow.state {
                FlowState::Paused => {
                    self.resume_flow(&flow_id.to_string())?;
                }
                FlowState::Created | FlowState::Running => {}
                FlowState::Completed
                | FlowState::FrozenForMerge
                | FlowState::Merged
                | FlowState::Aborted => {
                    let workflow = self.get_workflow(&run.workflow_id.to_string())?;
                    return self.sync_workflow_run_from_synthetic_flow(workflow_run_id, &workflow);
                }
            }
        }
        let resumed =
            self.transition_workflow_run(workflow_run_id, WorkflowRunState::Running, None, false)?;
        for child in self.child_runs_for_parent(resumed.id)? {
            if child.state == WorkflowRunState::Paused {
                let _ = self.resume_workflow_run(&child.id.to_string())?;
            }
        }
        Ok(resumed)
    }

    pub fn abort_workflow_run(
        &self,
        workflow_run_id: &str,
        reason: Option<&str>,
        forced: bool,
    ) -> Result<WorkflowRun> {
        let run = self.get_workflow_run(workflow_run_id)?;
        let flow_id = Self::workflow_bridge_flow_id(run.id);
        if let Some(flow) = self.state()?.flows.get(&flow_id).cloned() {
            match flow.state {
                FlowState::Created | FlowState::Running | FlowState::Paused => {
                    self.abort_flow(&flow_id.to_string(), reason, forced)?;
                }
                FlowState::Aborted => {}
                FlowState::Completed | FlowState::FrozenForMerge | FlowState::Merged => {
                    let workflow = self.get_workflow(&run.workflow_id.to_string())?;
                    return self.sync_workflow_run_from_synthetic_flow(workflow_run_id, &workflow);
                }
            }
        }
        let aborted = self.transition_workflow_run(
            workflow_run_id,
            WorkflowRunState::Aborted,
            reason,
            forced,
        )?;
        for child in self.child_runs_for_parent(aborted.id)? {
            if !child.state.is_terminal() {
                let _ = self.abort_workflow_run(&child.id.to_string(), reason, forced)?;
            }
        }
        Ok(aborted)
    }

    pub fn workflow_runtime_stream_items_with_detail(
        &self,
        workflow_run_id: &str,
        attempt_id: Option<&str>,
        limit: usize,
        detail: RuntimeStreamDetailLevel,
    ) -> Result<Vec<RuntimeStreamItemView>> {
        let (run, _workflow, flow) = self.workflow_run_bridge_flow(workflow_run_id)?;
        let Some(flow) = flow else {
            return Ok(Vec::new());
        };
        let _ = run;
        self.runtime_stream_items_with_detail(Some(&flow.id.to_string()), attempt_id, limit, detail)
    }

    pub fn workflow_merge_prepare(
        &self,
        workflow_run_id: &str,
        target_branch: Option<&str>,
    ) -> Result<crate::core::state::MergeState> {
        let (run, _workflow, flow) = self.workflow_run_bridge_flow(workflow_run_id)?;
        let flow_id = flow
            .map(|flow| flow.id)
            .unwrap_or_else(|| Self::workflow_bridge_flow_uuid(run.id));
        self.merge_prepare(&flow_id.to_string(), target_branch)
    }

    pub fn workflow_merge_approve(
        &self,
        workflow_run_id: &str,
    ) -> Result<crate::core::state::MergeState> {
        let (run, _workflow, flow) = self.workflow_run_bridge_flow(workflow_run_id)?;
        let flow_id = flow
            .map(|flow| flow.id)
            .unwrap_or_else(|| Self::workflow_bridge_flow_uuid(run.id));
        self.merge_approve(&flow_id.to_string())
    }

    pub fn workflow_merge_execute_with_options(
        &self,
        workflow_run_id: &str,
        options: MergeExecuteOptions,
    ) -> Result<crate::core::state::MergeState> {
        let (run, _workflow, flow) = self.workflow_run_bridge_flow(workflow_run_id)?;
        let flow_id = flow
            .map(|flow| flow.id)
            .unwrap_or_else(|| Self::workflow_bridge_flow_uuid(run.id));
        self.merge_execute_with_options(&flow_id.to_string(), options)
    }

    pub fn signal_workflow_run(
        &self,
        workflow_run_id: &str,
        signal_name: &str,
        idempotency_key: &str,
        payload: Option<WorkflowDataValue>,
        step_id: Option<&str>,
        emitted_by: &str,
    ) -> Result<WorkflowRun> {
        let run = self.get_workflow_run(workflow_run_id)?;
        let signal_name = signal_name.trim();
        let idempotency_key = idempotency_key.trim();
        if signal_name.is_empty() || idempotency_key.is_empty() {
            return Err(HivemindError::user(
                "workflow_signal_invalid",
                "Signal name and idempotency key must be non-empty",
                "registry:signal_workflow_run",
            ));
        }
        if run
            .signals
            .iter()
            .any(|signal| signal.idempotency_key == idempotency_key)
        {
            return Err(HivemindError::user(
                "workflow_signal_duplicate",
                format!("Signal idempotency key '{idempotency_key}' has already been used"),
                "registry:signal_workflow_run",
            ));
        }
        let step_uuid = step_id.map(Uuid::parse_str).transpose().map_err(|_| {
            HivemindError::user(
                "invalid_workflow_step_id",
                "Signal step id must be a valid UUID",
                "registry:signal_workflow_run",
            )
        })?;
        let signal = WorkflowSignal {
            signal_name: signal_name.to_string(),
            idempotency_key: idempotency_key.to_string(),
            payload: payload
                .map(|value| Self::normalize_workflow_value(value, "registry:signal_workflow_run"))
                .transpose()?,
            step_id: step_uuid,
            emitted_at: Utc::now(),
            emitted_by: emitted_by.to_string(),
        };
        self.append_event(
            Event::new(
                EventPayload::WorkflowSignalReceived {
                    workflow_run_id: run.id,
                    signal,
                },
                CorrelationIds::for_workflow_run(run.project_id, run.workflow_id, run.id),
            ),
            "registry:signal_workflow_run",
        )?;
        let workflow = self.get_workflow(&run.workflow_id.to_string())?;
        self.process_wait_steps(workflow_run_id, &workflow, "registry:signal_workflow_run")?;
        self.apply_workflow_dependency_releases(workflow_run_id, &workflow)
    }

    pub fn workflow_step_set_state(
        &self,
        workflow_run_id: &str,
        step_id: &str,
        state: WorkflowStepState,
        reason: Option<&str>,
    ) -> Result<WorkflowRun> {
        let run = self
            .get_workflow_run(workflow_run_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let step_id = Uuid::parse_str(step_id).map_err(|_| {
            HivemindError::user(
                "invalid_workflow_step_id",
                format!("Invalid workflow step id '{step_id}'"),
                "registry:workflow_step_set_state",
            )
        })?;
        let step_run = run.step_runs.get(&step_id).ok_or_else(|| {
            HivemindError::user(
                "workflow_step_not_found",
                format!("Workflow step '{step_id}' not found in run '{}'", run.id),
                "registry:workflow_step_set_state",
            )
        })?;
        if !step_run.state.can_transition_to(state) {
            let err = HivemindError::user(
                "invalid_workflow_step_transition",
                format!(
                    "Cannot transition workflow step from {:?} to {state:?}",
                    step_run.state
                ),
                "registry:workflow_step_set_state",
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_workflow_step(
                    run.project_id,
                    run.workflow_id,
                    run.id,
                    run.root_workflow_run_id,
                    run.parent_workflow_run_id,
                    step_id,
                    step_run.id,
                ),
            );
            return Err(err);
        }

        self.append_event(
            Event::new(
                EventPayload::WorkflowStepStateChanged {
                    workflow_run_id: run.id,
                    step_id,
                    step_run_id: step_run.id,
                    state,
                    reason: reason.map(str::to_string),
                },
                CorrelationIds::for_workflow_step(
                    run.project_id,
                    run.workflow_id,
                    run.id,
                    run.root_workflow_run_id,
                    run.parent_workflow_run_id,
                    step_id,
                    step_run.id,
                ),
            ),
            "registry:workflow_step_set_state",
        )?;

        self.get_workflow_run(workflow_run_id)
    }

    fn workflow_bridge_graph_id(run_id: Uuid) -> Uuid {
        Self::workflow_bridge_uuid(&format!("workflow-graph:{run_id}"))
    }

    fn workflow_bridge_flow_id(run_id: Uuid) -> Uuid {
        Self::workflow_bridge_uuid(&format!("workflow-flow:{run_id}"))
    }

    pub(crate) fn workflow_bridge_task_id(run_id: Uuid, step_id: Uuid) -> Uuid {
        Self::workflow_bridge_uuid(&format!("workflow-task:{run_id}:{step_id}"))
    }

    fn workflow_bridge_run_for_flow_state(state: &AppState, flow_id: Uuid) -> Option<&WorkflowRun> {
        state
            .workflow_runs
            .values()
            .find(|run| Self::workflow_bridge_flow_id(run.id) == flow_id)
    }

    pub(crate) fn workflow_bridge_step_for_task(
        run: &WorkflowRun,
        task_id: Uuid,
    ) -> Option<(Uuid, Uuid)> {
        run.step_runs.iter().find_map(|(step_id, step_run)| {
            (Self::workflow_bridge_task_id(run.id, *step_id) == task_id)
                .then_some((*step_id, step_run.id))
        })
    }

    fn child_workflow_run_for_step(
        state: &AppState,
        run: &WorkflowRun,
        step_id: Uuid,
    ) -> Option<WorkflowRun> {
        state
            .workflow_runs
            .values()
            .filter(|child| {
                child.parent_workflow_run_id == Some(run.id)
                    && child.parent_step_id == Some(step_id)
            })
            .max_by_key(|child| child.created_at)
            .cloned()
    }

    pub(crate) fn correlation_for_flow_event(state: &AppState, flow: &TaskFlow) -> CorrelationIds {
        Self::workflow_bridge_run_for_flow_state(state, flow.id).map_or_else(
            || CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            |run| Self::workflow_bridge_corr(run, flow.graph_id, Some(flow.id), None, None, None),
        )
    }

    pub(crate) fn correlation_for_graph_flow_id_event(
        state: &AppState,
        project_id: Uuid,
        graph_id: Uuid,
        flow_id: Uuid,
    ) -> CorrelationIds {
        Self::workflow_bridge_run_for_flow_state(state, flow_id).map_or_else(
            || CorrelationIds::for_graph_flow(project_id, graph_id, flow_id),
            |run| Self::workflow_bridge_corr(run, graph_id, Some(flow_id), None, None, None),
        )
    }

    pub(crate) fn correlation_for_flow_task_event(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
    ) -> CorrelationIds {
        Self::workflow_bridge_run_for_flow_state(state, flow.id)
            .and_then(|run| {
                Self::workflow_bridge_step_for_task(run, task_id).map(|(step_id, step_run_id)| {
                    Self::workflow_bridge_corr(
                        run,
                        flow.graph_id,
                        Some(flow.id),
                        Some(task_id),
                        Some(step_id),
                        Some(step_run_id),
                    )
                })
            })
            .unwrap_or_else(|| {
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                )
            })
    }

    pub(crate) fn correlation_for_flow_task_attempt_event(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
    ) -> CorrelationIds {
        let mut corr = Self::correlation_for_flow_task_event(state, flow, task_id);
        corr.attempt_id = Some(attempt_id);
        corr
    }

    fn workflow_bridge_uuid(seed: &str) -> Uuid {
        let digest = Sha256::digest(seed.as_bytes());
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }

    fn workflow_bridge_corr(
        run: &WorkflowRun,
        graph_id: Uuid,
        flow_id: Option<Uuid>,
        task_id: Option<Uuid>,
        step_id: Option<Uuid>,
        step_run_id: Option<Uuid>,
    ) -> CorrelationIds {
        CorrelationIds {
            project_id: Some(run.project_id),
            graph_id: Some(graph_id),
            flow_id,
            workflow_id: Some(run.workflow_id),
            workflow_run_id: Some(run.id),
            root_workflow_run_id: Some(run.root_workflow_run_id),
            parent_workflow_run_id: run.parent_workflow_run_id,
            task_id,
            step_id,
            step_run_id,
            attempt_id: None,
        }
    }

    fn workflow_run_corr(run: &WorkflowRun) -> CorrelationIds {
        CorrelationIds {
            project_id: Some(run.project_id),
            graph_id: None,
            flow_id: None,
            workflow_id: Some(run.workflow_id),
            workflow_run_id: Some(run.id),
            root_workflow_run_id: Some(run.root_workflow_run_id),
            parent_workflow_run_id: run.parent_workflow_run_id,
            task_id: None,
            step_id: run.parent_step_id,
            step_run_id: None,
            attempt_id: None,
        }
    }

    pub(crate) fn workflow_bridge_flow_uuid(workflow_run_id: Uuid) -> Uuid {
        Self::workflow_bridge_flow_id(workflow_run_id)
    }

    pub(crate) fn workflow_run_bridge_flow(
        &self,
        workflow_run_id: &str,
    ) -> Result<(WorkflowRun, WorkflowDefinition, Option<TaskFlow>)> {
        let run = self.get_workflow_run(workflow_run_id)?;
        let workflow = self.get_workflow(&run.workflow_id.to_string())?;
        let flow = self
            .state()?
            .flows
            .get(&Self::workflow_bridge_flow_id(run.id))
            .cloned();
        Ok((run, workflow, flow))
    }

    fn workflow_bridge_graph_task(step: &WorkflowStepDefinition, task_id: Uuid) -> GraphTask {
        GraphTask {
            id: task_id,
            title: step.name.clone(),
            description: step.description.clone(),
            criteria: SuccessCriteria {
                description: step
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Complete workflow step '{}'", step.name)),
                checks: Vec::new(),
            },
            retry_policy: RetryPolicy {
                max_retries: 2,
                escalate_on_failure: false,
            },
            checkpoints: Vec::new(),
            scope: None,
        }
    }

    fn workflow_step_for_definition<'a>(
        workflow: &'a WorkflowDefinition,
        step_id: Uuid,
        origin: &'static str,
    ) -> Result<&'a WorkflowStepDefinition> {
        workflow.steps.get(&step_id).ok_or_else(|| {
            HivemindError::system(
                "workflow_step_not_found_in_definition",
                format!("Workflow step '{step_id}' missing from definition"),
                origin,
            )
        })
    }

    fn workflow_step_names(workflow: &WorkflowDefinition) -> BTreeMap<Uuid, String> {
        workflow
            .steps
            .values()
            .map(|step| (step.id, step.name.clone()))
            .collect()
    }

    fn select_workflow_output_entries(
        run: &WorkflowRun,
        selector: &WorkflowBagSelector,
        origin: &'static str,
    ) -> Result<Vec<WorkflowOutputBagEntry>> {
        let producer_order = selector
            .producer_step_ids
            .iter()
            .enumerate()
            .map(|(idx, step_id)| (*step_id, idx))
            .collect::<std::collections::HashMap<_, _>>();
        let mut entries = run
            .output_bag
            .entries
            .iter()
            .filter(|entry| entry.output_name == selector.output_name)
            .filter(|entry| {
                selector.producer_step_ids.is_empty()
                    || selector.producer_step_ids.contains(&entry.producer_step_id)
            })
            .filter(|entry| selector.tags.iter().all(|tag| entry.tags.contains(tag)))
            .cloned()
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| {
            producer_order
                .get(&a.producer_step_id)
                .cmp(&producer_order.get(&b.producer_step_id))
                .then_with(|| a.event_sequence.cmp(&b.event_sequence))
                .then_with(|| a.entry_id.cmp(&b.entry_id))
        });

        if let Some(expected_schema) = selector.expected_schema.as_ref() {
            if let Some(entry) = entries
                .iter()
                .find(|entry| entry.payload.schema != *expected_schema)
            {
                return Err(HivemindError::user(
                    "workflow_output_schema_mismatch",
                    format!(
                        "Workflow output '{}' expected schema '{}' but step '{}' produced '{}'",
                        selector.output_name,
                        expected_schema,
                        entry.producer_step_id,
                        entry.payload.schema
                    ),
                    origin,
                ));
            }
        }
        if let Some(expected_version) = selector.expected_schema_version {
            if let Some(entry) = entries
                .iter()
                .find(|entry| entry.payload.schema_version != expected_version)
            {
                return Err(HivemindError::user(
                    "workflow_output_schema_version_mismatch",
                    format!(
                        "Workflow output '{}' expected schema version '{}' but step '{}' produced '{}'",
                        selector.output_name,
                        expected_version,
                        entry.producer_step_id,
                        entry.payload.schema_version
                    ),
                    origin,
                ));
            }
        }

        Ok(entries)
    }

    fn resolve_workflow_step_inputs(
        &self,
        run: &WorkflowRun,
        workflow: &WorkflowDefinition,
        step: &WorkflowStepDefinition,
        origin: &'static str,
    ) -> Result<WorkflowStepContextSnapshot> {
        let step_run = run.step_runs.get(&step.id).ok_or_else(|| {
            HivemindError::system(
                "workflow_step_run_missing",
                format!(
                    "Workflow step run '{}' missing from workflow run '{}'",
                    step.id, run.id
                ),
                origin,
            )
        })?;
        let step_names = Self::workflow_step_names(workflow);
        let mut inputs = BTreeMap::new();
        let mut resolutions = Vec::with_capacity(step.input_bindings.len());

        for binding in &step.input_bindings {
            let (value, selected_entry_ids) = match &binding.source {
                WorkflowStepInputSource::ContextKey { key } => {
                    let value = run
                        .context
                        .current_snapshot
                        .values
                        .get(key)
                        .cloned()
                        .ok_or_else(|| {
                            HivemindError::user(
                                "workflow_context_key_missing",
                                format!(
                                    "Workflow step '{}' requires missing context key '{}'",
                                    step.name, key
                                ),
                                origin,
                            )
                        })?;
                    (value, Vec::new())
                }
                WorkflowStepInputSource::Literal { value } => (value.clone(), Vec::new()),
                WorkflowStepInputSource::Bag { selector, reducer } => {
                    let entries = Self::select_workflow_output_entries(run, selector, origin)?;
                    let selected_entry_ids = entries.iter().map(|entry| entry.entry_id).collect();
                    let reduced =
                        reducer
                            .reduce(selector, &entries, &step_names)
                            .map_err(|err| {
                                HivemindError::user(
                                    "workflow_output_reduce_failed",
                                    err.to_string(),
                                    origin,
                                )
                            })?;
                    (reduced, selected_entry_ids)
                }
            };

            inputs.insert(binding.name.clone(), value.clone());
            resolutions.push(WorkflowInputBindingResolution {
                binding: binding.name.clone(),
                source: binding.source.clone(),
                selected_entry_ids,
                value,
            });
        }

        Ok(WorkflowStepContextSnapshot::new(
            run.id,
            step.id,
            step_run.id,
            run.context.current_snapshot.snapshot_hash.clone(),
            run.output_bag.bag_hash.clone(),
            inputs,
            resolutions,
            Utc::now(),
        ))
    }

    fn workflow_step_input_resolution_recorded(
        run: &WorkflowRun,
        step_id: Uuid,
        snapshot_hash: &str,
    ) -> bool {
        run.step_contexts
            .get(&step_id)
            .is_some_and(|snapshot| snapshot.snapshot_hash == snapshot_hash)
    }

    fn resolve_workflow_value_source(
        source: &WorkflowValueSource,
        snapshot: &WorkflowStepContextSnapshot,
        child_run: Option<&WorkflowRun>,
        origin: &'static str,
    ) -> Result<WorkflowDataValue> {
        match source {
            WorkflowValueSource::Literal { value } => Ok(value.clone()),
            WorkflowValueSource::InputBinding { binding } => {
                snapshot.inputs.get(binding).cloned().ok_or_else(|| {
                    HivemindError::user(
                        "workflow_input_binding_missing",
                        format!("Workflow step input binding '{binding}' was not resolved"),
                        origin,
                    )
                })
            }
            WorkflowValueSource::ChildContextKey { key } => child_run
                .and_then(|run| run.context.current_snapshot.values.get(key).cloned())
                .ok_or_else(|| {
                    HivemindError::user(
                        "workflow_child_context_key_missing",
                        format!("Child workflow context key '{key}' was not found"),
                        origin,
                    )
                }),
        }
    }

    fn workflow_step_output_recorded(
        run: &WorkflowRun,
        step_run_id: Uuid,
        output_name: &str,
    ) -> bool {
        run.output_bag.entries.iter().any(|entry| {
            entry.producer_step_run_id == step_run_id && entry.output_name == output_name
        })
    }

    fn workflow_context_snapshot_recorded(run: &WorkflowRun, step_run_id: Uuid) -> bool {
        run.context
            .snapshots
            .iter()
            .any(|snapshot| snapshot.trigger_step_run_id == Some(step_run_id))
    }

    fn append_workflow_step_outputs(
        &self,
        run: &WorkflowRun,
        step: &WorkflowStepDefinition,
        snapshot: &WorkflowStepContextSnapshot,
        child_run: Option<&WorkflowRun>,
        origin: &'static str,
    ) -> Result<()> {
        let step_run = run.step_runs.get(&step.id).ok_or_else(|| {
            HivemindError::system(
                "workflow_step_run_missing",
                format!(
                    "Workflow step run '{}' missing from workflow run '{}'",
                    step.id, run.id
                ),
                origin,
            )
        })?;
        let mut sequence = run.output_bag.next_sequence;
        for output in &step.output_bindings {
            if Self::workflow_step_output_recorded(run, step_run.id, &output.name) {
                continue;
            }
            let payload =
                Self::resolve_workflow_value_source(&output.source, snapshot, child_run, origin)?;
            let entry = WorkflowOutputBagEntry {
                entry_id: Uuid::new_v4(),
                workflow_run_id: run.id,
                producer_step_id: step.id,
                producer_step_run_id: step_run.id,
                branch_step_id: (step.kind == WorkflowStepKind::Task).then_some(step.id),
                join_step_id: (step.kind == WorkflowStepKind::Join).then_some(step.id),
                output_name: output.name.clone(),
                tags: output.tags.clone(),
                payload,
                event_sequence: sequence,
                appended_at: Utc::now(),
            };
            sequence = sequence.saturating_add(1);
            self.append_event(
                Event::new(
                    EventPayload::WorkflowOutputAppended {
                        workflow_run_id: run.id,
                        step_id: step.id,
                        step_run_id: step_run.id,
                        entry,
                    },
                    CorrelationIds::for_workflow_step(
                        run.project_id,
                        run.workflow_id,
                        run.id,
                        run.root_workflow_run_id,
                        run.parent_workflow_run_id,
                        step.id,
                        step_run.id,
                    ),
                ),
                origin,
            )?;
        }
        Ok(())
    }

    fn capture_workflow_context_snapshot(
        &self,
        run: &WorkflowRun,
        step: &WorkflowStepDefinition,
        snapshot: &WorkflowStepContextSnapshot,
        child_run: Option<&WorkflowRun>,
        origin: &'static str,
    ) -> Result<()> {
        let step_run = run.step_runs.get(&step.id).ok_or_else(|| {
            HivemindError::system(
                "workflow_step_run_missing",
                format!(
                    "Workflow step run '{}' missing from workflow run '{}'",
                    step.id, run.id
                ),
                origin,
            )
        })?;
        if step.context_patches.is_empty()
            || Self::workflow_context_snapshot_recorded(run, step_run.id)
        {
            return Ok(());
        }

        let mut patches = BTreeMap::new();
        for patch in &step.context_patches {
            patches.insert(
                patch.key.clone(),
                Self::resolve_workflow_value_source(&patch.source, snapshot, child_run, origin)?,
            );
        }

        let next_context = run
            .context
            .apply_patches(
                patches,
                Utc::now(),
                format!("step_completion:{}", step.name),
                Some(step.id),
                Some(step_run.id),
            )
            .map_err(|err| {
                HivemindError::user("workflow_context_patch_failed", err.to_string(), origin)
            })?;

        self.append_event(
            Event::new(
                EventPayload::WorkflowContextSnapshotCaptured {
                    workflow_run_id: run.id,
                    snapshot: next_context.current_snapshot,
                },
                CorrelationIds::for_workflow_step(
                    run.project_id,
                    run.workflow_id,
                    run.id,
                    run.root_workflow_run_id,
                    run.parent_workflow_run_id,
                    step.id,
                    step_run.id,
                ),
            ),
            origin,
        )?;

        Ok(())
    }

    fn materialize_workflow_step_completion(
        &self,
        workflow: &WorkflowDefinition,
        run: &WorkflowRun,
        step_id: Uuid,
        child_run: Option<&WorkflowRun>,
        origin: &'static str,
    ) -> Result<()> {
        let step = Self::workflow_step_for_definition(workflow, step_id, origin)?;
        let step_run = run.step_runs.get(&step_id).ok_or_else(|| {
            HivemindError::system(
                "workflow_step_run_missing",
                format!(
                    "Workflow step run '{}' missing from workflow run '{}'",
                    step_id, run.id
                ),
                origin,
            )
        })?;
        let snapshot = if let Some(snapshot) = run.step_contexts.get(&step_id).cloned() {
            snapshot
        } else {
            let snapshot = self.resolve_workflow_step_inputs(run, workflow, step, origin)?;
            self.append_event(
                Event::new(
                    EventPayload::WorkflowStepInputsResolved {
                        workflow_run_id: run.id,
                        step_id,
                        step_run_id: step_run.id,
                        snapshot: snapshot.clone(),
                    },
                    CorrelationIds::for_workflow_step(
                        run.project_id,
                        run.workflow_id,
                        run.id,
                        run.root_workflow_run_id,
                        run.parent_workflow_run_id,
                        step_id,
                        step_run.id,
                    ),
                ),
                origin,
            )?;
            snapshot
        };

        self.append_workflow_step_outputs(run, step, &snapshot, child_run, origin)?;
        self.capture_workflow_context_snapshot(run, step, &snapshot, child_run, origin)?;
        Ok(())
    }

    fn child_workflow_succeeded(run: &WorkflowRun) -> bool {
        run.state == WorkflowRunState::Completed
            && run.step_runs.values().all(|step_run| {
                matches!(
                    step_run.state,
                    WorkflowStepState::Succeeded | WorkflowStepState::Skipped
                )
            })
    }

    fn selected_entries_for_selector<'a>(
        run: &'a WorkflowRun,
        selector: &WorkflowBagSelector,
    ) -> Vec<&'a WorkflowOutputBagEntry> {
        run.output_bag
            .entries
            .iter()
            .filter(|entry| entry.output_name == selector.output_name)
            .filter(|entry| {
                selector.producer_step_ids.is_empty()
                    || selector.producer_step_ids.contains(&entry.producer_step_id)
            })
            .filter(|entry| {
                selector.tags.is_empty()
                    || selector
                        .tags
                        .iter()
                        .all(|tag| entry.tags.iter().any(|item| item == tag))
            })
            .filter(|entry| {
                selector
                    .expected_schema
                    .as_ref()
                    .is_none_or(|schema| entry.payload.schema == *schema)
            })
            .filter(|entry| {
                selector
                    .expected_schema_version
                    .is_none_or(|version| entry.payload.schema_version == version)
            })
            .collect::<Vec<_>>()
    }

    fn evaluate_condition_expression(
        run: &WorkflowRun,
        workflow: &WorkflowDefinition,
        condition: &WorkflowConditionExpression,
    ) -> Result<(bool, BTreeMap<String, WorkflowDataValue>)> {
        let step_names = workflow
            .steps
            .values()
            .map(|step| (step.id, step.name.clone()))
            .collect::<BTreeMap<_, _>>();
        match condition {
            WorkflowConditionExpression::ContextKeyExists { key } => Ok((
                run.context.current_snapshot.values.contains_key(key),
                BTreeMap::from([(
                    "context_value".to_string(),
                    run.context
                        .current_snapshot
                        .values
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| {
                            WorkflowDataValue::new("application/null", 1, serde_json::Value::Null)
                                .expect("static null value")
                        }),
                )]),
            )),
            WorkflowConditionExpression::ContextValueEquals { key, value } => Ok((
                run.context.current_snapshot.values.get(key) == Some(value),
                BTreeMap::from([("expected".to_string(), value.clone())]),
            )),
            WorkflowConditionExpression::BagCountAtLeast { selector, count } => {
                let selected = Self::selected_entries_for_selector(run, selector);
                Ok((
                    selected.len() >= (*count as usize),
                    BTreeMap::from([(
                        "match_count".to_string(),
                        WorkflowDataValue::new(
                            "application/json",
                            1,
                            serde_json::json!(selected.len()),
                        )
                        .expect("static count value"),
                    )]),
                ))
            }
            WorkflowConditionExpression::BagValueEquals {
                selector,
                reducer,
                value,
            } => {
                let selected = Self::selected_entries_for_selector(run, selector);
                let selected = selected.into_iter().cloned().collect::<Vec<_>>();
                let reduced = reducer
                    .reduce(selector, &selected, &step_names)
                    .map_err(|err| {
                        HivemindError::user(
                            "workflow_condition_invalid",
                            err.to_string(),
                            "registry:evaluate_condition_expression",
                        )
                    })?;
                Ok((
                    reduced == *value,
                    BTreeMap::from([
                        ("expected".to_string(), value.clone()),
                        ("actual".to_string(), reduced),
                    ]),
                ))
            }
        }
    }

    fn skip_branch_subtree(
        &self,
        workflow_run_id: &str,
        workflow: &WorkflowDefinition,
        root_step_ids: &[Uuid],
    ) -> Result<()> {
        let mut queue = root_step_ids.to_vec();
        let mut visited = BTreeSet::new();
        while let Some(step_id) = queue.pop() {
            if !visited.insert(step_id) {
                continue;
            }
            let run = self.get_workflow_run(workflow_run_id)?;
            let Some(step_run) = run.step_runs.get(&step_id) else {
                continue;
            };
            if matches!(
                step_run.state,
                WorkflowStepState::Pending | WorkflowStepState::Ready
            ) {
                let _ = self.workflow_step_set_state(
                    workflow_run_id,
                    &step_id.to_string(),
                    WorkflowStepState::Skipped,
                    Some("conditional_branch_not_selected"),
                )?;
            }
            for child in workflow.steps.values().filter(|candidate| {
                candidate.depends_on.contains(&step_id)
                    && candidate.kind != WorkflowStepKind::Workflow
            }) {
                queue.push(child.id);
            }
        }
        Ok(())
    }

    fn process_ready_conditional_steps(
        &self,
        workflow_run_id: &str,
        workflow: &WorkflowDefinition,
        origin: &'static str,
    ) -> Result<()> {
        loop {
            let run = self.get_workflow_run(workflow_run_id)?;
            let Some(step) = workflow.steps.values().find(|step| {
                step.kind == WorkflowStepKind::Conditional
                    && run
                        .step_runs
                        .get(&step.id)
                        .is_some_and(|step_run| step_run.state == WorkflowStepState::Ready)
            }) else {
                break;
            };
            let step = step.clone();
            let step_run = run
                .step_runs
                .get(&step.id)
                .cloned()
                .expect("ready step run exists");
            let conditional = step.conditional.clone().ok_or_else(|| {
                HivemindError::system(
                    "workflow_conditional_config_missing",
                    format!("Conditional step '{}' is missing config", step.name),
                    origin,
                )
            })?;
            let mut chosen_branch = None;
            for branch in &conditional.branches {
                let (result, inputs) =
                    Self::evaluate_condition_expression(&run, workflow, &branch.condition)?;
                self.append_event(
                    Event::new(
                        EventPayload::WorkflowConditionEvaluated {
                            workflow_run_id: run.id,
                            step_id: step.id,
                            step_run_id: step_run.id,
                            inputs,
                            result,
                            chosen_path: result.then(|| branch.name.clone()),
                        },
                        CorrelationIds::for_workflow_step(
                            run.project_id,
                            run.workflow_id,
                            run.id,
                            run.root_workflow_run_id,
                            run.parent_workflow_run_id,
                            step.id,
                            step_run.id,
                        ),
                    ),
                    origin,
                )?;
                if result {
                    chosen_branch = Some(branch.name.clone());
                    break;
                }
            }
            let chosen_branch = chosen_branch
                .or(conditional.default_branch.clone())
                .ok_or_else(|| {
                    HivemindError::user(
                        "workflow_condition_no_branch_selected",
                        format!("Conditional step '{}' matched no branch", step.name),
                        origin,
                    )
                })?;
            let non_selected = conditional
                .branches
                .iter()
                .filter(|branch| branch.name != chosen_branch)
                .flat_map(|branch| branch.activate_step_ids.iter().copied())
                .collect::<Vec<_>>();
            self.workflow_step_set_state(
                workflow_run_id,
                &step.id.to_string(),
                WorkflowStepState::Running,
                Some("condition_evaluating"),
            )?;
            self.workflow_step_set_state(
                workflow_run_id,
                &step.id.to_string(),
                WorkflowStepState::Succeeded,
                Some(&format!("branch:{chosen_branch}")),
            )?;
            self.skip_branch_subtree(workflow_run_id, workflow, &non_selected)?;
        }
        Ok(())
    }

    fn signal_matches_wait(signal: &WorkflowSignal, condition: &WorkflowWaitCondition) -> bool {
        match condition {
            WorkflowWaitCondition::Signal { signal_name, .. }
            | WorkflowWaitCondition::HumanSignal { signal_name } => {
                signal.signal_name == *signal_name
            }
            WorkflowWaitCondition::Timer { .. } => false,
        }
    }

    fn process_wait_steps(
        &self,
        workflow_run_id: &str,
        workflow: &WorkflowDefinition,
        origin: &'static str,
    ) -> Result<()> {
        let now = Utc::now();
        let run = self.get_workflow_run(workflow_run_id)?;
        for step in workflow
            .steps
            .values()
            .filter(|step| step.kind == WorkflowStepKind::Wait)
        {
            let Some(step_run) = run.step_runs.get(&step.id).cloned() else {
                continue;
            };
            let wait_cfg = step.wait.clone().ok_or_else(|| {
                HivemindError::system(
                    "workflow_wait_config_missing",
                    format!("Wait step '{}' is missing config", step.name),
                    origin,
                )
            })?;
            if step_run.state == WorkflowStepState::Ready {
                self.workflow_step_set_state(
                    workflow_run_id,
                    &step.id.to_string(),
                    WorkflowStepState::Running,
                    Some("wait_armed"),
                )?;
                let status = wait_cfg.condition.to_wait_status(now);
                self.append_event(
                    Event::new(
                        EventPayload::WorkflowWaitActivated {
                            workflow_run_id: run.id,
                            step_id: step.id,
                            step_run_id: step_run.id,
                            wait_status: status.clone(),
                        },
                        CorrelationIds::for_workflow_step(
                            run.project_id,
                            run.workflow_id,
                            run.id,
                            run.root_workflow_run_id,
                            run.parent_workflow_run_id,
                            step.id,
                            step_run.id,
                        ),
                    ),
                    origin,
                )?;
                self.workflow_step_set_state(
                    workflow_run_id,
                    &step.id.to_string(),
                    WorkflowStepState::Waiting,
                    Some("waiting"),
                )?;
                continue;
            }
            if step_run.state != WorkflowStepState::Waiting {
                continue;
            }
            let Some(wait_status) = step_run.wait_status.clone() else {
                continue;
            };
            let completion = match &wait_status.condition {
                WorkflowWaitCondition::Timer { .. } => wait_status
                    .resume_at
                    .filter(|resume_at| *resume_at <= now)
                    .map(|_| ("timer_elapsed".to_string(), None)),
                WorkflowWaitCondition::Signal {
                    payload_schema,
                    payload_schema_version,
                    ..
                } => run
                    .signals
                    .iter()
                    .find(|signal| {
                        signal.step_id.is_none_or(|id| id == step.id)
                            && Self::signal_matches_wait(signal, &wait_status.condition)
                    })
                    .and_then(|signal| {
                        if let Some(schema) = payload_schema {
                            let payload = signal.payload.as_ref()?;
                            if payload.schema != *schema
                                || payload_schema_version
                                    .is_some_and(|version| payload.schema_version != version)
                            {
                                return None;
                            }
                        }
                        Some(("signal_received".to_string(), Some(signal.clone())))
                    }),
                WorkflowWaitCondition::HumanSignal { .. } => run
                    .signals
                    .iter()
                    .find(|signal| {
                        signal.step_id.is_none_or(|id| id == step.id)
                            && signal.emitted_by == "human"
                            && Self::signal_matches_wait(signal, &wait_status.condition)
                    })
                    .map(|signal| ("human_signal_received".to_string(), Some(signal.clone()))),
            };
            let Some((completion_reason, signal)) = completion else {
                continue;
            };
            let mut completed = wait_status.clone();
            completed.completed_at = Some(now);
            completed.completion_reason = Some(completion_reason.clone());
            completed.signal = signal;
            self.append_event(
                Event::new(
                    EventPayload::WorkflowWaitCompleted {
                        workflow_run_id: run.id,
                        step_id: step.id,
                        step_run_id: step_run.id,
                        wait_status: completed,
                    },
                    CorrelationIds::for_workflow_step(
                        run.project_id,
                        run.workflow_id,
                        run.id,
                        run.root_workflow_run_id,
                        run.parent_workflow_run_id,
                        step.id,
                        step_run.id,
                    ),
                ),
                origin,
            )?;
            self.workflow_step_set_state(
                workflow_run_id,
                &step.id.to_string(),
                WorkflowStepState::Succeeded,
                Some(&completion_reason),
            )?;
        }
        Ok(())
    }

    fn apply_workflow_dependency_releases(
        &self,
        workflow_run_id: &str,
        workflow: &WorkflowDefinition,
    ) -> Result<WorkflowRun> {
        let mut run = self.get_workflow_run(workflow_run_id)?;
        for step in workflow.steps.values() {
            let Some(step_run) = run.step_runs.get(&step.id) else {
                continue;
            };
            if step_run.state != WorkflowStepState::Pending {
                continue;
            }
            let ready = !step.depends_on.is_empty()
                && step.depends_on.iter().all(|dep_step_id| {
                    run.step_runs
                        .get(dep_step_id)
                        .is_some_and(|dep| dep.state == WorkflowStepState::Succeeded)
                });
            if ready {
                run = self.workflow_step_set_state(
                    workflow_run_id,
                    &step.id.to_string(),
                    WorkflowStepState::Ready,
                    None,
                )?;
            }
        }
        Ok(run)
    }

    fn process_workflow_child_steps(
        &self,
        workflow_run_id: &str,
        workflow: &WorkflowDefinition,
        origin: &'static str,
    ) -> Result<WorkflowRun> {
        let mut run = self.get_workflow_run(workflow_run_id)?;
        for step in workflow
            .steps
            .values()
            .filter(|step| step.kind == WorkflowStepKind::Workflow)
        {
            let Some(step_run) = run.step_runs.get(&step.id).cloned() else {
                continue;
            };
            if !matches!(
                step_run.state,
                WorkflowStepState::Ready | WorkflowStepState::Retry | WorkflowStepState::Waiting
            ) {
                continue;
            }
            let child_cfg = step.child_workflow.clone().ok_or_else(|| {
                HivemindError::system(
                    "workflow_child_config_missing",
                    format!(
                        "Workflow step '{}' is missing child workflow config",
                        step.name
                    ),
                    origin,
                )
            })?;

            let existing_child = Self::child_workflow_run_for_step(&self.state()?, &run, step.id);
            if step_run.state != WorkflowStepState::Waiting && existing_child.is_none() {
                let snapshot = self.resolve_workflow_step_inputs(&run, workflow, step, origin)?;
                if !Self::workflow_step_input_resolution_recorded(
                    &run,
                    step.id,
                    &snapshot.snapshot_hash,
                ) {
                    self.append_event(
                        Event::new(
                            EventPayload::WorkflowStepInputsResolved {
                                workflow_run_id: run.id,
                                step_id: step.id,
                                step_run_id: step_run.id,
                                snapshot: snapshot.clone(),
                            },
                            CorrelationIds::for_workflow_step(
                                run.project_id,
                                run.workflow_id,
                                run.id,
                                run.root_workflow_run_id,
                                run.parent_workflow_run_id,
                                step.id,
                                step_run.id,
                            ),
                        ),
                        origin,
                    )?;
                }
                let child = self.create_workflow_run_internal(
                    &self.get_workflow(&child_cfg.workflow_id.to_string())?,
                    Some(&run.context.schema),
                    Some(run.context.schema_version),
                    snapshot.inputs.clone(),
                    Some(run.root_workflow_run_id),
                    Some(run.id),
                    Some(step.id),
                    origin,
                )?;
                self.start_workflow_run(&child.id.to_string())?;
                self.workflow_step_set_state(
                    workflow_run_id,
                    &step.id.to_string(),
                    WorkflowStepState::Running,
                    None,
                )?;
                run = self.workflow_step_set_state(
                    workflow_run_id,
                    &step.id.to_string(),
                    WorkflowStepState::Waiting,
                    None,
                )?;
            }

            let child = Self::child_workflow_run_for_step(&self.state()?, &run, step.id);
            let Some(child) = child else {
                continue;
            };
            if child.state == WorkflowRunState::Running {
                let _ = self.tick_workflow_run(&child.id.to_string(), false, Some(1))?;
            }
            let child = self.get_workflow_run(&child.id.to_string())?;
            if child.state == WorkflowRunState::Running || child.state == WorkflowRunState::Paused {
                continue;
            }

            let behavior = self.child_run_terminal_behavior(&child)?;

            match behavior {
                WorkflowChildTerminalBehavior::Complete => {
                    self.workflow_step_set_state(
                        workflow_run_id,
                        &step.id.to_string(),
                        WorkflowStepState::Succeeded,
                        None,
                    )?;
                    let refreshed = self.get_workflow_run(workflow_run_id)?;
                    self.materialize_workflow_step_completion(
                        workflow,
                        &refreshed,
                        step.id,
                        Some(&child),
                        origin,
                    )?;
                    run = self.get_workflow_run(workflow_run_id)?;
                }
                WorkflowChildTerminalBehavior::FailStep => {
                    run = self.workflow_step_set_state(
                        workflow_run_id,
                        &step.id.to_string(),
                        WorkflowStepState::Failed,
                        Some("child_workflow_failed"),
                    )?;
                }
                WorkflowChildTerminalBehavior::AbortParent => {
                    return self.abort_workflow_run(
                        workflow_run_id,
                        Some("child_workflow_failed"),
                        false,
                    );
                }
            }
        }
        Ok(run)
    }

    pub(crate) fn workflow_attempt_context_for_task(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<Option<(AttemptContextWorkflowManifest, String)>> {
        let Some(run) = Self::workflow_bridge_run_for_flow_state(state, flow.id) else {
            return Ok(None);
        };
        let Some((step_id, step_run_id)) = Self::workflow_bridge_step_for_task(run, task_id) else {
            return Ok(None);
        };
        let workflow = self.get_workflow(&run.workflow_id.to_string())?;
        let step = Self::workflow_step_for_definition(&workflow, step_id, origin)?;
        let snapshot = self.resolve_workflow_step_inputs(run, &workflow, step, origin)?;
        if !Self::workflow_step_input_resolution_recorded(run, step_id, &snapshot.snapshot_hash) {
            self.append_event(
                Event::new(
                    EventPayload::WorkflowStepInputsResolved {
                        workflow_run_id: run.id,
                        step_id,
                        step_run_id,
                        snapshot: snapshot.clone(),
                    },
                    CorrelationIds::for_workflow_step(
                        run.project_id,
                        run.workflow_id,
                        run.id,
                        run.root_workflow_run_id,
                        run.parent_workflow_run_id,
                        step_id,
                        step_run_id,
                    ),
                ),
                origin,
            )?;
        }
        let output_entry_ids = snapshot
            .resolutions
            .iter()
            .flat_map(|item| item.selected_entry_ids.iter().copied())
            .collect::<Vec<_>>();
        let manifest = AttemptContextWorkflowManifest {
            workflow_run_id: run.id,
            step_id,
            step_run_id,
            context_schema: run.context.schema.clone(),
            context_schema_version: run.context.schema_version,
            context_snapshot_hash: run.context.current_snapshot.snapshot_hash.clone(),
            step_input_snapshot_hash: snapshot.snapshot_hash.clone(),
            output_bag_hash: run.output_bag.bag_hash.clone(),
            output_entry_ids,
        };
        let section = format!(
            "Workflow Step Context:\n- workflow_run_id: {}\n- step_id: {}\n- step_run_id: {}\n- context_snapshot_hash: {}\n- output_bag_hash: {}\n- step_input_snapshot_hash: {}\n- inputs:\n{}",
            run.id,
            step_id,
            step_run_id,
            run.context.current_snapshot.snapshot_hash,
            run.output_bag.bag_hash,
            snapshot.snapshot_hash,
            serde_yaml::to_string(&snapshot.inputs).map_err(|err| HivemindError::system(
                "workflow_step_context_render_failed",
                err.to_string(),
                origin,
            ))?
        );
        Ok(Some((manifest, section)))
    }

    fn append_join_task_exec_transition(
        &self,
        run: &WorkflowRun,
        from: TaskExecState,
        to: TaskExecState,
        step_id: Uuid,
        origin: &'static str,
    ) -> Result<()> {
        let flow_id = Self::workflow_bridge_flow_id(run.id);
        let graph_id = Self::workflow_bridge_graph_id(run.id);
        let task_id = Self::workflow_bridge_task_id(run.id, step_id);
        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id,
                    task_id,
                    attempt_id: None,
                    from,
                    to,
                },
                Self::workflow_bridge_corr(
                    run,
                    graph_id,
                    Some(flow_id),
                    Some(task_id),
                    Some(step_id),
                    run.step_runs.get(&step_id).map(|step_run| step_run.id),
                ),
            ),
            origin,
        )?;
        Ok(())
    }

    fn execute_ready_join_step(
        &self,
        workflow: &WorkflowDefinition,
        run: &WorkflowRun,
        step: &WorkflowStepDefinition,
        origin: &'static str,
    ) -> Result<()> {
        let flow_id = Self::workflow_bridge_flow_id(run.id);
        let flow = self.get_flow(&flow_id.to_string())?;
        let task_id = Self::workflow_bridge_task_id(run.id, step.id);
        let exec_state = flow
            .task_executions
            .get(&task_id)
            .map(|exec| exec.state)
            .unwrap_or(TaskExecState::Pending);

        let snapshot = match self.resolve_workflow_step_inputs(run, workflow, step, origin) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                match exec_state {
                    TaskExecState::Pending => {
                        self.append_join_task_exec_transition(
                            run,
                            TaskExecState::Pending,
                            TaskExecState::Ready,
                            step.id,
                            origin,
                        )?;
                        self.append_join_task_exec_transition(
                            run,
                            TaskExecState::Ready,
                            TaskExecState::Running,
                            step.id,
                            origin,
                        )?;
                    }
                    TaskExecState::Ready => {
                        self.append_join_task_exec_transition(
                            run,
                            TaskExecState::Ready,
                            TaskExecState::Running,
                            step.id,
                            origin,
                        )?;
                    }
                    TaskExecState::Running
                    | TaskExecState::Verifying
                    | TaskExecState::Retry
                    | TaskExecState::Success
                    | TaskExecState::Failed
                    | TaskExecState::Escalated => {}
                }
                self.append_join_task_exec_transition(
                    run,
                    if matches!(
                        exec_state,
                        TaskExecState::Running | TaskExecState::Verifying
                    ) {
                        exec_state
                    } else {
                        TaskExecState::Running
                    },
                    TaskExecState::Failed,
                    step.id,
                    origin,
                )?;
                self.record_error_event(
                    &err,
                    CorrelationIds::for_workflow_step(
                        run.project_id,
                        run.workflow_id,
                        run.id,
                        run.root_workflow_run_id,
                        run.parent_workflow_run_id,
                        step.id,
                        run.step_runs
                            .get(&step.id)
                            .map_or(step.id, |step_run| step_run.id),
                    ),
                );
                return Err(err);
            }
        };

        let step_run_id = run
            .step_runs
            .get(&step.id)
            .map_or(step.id, |step_run| step_run.id);
        if !Self::workflow_step_input_resolution_recorded(run, step.id, &snapshot.snapshot_hash) {
            self.append_event(
                Event::new(
                    EventPayload::WorkflowStepInputsResolved {
                        workflow_run_id: run.id,
                        step_id: step.id,
                        step_run_id,
                        snapshot: snapshot.clone(),
                    },
                    CorrelationIds::for_workflow_step(
                        run.project_id,
                        run.workflow_id,
                        run.id,
                        run.root_workflow_run_id,
                        run.parent_workflow_run_id,
                        step.id,
                        step_run_id,
                    ),
                ),
                origin,
            )?;
        }

        match exec_state {
            TaskExecState::Pending => {
                self.append_join_task_exec_transition(
                    run,
                    TaskExecState::Pending,
                    TaskExecState::Ready,
                    step.id,
                    origin,
                )?;
                self.append_join_task_exec_transition(
                    run,
                    TaskExecState::Ready,
                    TaskExecState::Running,
                    step.id,
                    origin,
                )?;
            }
            TaskExecState::Ready => {
                self.append_join_task_exec_transition(
                    run,
                    TaskExecState::Ready,
                    TaskExecState::Running,
                    step.id,
                    origin,
                )?;
            }
            TaskExecState::Running
            | TaskExecState::Verifying
            | TaskExecState::Retry
            | TaskExecState::Success
            | TaskExecState::Failed
            | TaskExecState::Escalated => {}
        }

        self.materialize_workflow_step_completion(workflow, run, step.id, None, origin)?;
        self.append_join_task_exec_transition(
            run,
            TaskExecState::Running,
            TaskExecState::Verifying,
            step.id,
            origin,
        )?;
        self.append_join_task_exec_transition(
            run,
            TaskExecState::Verifying,
            TaskExecState::Success,
            step.id,
            origin,
        )?;
        Ok(())
    }

    fn process_ready_join_steps(
        &self,
        workflow_run_id: &str,
        workflow: &WorkflowDefinition,
        origin: &'static str,
    ) -> Result<()> {
        loop {
            let run = self.get_workflow_run(workflow_run_id)?;
            let next_join = workflow.steps.values().find(|step| {
                step.kind == WorkflowStepKind::Join
                    && run
                        .step_runs
                        .get(&step.id)
                        .is_some_and(|step_run| step_run.state == WorkflowStepState::Ready)
            });
            let Some(step) = next_join.cloned() else {
                break;
            };
            if let Err(err) = self.execute_ready_join_step(workflow, &run, &step, origin) {
                let _ = self.sync_workflow_run_from_synthetic_flow(workflow_run_id, workflow);
                return Err(err);
            }
            let _ = self.sync_workflow_run_from_synthetic_flow(workflow_run_id, workflow)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn ensure_synthetic_flow_for_workflow_run(
        &self,
        run: &WorkflowRun,
        workflow: &WorkflowDefinition,
    ) -> Result<Uuid> {
        if let Some(step) = workflow.steps.values().find(|step| {
            !matches!(
                step.kind,
                WorkflowStepKind::Task
                    | WorkflowStepKind::Join
                    | WorkflowStepKind::Workflow
                    | WorkflowStepKind::Conditional
                    | WorkflowStepKind::Wait
            )
        }) {
            let step_run_id = run.step_runs.get(&step.id).map(|step_run| step_run.id);
            let err = HivemindError::user(
                "workflow_step_kind_not_supported",
                format!(
                    "Workflow step '{}' uses unsupported kind {:?} for Sprint 65 execution",
                    step.name, step.kind
                ),
                "registry:tick_workflow_run",
            )
            .with_hint("Unsupported workflow step kind for Phase 5 execution");
            self.record_error_event(
                &err,
                CorrelationIds::for_workflow_step(
                    run.project_id,
                    run.workflow_id,
                    run.id,
                    run.root_workflow_run_id,
                    run.parent_workflow_run_id,
                    step.id,
                    step_run_id.unwrap_or(step.id),
                ),
            );
            return Err(err);
        }

        let graph_id = Self::workflow_bridge_graph_id(run.id);
        let flow_id = Self::workflow_bridge_flow_id(run.id);
        let state = self.state()?;
        let existing_graph = state.graphs.get(&graph_id).cloned();

        if existing_graph.is_none() {
            self.append_event(
                Event::new(
                    EventPayload::TaskGraphCreated {
                        graph_id,
                        project_id: run.project_id,
                        name: format!("workflow-{}", run.id),
                        description: Some(format!("Synthetic graph for workflow run {}", run.id)),
                    },
                    Self::workflow_bridge_corr(run, graph_id, Some(flow_id), None, None, None),
                ),
                "registry:tick_workflow_run",
            )?;
        }

        for step in workflow.steps.values() {
            if matches!(
                step.kind,
                WorkflowStepKind::Workflow | WorkflowStepKind::Conditional | WorkflowStepKind::Wait
            ) {
                continue;
            }
            let task_id = Self::workflow_bridge_task_id(run.id, step.id);
            let step_run_id = run.step_runs.get(&step.id).map(|step_run| step_run.id);
            let corr = Self::workflow_bridge_corr(
                run,
                graph_id,
                Some(flow_id),
                Some(task_id),
                Some(step.id),
                step_run_id,
            );
            if !state.tasks.contains_key(&task_id) {
                self.append_event(
                    Event::new(
                        EventPayload::TaskCreated {
                            id: task_id,
                            project_id: run.project_id,
                            title: step.name.clone(),
                            description: step.description.clone(),
                            scope: None,
                        },
                        corr.clone(),
                    ),
                    "registry:tick_workflow_run",
                )?;
            }

            let graph_has_task = existing_graph
                .as_ref()
                .is_some_and(|graph| graph.tasks.contains_key(&task_id));
            if !graph_has_task {
                self.append_event(
                    Event::new(
                        EventPayload::TaskAddedToGraph {
                            graph_id,
                            task: Self::workflow_bridge_graph_task(step, task_id),
                        },
                        corr.clone(),
                    ),
                    "registry:tick_workflow_run",
                )?;
            }

            if step.kind == WorkflowStepKind::Join
                && state
                    .tasks
                    .get(&task_id)
                    .and_then(|task| task.runtime_override.as_ref())
                    .is_none_or(|runtime| runtime.adapter_name != "native")
            {
                self.append_event(
                    Event::new(
                        EventPayload::TaskRuntimeConfigured {
                            task_id,
                            adapter_name: "native".to_string(),
                            binary_path: "builtin-native".to_string(),
                            model: None,
                            args: Vec::new(),
                            env: HashMap::new(),
                            timeout_ms: 60_000,
                        },
                        corr,
                    ),
                    "registry:tick_workflow_run",
                )?;
            }
        }

        let graph = self.get_graph(&graph_id.to_string())?;
        for step in workflow.steps.values() {
            if matches!(
                step.kind,
                WorkflowStepKind::Workflow | WorkflowStepKind::Conditional | WorkflowStepKind::Wait
            ) {
                continue;
            }
            let to_task = Self::workflow_bridge_task_id(run.id, step.id);
            let step_run_id = run.step_runs.get(&step.id).map(|step_run| step_run.id);
            let existing_deps = graph
                .dependencies
                .get(&to_task)
                .cloned()
                .unwrap_or_default();
            for dep_step_id in &step.depends_on {
                if workflow
                    .steps
                    .get(dep_step_id)
                    .is_some_and(|dep_step| dep_step.kind == WorkflowStepKind::Workflow)
                {
                    continue;
                }
                let from_task = Self::workflow_bridge_task_id(run.id, *dep_step_id);
                if existing_deps.contains(&from_task) {
                    continue;
                }
                self.append_event(
                    Event::new(
                        EventPayload::DependencyAdded {
                            graph_id,
                            from_task,
                            to_task,
                        },
                        Self::workflow_bridge_corr(
                            run,
                            graph_id,
                            Some(flow_id),
                            Some(to_task),
                            Some(step.id),
                            step_run_id,
                        ),
                    ),
                    "registry:tick_workflow_run",
                )?;
            }
        }

        let graph = self.get_graph(&graph_id.to_string())?;
        if graph.state == GraphState::Draft {
            let issues = Self::validate_graph_issues(&graph);
            if !issues.is_empty() {
                let err = HivemindError::user(
                    "workflow_synthetic_graph_invalid",
                    issues.join("; "),
                    "registry:tick_workflow_run",
                );
                self.record_error_event(
                    &err,
                    Self::workflow_bridge_corr(run, graph_id, Some(flow_id), None, None, None),
                );
                return Err(err);
            }
            self.append_event(
                Event::new(
                    EventPayload::TaskGraphValidated {
                        graph_id,
                        project_id: run.project_id,
                        valid: true,
                        issues: Vec::new(),
                    },
                    Self::workflow_bridge_corr(run, graph_id, Some(flow_id), None, None, None),
                ),
                "registry:tick_workflow_run",
            )?;
        }

        if !self.state()?.flows.contains_key(&flow_id) {
            self.append_event(
                Event::new(
                    EventPayload::TaskFlowCreated {
                        flow_id,
                        graph_id,
                        project_id: run.project_id,
                        name: Some(format!("workflow-run-{}", run.id)),
                        task_ids: workflow
                            .steps
                            .values()
                            .filter(|step| step.kind != WorkflowStepKind::Workflow)
                            .filter(|step| {
                                !matches!(
                                    step.kind,
                                    WorkflowStepKind::Conditional | WorkflowStepKind::Wait
                                )
                            })
                            .map(|step| Self::workflow_bridge_task_id(run.id, step.id))
                            .collect(),
                    },
                    Self::workflow_bridge_corr(run, graph_id, Some(flow_id), None, None, None),
                ),
                "registry:tick_workflow_run",
            )?;
        }

        Ok(flow_id)
    }

    fn workflow_step_state_for_task_exec(
        step_kind: WorkflowStepKind,
        state: TaskExecState,
    ) -> WorkflowStepState {
        if step_kind == WorkflowStepKind::Join {
            return match state {
                TaskExecState::Pending => WorkflowStepState::Pending,
                TaskExecState::Ready | TaskExecState::Running | TaskExecState::Verifying => {
                    WorkflowStepState::Ready
                }
                TaskExecState::Success => WorkflowStepState::Succeeded,
                TaskExecState::Retry => WorkflowStepState::Retry,
                TaskExecState::Failed | TaskExecState::Escalated => WorkflowStepState::Failed,
            };
        }

        match state {
            TaskExecState::Pending => WorkflowStepState::Pending,
            TaskExecState::Ready => WorkflowStepState::Ready,
            TaskExecState::Running => WorkflowStepState::Running,
            TaskExecState::Verifying => WorkflowStepState::Verifying,
            TaskExecState::Success => WorkflowStepState::Succeeded,
            TaskExecState::Retry => WorkflowStepState::Retry,
            TaskExecState::Failed | TaskExecState::Escalated => WorkflowStepState::Failed,
        }
    }

    fn workflow_step_reconcile_path(
        current: WorkflowStepState,
        target: WorkflowStepState,
    ) -> Vec<WorkflowStepState> {
        use WorkflowStepState as Step;

        if current == target {
            return Vec::new();
        }

        match target {
            Step::Pending => Vec::new(),
            Step::Ready => vec![Step::Ready],
            Step::Running => match current {
                Step::Pending | Step::Failed => vec![Step::Ready, Step::Running],
                _ => vec![Step::Running],
            },
            Step::Verifying => match current {
                Step::Pending | Step::Failed => vec![Step::Ready, Step::Running, Step::Verifying],
                Step::Ready | Step::Retry => vec![Step::Running, Step::Verifying],
                _ => vec![Step::Verifying],
            },
            Step::Retry => match current {
                Step::Pending | Step::Failed => vec![Step::Ready, Step::Running, Step::Retry],
                Step::Ready => vec![Step::Running, Step::Retry],
                _ => vec![Step::Retry],
            },
            Step::Waiting => match current {
                Step::Pending | Step::Failed => vec![Step::Ready, Step::Running, Step::Waiting],
                Step::Ready => vec![Step::Running, Step::Waiting],
                _ => vec![Step::Waiting],
            },
            Step::Succeeded => match current {
                Step::Pending | Step::Failed => {
                    vec![Step::Ready, Step::Running, Step::Verifying, Step::Succeeded]
                }
                Step::Ready | Step::Retry => {
                    vec![Step::Running, Step::Verifying, Step::Succeeded]
                }
                Step::Running => vec![Step::Verifying, Step::Succeeded],
                _ => vec![Step::Succeeded],
            },
            Step::Failed => match current {
                Step::Pending => vec![Step::Ready, Step::Running, Step::Failed],
                Step::Ready => vec![Step::Running, Step::Failed],
                _ => vec![Step::Failed],
            },
            Step::Skipped => vec![Step::Skipped],
            Step::Aborted => vec![Step::Aborted],
        }
    }

    fn sync_workflow_run_from_synthetic_flow(
        &self,
        workflow_run_id: &str,
        workflow: &WorkflowDefinition,
    ) -> Result<WorkflowRun> {
        let mut run = self.get_workflow_run(workflow_run_id)?;
        let flow_id = Self::workflow_bridge_flow_id(run.id);
        let Some(flow) = self.state()?.flows.get(&flow_id).cloned() else {
            return Ok(run);
        };

        for step in workflow.steps.values() {
            if matches!(
                step.kind,
                WorkflowStepKind::Conditional | WorkflowStepKind::Wait
            ) {
                continue;
            }
            let task_id = Self::workflow_bridge_task_id(run.id, step.id);
            let Some(exec) = flow.task_executions.get(&task_id) else {
                continue;
            };
            let target_state = Self::workflow_step_state_for_task_exec(step.kind, exec.state);
            let current_state = run
                .step_runs
                .get(&step.id)
                .map_or(WorkflowStepState::Pending, |step_run| step_run.state);
            if current_state == target_state {
                continue;
            }
            for state in Self::workflow_step_reconcile_path(current_state, target_state) {
                run = self.workflow_step_set_state(
                    workflow_run_id,
                    &step.id.to_string(),
                    state,
                    None,
                )?;
            }
            if target_state == WorkflowStepState::Succeeded {
                let refreshed = self.get_workflow_run(workflow_run_id)?;
                self.materialize_workflow_step_completion(
                    workflow,
                    &refreshed,
                    step.id,
                    None,
                    "registry:sync_workflow_run_from_synthetic_flow",
                )?;
                run = self.get_workflow_run(workflow_run_id)?;
            }
        }

        if run.state == WorkflowRunState::Running && run.can_complete() {
            return self.complete_workflow_run(workflow_run_id);
        }

        if flow.state == FlowState::Aborted && !run.state.is_terminal() {
            return self.transition_workflow_run(
                workflow_run_id,
                WorkflowRunState::Aborted,
                Some("synthetic_flow_aborted"),
                false,
            );
        }

        Ok(run)
    }

    pub(crate) fn reconcile_workflow_bridge_for_flow(
        &self,
        flow_id: Uuid,
        origin: &'static str,
    ) -> Result<()> {
        let state = self.state()?;
        let Some(run) = Self::workflow_bridge_run_for_flow_state(&state, flow_id).cloned() else {
            return Ok(());
        };
        let workflow = self.get_workflow(&run.workflow_id.to_string())?;
        let workflow_run_id = run.id.to_string();
        let _ = self.sync_workflow_run_from_synthetic_flow(&workflow_run_id, &workflow)?;
        self.process_ready_join_steps(&workflow_run_id, &workflow, origin)?;
        let _ = self.sync_workflow_run_from_synthetic_flow(&workflow_run_id, &workflow)?;
        Ok(())
    }

    fn transition_workflow_run(
        &self,
        workflow_run_id: &str,
        target_state: WorkflowRunState,
        reason: Option<&str>,
        forced: bool,
    ) -> Result<WorkflowRun> {
        let run = self
            .get_workflow_run(workflow_run_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        if run.state == target_state {
            return Ok(run);
        }
        if !run.state.can_transition_to(target_state) {
            let err = HivemindError::user(
                "invalid_workflow_run_transition",
                format!(
                    "Cannot transition workflow run from {:?} to {target_state:?}",
                    run.state
                ),
                "registry:transition_workflow_run",
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_workflow_run(run.project_id, run.workflow_id, run.id),
            );
            return Err(err);
        }

        let payload = match target_state {
            WorkflowRunState::Running if run.state == WorkflowRunState::Paused => {
                EventPayload::WorkflowRunResumed {
                    workflow_run_id: run.id,
                }
            }
            WorkflowRunState::Running => EventPayload::WorkflowRunStarted {
                workflow_run_id: run.id,
            },
            WorkflowRunState::Paused => EventPayload::WorkflowRunPaused {
                workflow_run_id: run.id,
            },
            WorkflowRunState::Completed => EventPayload::WorkflowRunCompleted {
                workflow_run_id: run.id,
            },
            WorkflowRunState::Aborted => EventPayload::WorkflowRunAborted {
                workflow_run_id: run.id,
                reason: reason.map(str::to_string),
                forced,
            },
            WorkflowRunState::Created => {
                unreachable!("created is only used for run initialization")
            }
        };

        self.append_event(
            Event::new(
                payload,
                CorrelationIds::for_workflow_run(run.project_id, run.workflow_id, run.id),
            ),
            "registry:transition_workflow_run",
        )?;

        self.get_workflow_run(workflow_run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::registry::RegistryConfig;
    use crate::core::scope::RepoAccessMode;

    #[test]
    fn workflow_registry_roundtrip_supports_create_run_and_start() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        registry
            .project_runtime_set(
                &project.id.to_string(),
                "native",
                "builtin-native",
                None,
                &[],
                &[],
                60_000,
                1,
            )
            .unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("README.md"), "seed\n").unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "init",
            ])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        registry
            .attach_repo(
                &project.id.to_string(),
                repo_dir.to_str().unwrap(),
                Some("repo"),
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("foundation"))
            .unwrap();
        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        let started = registry.start_workflow_run(&run.id.to_string()).unwrap();

        assert_eq!(workflow.project_id, project.id);
        assert_eq!(run.workflow_id, workflow.id);
        assert_eq!(started.state, WorkflowRunState::Running);
    }

    #[test]
    fn workflow_registry_supports_step_authoring_and_completion() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("foundation"))
            .unwrap();

        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "first-step",
                WorkflowStepKind::Task,
                Some("demo task"),
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let step_id = workflow.steps.values().next().unwrap().id;

        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        registry.start_workflow_run(&run.id.to_string()).unwrap();
        registry
            .workflow_step_set_state(
                &run.id.to_string(),
                &step_id.to_string(),
                WorkflowStepState::Ready,
                None,
            )
            .unwrap_err();
        registry
            .workflow_step_set_state(
                &run.id.to_string(),
                &step_id.to_string(),
                WorkflowStepState::Running,
                None,
            )
            .unwrap();
        registry
            .workflow_step_set_state(
                &run.id.to_string(),
                &step_id.to_string(),
                WorkflowStepState::Succeeded,
                None,
            )
            .unwrap();

        let completed = registry.complete_workflow_run(&run.id.to_string()).unwrap();
        assert_eq!(completed.state, WorkflowRunState::Completed);
    }

    #[test]
    #[allow(clippy::similar_names, clippy::too_many_lines)]
    fn workflow_tick_bridges_task_steps_into_synthetic_flow_execution() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();

        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("execution"))
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "step-a",
                WorkflowStepKind::Task,
                Some("first step"),
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let step_a = workflow
            .steps
            .values()
            .find(|step| step.name == "step-a")
            .unwrap()
            .id;
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "step-b",
                WorkflowStepKind::Task,
                Some("second step"),
                &[step_a.to_string()],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let step_b = workflow
            .steps
            .values()
            .find(|step| step.name == "step-b")
            .unwrap()
            .id;

        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        let run = registry.start_workflow_run(&run.id.to_string()).unwrap();
        let flow_id = registry
            .ensure_synthetic_flow_for_workflow_run(&run, &workflow)
            .unwrap();
        registry.start_flow(&flow_id.to_string()).unwrap();

        let step_a_task_id = Registry::workflow_bridge_task_id(run.id, step_a);
        let step_b_task_id = Registry::workflow_bridge_task_id(run.id, step_b);
        let corr = Registry::workflow_bridge_corr(
            &run,
            Registry::workflow_bridge_graph_id(run.id),
            Some(flow_id),
            None,
            None,
            None,
        );

        registry
            .append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id,
                        task_id: step_a_task_id,
                        attempt_id: None,
                        from: TaskExecState::Ready,
                        to: TaskExecState::Running,
                    },
                    corr.clone(),
                ),
                "test:workflow_tick_bridges",
            )
            .unwrap();
        registry
            .append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id,
                        task_id: step_a_task_id,
                        attempt_id: None,
                        from: TaskExecState::Running,
                        to: TaskExecState::Verifying,
                    },
                    corr.clone(),
                ),
                "test:workflow_tick_bridges",
            )
            .unwrap();
        registry
            .append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id,
                        task_id: step_a_task_id,
                        attempt_id: None,
                        from: TaskExecState::Verifying,
                        to: TaskExecState::Success,
                    },
                    corr.clone(),
                ),
                "test:workflow_tick_bridges",
            )
            .unwrap();
        registry
            .append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id,
                        task_id: step_b_task_id,
                        attempt_id: None,
                        from: TaskExecState::Pending,
                        to: TaskExecState::Ready,
                    },
                    corr.clone(),
                ),
                "test:workflow_tick_bridges",
            )
            .unwrap();

        let after_step_a = registry
            .sync_workflow_run_from_synthetic_flow(&run.id.to_string(), &workflow)
            .unwrap();
        assert_eq!(
            after_step_a.step_runs.get(&step_a).unwrap().state,
            WorkflowStepState::Succeeded
        );
        assert_eq!(
            after_step_a.step_runs.get(&step_b).unwrap().state,
            WorkflowStepState::Ready
        );

        for (from, to) in [
            (TaskExecState::Ready, TaskExecState::Running),
            (TaskExecState::Running, TaskExecState::Verifying),
            (TaskExecState::Verifying, TaskExecState::Success),
        ] {
            registry
                .append_event(
                    Event::new(
                        EventPayload::TaskExecutionStateChanged {
                            flow_id,
                            task_id: step_b_task_id,
                            attempt_id: None,
                            from,
                            to,
                        },
                        corr.clone(),
                    ),
                    "test:workflow_tick_bridges",
                )
                .unwrap();
        }
        registry
            .append_event(
                Event::new(EventPayload::TaskFlowCompleted { flow_id }, corr),
                "test:workflow_tick_bridges",
            )
            .unwrap();

        let completed = registry
            .sync_workflow_run_from_synthetic_flow(&run.id.to_string(), &workflow)
            .unwrap();
        assert_eq!(completed.state, WorkflowRunState::Completed);
        assert_eq!(
            completed.step_runs.get(&step_b).unwrap().state,
            WorkflowStepState::Succeeded
        );
    }

    #[test]
    fn workflow_lifecycle_delegates_to_synthetic_flow() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();

        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("execution"))
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "step-a",
                WorkflowStepKind::Task,
                Some("first step"),
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "step-b",
                WorkflowStepKind::Task,
                Some("second step"),
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();

        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        let run = registry.start_workflow_run(&run.id.to_string()).unwrap();
        let flow_id = registry
            .ensure_synthetic_flow_for_workflow_run(&run, &workflow)
            .unwrap();
        registry.start_flow(&flow_id.to_string()).unwrap();

        let paused = registry.pause_workflow_run(&run.id.to_string()).unwrap();
        assert_eq!(paused.state, WorkflowRunState::Paused);
        assert_eq!(
            registry.get_flow(&flow_id.to_string()).unwrap().state,
            FlowState::Paused
        );

        let resumed = registry.resume_workflow_run(&run.id.to_string()).unwrap();
        assert_eq!(resumed.state, WorkflowRunState::Running);
        assert_eq!(
            registry.get_flow(&flow_id.to_string()).unwrap().state,
            FlowState::Running
        );

        let aborted = registry
            .abort_workflow_run(&run.id.to_string(), Some("user requested"), false)
            .unwrap();
        assert_eq!(aborted.state, WorkflowRunState::Aborted);
        assert_eq!(
            registry.get_flow(&flow_id.to_string()).unwrap().state,
            FlowState::Aborted
        );
    }

    #[test]
    fn join_steps_stay_ready_until_native_join_execution_runs() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();

        let workflow = registry
            .create_workflow(&project.id.to_string(), "join-workflow", Some("join"))
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "branch-a",
                WorkflowStepKind::Task,
                Some("a"),
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let branch_a = workflow
            .steps
            .values()
            .find(|step| step.name == "branch-a")
            .unwrap()
            .id;
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "branch-b",
                WorkflowStepKind::Task,
                Some("b"),
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let branch_b = workflow
            .steps
            .values()
            .find(|step| step.name == "branch-b")
            .unwrap()
            .id;
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "join",
                WorkflowStepKind::Join,
                Some("join"),
                &[branch_a.to_string(), branch_b.to_string()],
                vec![WorkflowStepInputBinding {
                    name: "combined".to_string(),
                    source: WorkflowStepInputSource::Bag {
                        selector: WorkflowBagSelector {
                            output_name: "branch".to_string(),
                            producer_step_ids: vec![branch_a, branch_b],
                            tags: vec!["branch".to_string()],
                            expected_schema: Some("text/plain".to_string()),
                            expected_schema_version: Some(1),
                        },
                        reducer: WorkflowBagReducer::ReduceFunction {
                            function: WorkflowReduceFunction::ConcatTextNewline,
                        },
                    },
                }],
                vec![WorkflowStepOutputBinding {
                    name: "summary".to_string(),
                    source: WorkflowValueSource::InputBinding {
                        binding: "combined".to_string(),
                    },
                    tags: Vec::new(),
                }],
                vec![WorkflowContextPatchBinding {
                    key: "joined_summary".to_string(),
                    source: WorkflowValueSource::InputBinding {
                        binding: "combined".to_string(),
                    },
                }],
                None,
                None,
                None,
            )
            .unwrap();
        let join = workflow
            .steps
            .values()
            .find(|step| step.name == "join")
            .unwrap()
            .id;

        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        let run = registry.start_workflow_run(&run.id.to_string()).unwrap();
        let flow_id = registry
            .ensure_synthetic_flow_for_workflow_run(&run, &workflow)
            .unwrap();
        registry.start_flow(&flow_id.to_string()).unwrap();

        let graph_id = flow_id;
        let corr = Registry::workflow_bridge_corr(&run, graph_id, Some(flow_id), None, None, None);
        let join_task_id = Registry::workflow_bridge_task_id(run.id, join);
        let left_branch_task_id = Registry::workflow_bridge_task_id(run.id, branch_a);
        let right_branch_task_id = Registry::workflow_bridge_task_id(run.id, branch_b);

        for task_id in [left_branch_task_id, right_branch_task_id] {
            for (from, to) in [
                (TaskExecState::Pending, TaskExecState::Ready),
                (TaskExecState::Ready, TaskExecState::Running),
                (TaskExecState::Running, TaskExecState::Verifying),
                (TaskExecState::Verifying, TaskExecState::Success),
            ] {
                registry
                    .append_event(
                        Event::new(
                            EventPayload::TaskExecutionStateChanged {
                                flow_id,
                                task_id,
                                attempt_id: None,
                                from,
                                to,
                            },
                            corr.clone(),
                        ),
                        "test:join_steps_stay_ready_until_native_join_execution_runs",
                    )
                    .unwrap();
            }
        }
        for (from, to) in [
            (TaskExecState::Pending, TaskExecState::Ready),
            (TaskExecState::Ready, TaskExecState::Running),
        ] {
            registry
                .append_event(
                    Event::new(
                        EventPayload::TaskExecutionStateChanged {
                            flow_id,
                            task_id: join_task_id,
                            attempt_id: None,
                            from,
                            to,
                        },
                        corr.clone(),
                    ),
                    "test:join_steps_stay_ready_until_native_join_execution_runs",
                )
                .unwrap();
        }

        let synced = registry
            .sync_workflow_run_from_synthetic_flow(&run.id.to_string(), &workflow)
            .unwrap();
        assert_eq!(
            synced.step_runs.get(&branch_a).unwrap().state,
            WorkflowStepState::Succeeded
        );
        assert_eq!(
            synced.step_runs.get(&branch_b).unwrap().state,
            WorkflowStepState::Succeeded
        );
        assert_eq!(
            synced.step_runs.get(&join).unwrap().state,
            WorkflowStepState::Ready
        );
    }

    #[test]
    fn reconcile_workflow_bridge_for_flow_completes_join_after_branch_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();

        let workflow = registry
            .create_workflow(&project.id.to_string(), "join-workflow", Some("join"))
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "branch-a",
                WorkflowStepKind::Task,
                Some("a"),
                &[],
                Vec::new(),
                vec![WorkflowStepOutputBinding {
                    name: "branch".to_string(),
                    source: WorkflowValueSource::Literal {
                        value: WorkflowDataValue::new(
                            "text/plain",
                            1,
                            serde_json::Value::String("alpha".into()),
                        )
                        .unwrap(),
                    },
                    tags: vec!["branch".to_string()],
                }],
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let branch_a = workflow
            .steps
            .values()
            .find(|step| step.name == "branch-a")
            .unwrap()
            .id;
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "branch-b",
                WorkflowStepKind::Task,
                Some("b"),
                &[],
                Vec::new(),
                vec![WorkflowStepOutputBinding {
                    name: "branch".to_string(),
                    source: WorkflowValueSource::Literal {
                        value: WorkflowDataValue::new(
                            "text/plain",
                            1,
                            serde_json::Value::String("beta".into()),
                        )
                        .unwrap(),
                    },
                    tags: vec!["branch".to_string()],
                }],
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let branch_b = workflow
            .steps
            .values()
            .find(|step| step.name == "branch-b")
            .unwrap()
            .id;
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "join",
                WorkflowStepKind::Join,
                Some("join"),
                &[branch_a.to_string(), branch_b.to_string()],
                vec![WorkflowStepInputBinding {
                    name: "combined".to_string(),
                    source: WorkflowStepInputSource::Bag {
                        selector: WorkflowBagSelector {
                            output_name: "branch".to_string(),
                            producer_step_ids: vec![branch_a, branch_b],
                            tags: vec!["branch".to_string()],
                            expected_schema: Some("text/plain".to_string()),
                            expected_schema_version: Some(1),
                        },
                        reducer: WorkflowBagReducer::ReduceFunction {
                            function: WorkflowReduceFunction::ConcatTextNewline,
                        },
                    },
                }],
                vec![WorkflowStepOutputBinding {
                    name: "summary".to_string(),
                    source: WorkflowValueSource::InputBinding {
                        binding: "combined".to_string(),
                    },
                    tags: Vec::new(),
                }],
                vec![WorkflowContextPatchBinding {
                    key: "joined_summary".to_string(),
                    source: WorkflowValueSource::InputBinding {
                        binding: "combined".to_string(),
                    },
                }],
                None,
                None,
                None,
            )
            .unwrap();
        let join = workflow
            .steps
            .values()
            .find(|step| step.name == "join")
            .unwrap()
            .id;

        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        let run = registry.start_workflow_run(&run.id.to_string()).unwrap();
        let flow_id = registry
            .ensure_synthetic_flow_for_workflow_run(&run, &workflow)
            .unwrap();
        registry.start_flow(&flow_id.to_string()).unwrap();

        let corr = Registry::workflow_bridge_corr(&run, flow_id, Some(flow_id), None, None, None);
        for step_id in [branch_a, branch_b] {
            let task_id = Registry::workflow_bridge_task_id(run.id, step_id);
            for (from, to) in [
                (TaskExecState::Pending, TaskExecState::Ready),
                (TaskExecState::Ready, TaskExecState::Running),
                (TaskExecState::Running, TaskExecState::Verifying),
                (TaskExecState::Verifying, TaskExecState::Success),
            ] {
                registry
                    .append_event(
                        Event::new(
                            EventPayload::TaskExecutionStateChanged {
                                flow_id,
                                task_id,
                                attempt_id: None,
                                from,
                                to,
                            },
                            corr.clone(),
                        ),
                        "test:reconcile_workflow_bridge_for_flow_completes_join_after_branch_recovery",
                    )
                    .unwrap();
            }
        }
        let join_task_id = Registry::workflow_bridge_task_id(run.id, join);
        registry
            .append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id,
                        task_id: join_task_id,
                        attempt_id: None,
                        from: TaskExecState::Pending,
                        to: TaskExecState::Ready,
                    },
                    corr,
                ),
                "test:reconcile_workflow_bridge_for_flow_completes_join_after_branch_recovery",
            )
            .unwrap();
        registry
            .reconcile_workflow_bridge_for_flow(
                flow_id,
                "test:reconcile_workflow_bridge_for_flow_completes_join_after_branch_recovery",
            )
            .unwrap();

        let reconciled = registry.get_workflow_run(&run.id.to_string()).unwrap();
        assert_eq!(
            reconciled.step_runs.get(&join).unwrap().state,
            WorkflowStepState::Succeeded
        );
        assert_eq!(reconciled.output_bag.entries.len(), 3);
        assert_eq!(
            reconciled
                .context
                .current_snapshot
                .values
                .get("joined_summary")
                .unwrap()
                .value,
            serde_json::Value::String("alpha\nbeta".to_string())
        );
    }

    #[test]
    fn workflow_add_step_deduplicates_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("foundation"))
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "root-step",
                WorkflowStepKind::Task,
                None,
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let root_step_id = workflow
            .steps
            .values()
            .find(|step| step.name == "root-step")
            .unwrap()
            .id;

        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "dependent-step",
                WorkflowStepKind::Task,
                None,
                &[root_step_id.to_string(), root_step_id.to_string()],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let dependent = workflow
            .steps
            .values()
            .find(|step| step.name == "dependent-step")
            .unwrap();

        assert_eq!(dependent.depends_on, vec![root_step_id]);
    }

    #[test]
    fn workflow_add_step_rejects_unknown_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("foundation"))
            .unwrap();

        let err = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "dependent-step",
                WorkflowStepKind::Task,
                None,
                &[Uuid::new_v4().to_string()],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .expect_err("unknown dependency should fail");

        assert_eq!(err.code, "workflow_step_dependency_not_found");
    }

    #[test]
    fn workflow_add_step_requires_child_workflow_for_workflow_steps() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("execution"))
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "child-workflow",
                WorkflowStepKind::Workflow,
                Some("nested child"),
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .expect_err("workflow step should require child target");

        assert_eq!(workflow.code, "workflow_child_workflow_required");
    }

    #[test]
    fn workflow_tick_completes_empty_workflows_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        let workflow = registry
            .create_workflow(&project.id.to_string(), "empty-workflow", Some("execution"))
            .unwrap();

        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        registry.start_workflow_run(&run.id.to_string()).unwrap();

        let completed = registry
            .tick_workflow_run(&run.id.to_string(), false, Some(1))
            .unwrap();

        assert_eq!(completed.state, WorkflowRunState::Completed);
    }

    #[test]
    fn workflow_bridge_correlation_for_flow_id_event_includes_workflow_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("execution"))
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "step-a",
                WorkflowStepKind::Task,
                None,
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();

        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        let run = registry.start_workflow_run(&run.id.to_string()).unwrap();
        let flow_id = registry
            .ensure_synthetic_flow_for_workflow_run(&run, &workflow)
            .unwrap();
        let graph_id = Registry::workflow_bridge_graph_id(run.id);
        let corr = Registry::correlation_for_graph_flow_id_event(
            &registry.state().unwrap(),
            project.id,
            graph_id,
            flow_id,
        );

        assert_eq!(corr.project_id, Some(project.id));
        assert_eq!(corr.graph_id, Some(graph_id));
        assert_eq!(corr.flow_id, Some(flow_id));
        assert_eq!(corr.workflow_id, Some(workflow.id));
        assert_eq!(corr.workflow_run_id, Some(run.id));
        assert_eq!(corr.root_workflow_run_id, Some(run.root_workflow_run_id));
    }

    #[test]
    fn workflow_tick_launches_child_workflow_and_maps_child_context_back_to_parent() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();

        let child = registry
            .create_workflow(&project.id.to_string(), "child-workflow", Some("child"))
            .unwrap();
        let parent = registry
            .create_workflow(&project.id.to_string(), "parent-workflow", Some("parent"))
            .unwrap();
        let child_id = child.id.to_string();
        let parent = registry
            .workflow_add_step(
                &parent.id.to_string(),
                "child-step",
                WorkflowStepKind::Workflow,
                Some("launches child"),
                &[],
                vec![WorkflowStepInputBinding {
                    name: "goal".to_string(),
                    source: WorkflowStepInputSource::ContextKey {
                        key: "goal".to_string(),
                    },
                }],
                vec![WorkflowStepOutputBinding {
                    name: "child_goal".to_string(),
                    source: WorkflowValueSource::ChildContextKey {
                        key: "goal".to_string(),
                    },
                    tags: vec!["child".to_string()],
                }],
                vec![WorkflowContextPatchBinding {
                    key: "child_goal".to_string(),
                    source: WorkflowValueSource::ChildContextKey {
                        key: "goal".to_string(),
                    },
                }],
                Some(&child_id),
                None,
                None,
            )
            .unwrap();
        let step_id = parent.steps.values().next().unwrap().id;

        let run = registry
            .create_workflow_run(
                &parent.id.to_string(),
                None,
                None,
                BTreeMap::from([(
                    "goal".to_string(),
                    WorkflowDataValue::new(
                        "text/plain",
                        1,
                        serde_json::Value::String("ship-it".to_string()),
                    )
                    .unwrap(),
                )]),
            )
            .unwrap();
        registry.start_workflow_run(&run.id.to_string()).unwrap();

        let completed = registry
            .tick_workflow_run(&run.id.to_string(), false, Some(1))
            .unwrap();

        assert_eq!(completed.state, WorkflowRunState::Completed);
        assert_eq!(
            completed.step_runs.get(&step_id).unwrap().state,
            WorkflowStepState::Succeeded
        );
        assert_eq!(
            completed
                .context
                .current_snapshot
                .values
                .get("child_goal")
                .unwrap()
                .value,
            serde_json::Value::String("ship-it".to_string())
        );
        assert_eq!(completed.output_bag.entries.len(), 1);

        let state = registry.state().unwrap();
        let child_run = Registry::child_workflow_run_for_step(&state, &completed, step_id).unwrap();
        assert_eq!(child_run.parent_workflow_run_id, Some(completed.id));
        assert_eq!(child_run.parent_step_id, Some(step_id));
        assert_eq!(
            child_run.root_workflow_run_id,
            completed.root_workflow_run_id
        );
        assert_eq!(
            child_run
                .context
                .current_snapshot
                .values
                .get("goal")
                .unwrap()
                .value,
            serde_json::Value::String("ship-it".to_string())
        );
    }

    #[test]
    fn child_run_terminal_behavior_uses_timeout_policy_when_runtime_timeout_is_observed() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();

        let child = registry
            .create_workflow(&project.id.to_string(), "child-workflow", Some("child"))
            .unwrap();
        let parent = registry
            .create_workflow(&project.id.to_string(), "parent-workflow", Some("parent"))
            .unwrap();
        let child_id = child.id.to_string();
        let parent = registry
            .workflow_add_step(
                &parent.id.to_string(),
                "child-step",
                WorkflowStepKind::Workflow,
                None,
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Some(&child_id),
                None,
                None,
            )
            .unwrap();
        let step_id = parent.steps.values().next().unwrap().id;

        let mut updated = parent.clone();
        updated
            .steps
            .get_mut(&step_id)
            .unwrap()
            .child_workflow
            .as_mut()
            .unwrap()
            .failure_policy
            .on_timeout = WorkflowChildTerminalBehavior::AbortParent;
        registry
            .append_event(
                Event::new(
                    EventPayload::WorkflowDefinitionUpdated {
                        definition: updated,
                    },
                    CorrelationIds::for_workflow(project.id, parent.id),
                ),
                "test:child_run_terminal_behavior_uses_timeout_policy_when_runtime_timeout_is_observed",
            )
            .unwrap();

        let parent_run = registry
            .create_workflow_run(&parent.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        let child_run = registry
            .create_workflow_run_internal(
                &child,
                None,
                None,
                BTreeMap::new(),
                Some(parent_run.id),
                Some(parent_run.id),
                Some(step_id),
                "test:child_run_terminal_behavior_uses_timeout_policy_when_runtime_timeout_is_observed",
            )
            .unwrap();
        registry
            .append_event(
                Event::new(
                    EventPayload::RuntimeTerminated {
                        attempt_id: Uuid::new_v4(),
                        reason: "no_observable_progress_timeout".to_string(),
                    },
                    CorrelationIds {
                        project_id: Some(project.id),
                        graph_id: None,
                        flow_id: None,
                        workflow_id: Some(child.id),
                        workflow_run_id: Some(child_run.id),
                        root_workflow_run_id: Some(parent_run.id),
                        parent_workflow_run_id: Some(parent_run.id),
                        task_id: None,
                        step_id: Some(step_id),
                        step_run_id: None,
                        attempt_id: None,
                    },
                ),
                "test:child_run_terminal_behavior_uses_timeout_policy_when_runtime_timeout_is_observed",
            )
            .unwrap();
        let child_run = registry
            .transition_workflow_run(
                &child_run.id.to_string(),
                WorkflowRunState::Aborted,
                Some("synthetic_flow_aborted"),
                false,
            )
            .unwrap();

        let behavior = registry.child_run_terminal_behavior(&child_run).unwrap();
        assert_eq!(behavior, WorkflowChildTerminalBehavior::AbortParent);
    }

    #[test]
    fn workflow_child_timeout_policy_can_abort_parent_run() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();

        let child = registry
            .create_workflow(&project.id.to_string(), "child-workflow", Some("child"))
            .unwrap();
        let child = registry
            .workflow_add_step(
                &child.id.to_string(),
                "leaf",
                WorkflowStepKind::Task,
                None,
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                None,
            )
            .unwrap();
        let parent = registry
            .create_workflow(&project.id.to_string(), "parent-workflow", Some("parent"))
            .unwrap();
        let child_id = child.id.to_string();
        let parent = registry
            .workflow_add_step(
                &parent.id.to_string(),
                "child-step",
                WorkflowStepKind::Workflow,
                None,
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Some(&child_id),
                None,
                None,
            )
            .unwrap();
        let step_id = parent.steps.values().next().unwrap().id;

        let mut updated = parent.clone();
        updated
            .steps
            .get_mut(&step_id)
            .unwrap()
            .child_workflow
            .as_mut()
            .unwrap()
            .failure_policy
            .on_timeout = WorkflowChildTerminalBehavior::AbortParent;
        registry
            .append_event(
                Event::new(
                    EventPayload::WorkflowDefinitionUpdated {
                        definition: updated.clone(),
                    },
                    CorrelationIds::for_workflow(project.id, parent.id),
                ),
                "test:workflow_child_timeout_policy_can_abort_parent_run",
            )
            .unwrap();

        let parent_run = registry
            .create_workflow_run(&parent.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        let parent_run = registry
            .start_workflow_run(&parent_run.id.to_string())
            .unwrap();
        registry
            .workflow_step_set_state(
                &parent_run.id.to_string(),
                &step_id.to_string(),
                WorkflowStepState::Running,
                None,
            )
            .unwrap();
        registry
            .workflow_step_set_state(
                &parent_run.id.to_string(),
                &step_id.to_string(),
                WorkflowStepState::Waiting,
                None,
            )
            .unwrap();
        let child_run = registry
            .create_workflow_run_internal(
                &child,
                None,
                None,
                BTreeMap::new(),
                Some(parent_run.id),
                Some(parent_run.id),
                Some(step_id),
                "test:workflow_child_timeout_policy_can_abort_parent_run",
            )
            .unwrap();
        let child_run = registry
            .start_workflow_run(&child_run.id.to_string())
            .unwrap();
        registry
            .append_event(
                Event::new(
                    EventPayload::RuntimeTerminated {
                        attempt_id: Uuid::new_v4(),
                        reason: "no_observable_progress_timeout".to_string(),
                    },
                    CorrelationIds {
                        project_id: Some(project.id),
                        graph_id: None,
                        flow_id: None,
                        workflow_id: Some(child.id),
                        workflow_run_id: Some(child_run.id),
                        root_workflow_run_id: Some(parent_run.id),
                        parent_workflow_run_id: Some(parent_run.id),
                        task_id: None,
                        step_id: Some(step_id),
                        step_run_id: None,
                        attempt_id: None,
                    },
                ),
                "test:workflow_child_timeout_policy_can_abort_parent_run",
            )
            .unwrap();
        let _ = registry
            .transition_workflow_run(
                &child_run.id.to_string(),
                WorkflowRunState::Aborted,
                Some("synthetic_flow_aborted"),
                false,
            )
            .unwrap();

        let aborted_parent = registry
            .process_workflow_child_steps(
                &parent_run.id.to_string(),
                &updated,
                "test:workflow_child_timeout_policy_can_abort_parent_run",
            )
            .unwrap();
        assert_eq!(aborted_parent.state, WorkflowRunState::Aborted);
    }

    #[test]
    fn conditional_and_wait_steps_are_typed_and_signal_driven() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        let workflow = registry
            .create_workflow(&project.id.to_string(), "control-workflow", Some("control"))
            .unwrap();
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "gate",
                WorkflowStepKind::Conditional,
                None,
                &[],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                Some(WorkflowConditionalConfig {
                    branches: vec![WorkflowConditionalBranch {
                        name: "approved".to_string(),
                        condition: WorkflowConditionExpression::ContextValueEquals {
                            key: "approved".to_string(),
                            value: WorkflowDataValue::new(
                                "application/json",
                                1,
                                serde_json::Value::Bool(true),
                            )
                            .unwrap(),
                        },
                        activate_step_ids: Vec::new(),
                    }],
                    default_branch: Some("approved".to_string()),
                }),
                None,
            )
            .unwrap();
        let gate_id = workflow.steps.values().next().unwrap().id;
        let workflow = registry
            .workflow_add_step(
                &workflow.id.to_string(),
                "approval-wait",
                WorkflowStepKind::Wait,
                None,
                &[gate_id.to_string()],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                None,
                Some(WorkflowWaitConfig {
                    condition: WorkflowWaitCondition::HumanSignal {
                        signal_name: "approve".to_string(),
                    },
                }),
            )
            .unwrap();
        let wait_id = workflow
            .steps
            .values()
            .find(|step| step.name == "approval-wait")
            .unwrap()
            .id;

        let run = registry
            .create_workflow_run(
                &workflow.id.to_string(),
                None,
                None,
                BTreeMap::from([(
                    "approved".to_string(),
                    WorkflowDataValue::new("application/json", 1, serde_json::Value::Bool(true))
                        .unwrap(),
                )]),
            )
            .unwrap();
        registry.start_workflow_run(&run.id.to_string()).unwrap();

        let waiting = registry
            .tick_workflow_run(&run.id.to_string(), false, Some(1))
            .unwrap();
        assert_eq!(
            waiting.step_runs.get(&gate_id).unwrap().state,
            WorkflowStepState::Succeeded
        );
        assert_eq!(
            waiting.step_runs.get(&wait_id).unwrap().state,
            WorkflowStepState::Waiting
        );
        assert!(waiting
            .step_runs
            .get(&wait_id)
            .unwrap()
            .wait_status
            .is_some());

        let signaled = registry
            .signal_workflow_run(
                &run.id.to_string(),
                "approve",
                "approval-1",
                None,
                Some(&wait_id.to_string()),
                "human",
            )
            .unwrap();
        assert_eq!(
            signaled.step_runs.get(&wait_id).unwrap().state,
            WorkflowStepState::Succeeded
        );

        let completed = registry
            .tick_workflow_run(&run.id.to_string(), false, Some(1))
            .unwrap();
        assert_eq!(completed.state, WorkflowRunState::Completed);

        let events = registry
            .read_events(&EventFilter {
                workflow_run_id: Some(run.id),
                ..EventFilter::default()
            })
            .unwrap();
        assert!(events.iter().any(|event| matches!(
            event.payload,
            EventPayload::WorkflowConditionEvaluated { .. }
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event.payload, EventPayload::WorkflowWaitActivated { .. })));
        assert!(events
            .iter()
            .any(|event| matches!(event.payload, EventPayload::WorkflowSignalReceived { .. })));
        assert!(events
            .iter()
            .any(|event| matches!(event.payload, EventPayload::WorkflowWaitCompleted { .. })));
    }

    #[test]
    fn duplicate_workflow_signal_idempotency_key_fails_loudly() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();
        let workflow = registry
            .create_workflow(&project.id.to_string(), "signal-workflow", Some("signal"))
            .unwrap();
        let run = registry
            .create_workflow_run(&workflow.id.to_string(), None, None, BTreeMap::new())
            .unwrap();
        registry.start_workflow_run(&run.id.to_string()).unwrap();

        registry
            .signal_workflow_run(
                &run.id.to_string(),
                "resume",
                "dup-key",
                None,
                None,
                "human",
            )
            .unwrap();
        let err = registry
            .signal_workflow_run(
                &run.id.to_string(),
                "resume",
                "dup-key",
                None,
                None,
                "human",
            )
            .expect_err("duplicate idempotency key should fail");
        assert_eq!(err.code, "workflow_signal_duplicate");
    }
}
