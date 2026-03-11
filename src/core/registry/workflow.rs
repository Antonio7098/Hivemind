//! Workflow registry operations.

#![allow(clippy::wildcard_imports, unused_imports)]

use crate::core::registry::shared_prelude::*;
use crate::core::registry::Registry;
use crate::core::workflow::{
    WorkflowDefinition, WorkflowRun, WorkflowRunState, WorkflowStepDefinition, WorkflowStepKind,
    WorkflowStepState,
};

impl Registry {
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

        let mut step = WorkflowStepDefinition::new(step_name, kind);
        step.description = description.map(str::to_string);
        step.depends_on = dependency_ids;

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

    pub fn create_workflow_run(&self, workflow_id_or_name: &str) -> Result<WorkflowRun> {
        let workflow = self
            .get_workflow(workflow_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let run = WorkflowRun::new_root(&workflow);
        self.append_event(
            Event::new(
                EventPayload::WorkflowRunCreated { run: run.clone() },
                CorrelationIds::for_workflow_run(workflow.project_id, workflow.id, run.id),
            ),
            "registry:create_workflow_run",
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
        self.transition_workflow_run(workflow_run_id, WorkflowRunState::Paused, None, false)
    }

    pub fn resume_workflow_run(&self, workflow_run_id: &str) -> Result<WorkflowRun> {
        self.transition_workflow_run(workflow_run_id, WorkflowRunState::Running, None, false)
    }

    pub fn abort_workflow_run(
        &self,
        workflow_run_id: &str,
        reason: Option<&str>,
        forced: bool,
    ) -> Result<WorkflowRun> {
        self.transition_workflow_run(workflow_run_id, WorkflowRunState::Aborted, reason, forced)
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

    #[test]
    fn workflow_registry_roundtrip_supports_create_run_and_start() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(dir.path().to_path_buf())).unwrap();
        let project = registry.create_project("workflow-project", None).unwrap();

        let workflow = registry
            .create_workflow(&project.id.to_string(), "demo-workflow", Some("foundation"))
            .unwrap();
        let run = registry
            .create_workflow_run(&workflow.id.to_string())
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
            )
            .unwrap();
        let step_id = workflow.steps.values().next().unwrap().id;

        let run = registry
            .create_workflow_run(&workflow.id.to_string())
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
}
