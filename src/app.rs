//! Application wiring and service-level accessors.

use crate::core::error::{HivemindError, Result};
use crate::core::events::{Event, RuntimeRole, RuntimeSelectionSource};
use crate::core::flow::{RetryMode, RunMode, TaskFlow};
use crate::core::graph::TaskGraph;
use crate::core::graph_query::{GraphQueryRequest, GraphQueryResult};
use crate::core::registry::{
    shared_types::{RuntimeStreamDetailLevel, RuntimeStreamItemView},
    AttemptListItem, CheckpointCompletionResult, EventsRecoverResult, EventsVerifyResult,
    GlobalSkillInspectResult, GlobalSkillSummary, GlobalSystemPromptInspectResult,
    GlobalSystemPromptSummary, GlobalTemplateInspectResult, GlobalTemplateSummary,
    GovernanceArtifactDeleteResult, GovernanceAttachmentUpdateResult, GovernanceNotepadResult,
    GraphSnapshotRefreshResult, GraphValidationResult, MergeExecuteOptions,
    ProjectConstitutionCheckResult, ProjectConstitutionMutationResult,
    ProjectConstitutionShowResult, ProjectConstitutionValidationResult,
    ProjectGovernanceDiagnosticsResult, ProjectGovernanceDocumentInspectResult,
    ProjectGovernanceDocumentSummary, ProjectGovernanceDocumentWriteResult,
    ProjectGovernanceInitResult, ProjectGovernanceInspectResult, ProjectGovernanceMigrateResult,
    ProjectGovernanceRepairApplyResult, ProjectGovernanceRepairPlanResult,
    ProjectGovernanceReplayResult, ProjectGovernanceSnapshotCreateResult,
    ProjectGovernanceSnapshotListResult, ProjectGovernanceSnapshotRestoreResult, Registry,
    RegistryConfig, RuntimeHealthStatus, RuntimeListEntry, TemplateInstantiationResult,
    WorktreeCleanupResult, WorktreeTurnRestoreResult,
};
use crate::core::scope::{RepoAccessMode, Scope};
use crate::core::state::{
    AppState, AttemptCheckpoint, AttemptState, MergeState, Project, ProjectRuntimeConfig, Task,
    TaskState,
};
use crate::core::verification::CheckConfig;
use crate::core::workflow::WorkflowRun;
use crate::core::worktree::WorktreeStatus;
use crate::storage::event_store::EventFilter;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AppContext {
    registry_config: RegistryConfig,
}

impl Default for AppContext {
    fn default() -> Self {
        Self::from_env()
    }
}

impl AppContext {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            registry_config: RegistryConfig::default_dir(),
        }
    }

    #[must_use]
    pub fn with_registry_config(registry_config: RegistryConfig) -> Self {
        Self { registry_config }
    }

    #[must_use]
    pub fn registry_config(&self) -> &RegistryConfig {
        &self.registry_config
    }

    /// Open a registry using the configured application wiring.
    ///
    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn open_registry(&self) -> Result<Registry> {
        Registry::open_with_config(self.registry_config.clone())
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn flow_service(&self) -> Result<FlowService> {
        Ok(FlowService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn graph_service(&self) -> Result<GraphService> {
        Ok(GraphService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn runtime_service(&self) -> Result<RuntimeService> {
        Ok(RuntimeService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn project_service(&self) -> Result<ProjectService> {
        Ok(ProjectService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn governance_service(&self) -> Result<GovernanceService> {
        Ok(GovernanceService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn merge_service(&self) -> Result<MergeService> {
        Ok(MergeService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn verification_service(&self) -> Result<VerificationService> {
        Ok(VerificationService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn checkpoint_service(&self) -> Result<CheckpointService> {
        Ok(CheckpointService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn task_service(&self) -> Result<TaskService> {
        Ok(TaskService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn attempt_service(&self) -> Result<AttemptService> {
        Ok(AttemptService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn event_service(&self) -> Result<EventService> {
        Ok(EventService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn state_service(&self) -> Result<StateService> {
        Ok(StateService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn chat_service(&self) -> Result<ChatService> {
        Ok(ChatService::new(self.open_registry()?))
    }

    /// # Errors
    /// Returns an error if the configured registry store cannot be opened.
    pub fn worktree_service(&self) -> Result<WorktreeService> {
        Ok(WorktreeService::new(self.open_registry()?))
    }
}

pub struct FlowService {
    registry: Registry,
}

impl FlowService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn create_flow(&self, graph_id: &str, name: Option<&str>) -> Result<TaskFlow> {
        self.registry.create_flow(graph_id, name)
    }
    pub fn list_flows(&self, project: Option<&str>) -> Result<Vec<TaskFlow>> {
        self.registry.list_flows(project)
    }
    pub fn start_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        self.registry.start_flow(flow_id)
    }
    pub fn tick_flow(
        &self,
        flow_id: &str,
        interactive: bool,
        max_parallel: Option<u16>,
    ) -> Result<TaskFlow> {
        self.registry.tick_flow(flow_id, interactive, max_parallel)
    }
    pub fn pause_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        self.registry.pause_flow(flow_id)
    }
    pub fn resume_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        self.registry.resume_flow(flow_id)
    }
    pub fn abort_flow(
        &self,
        flow_id: &str,
        reason: Option<&str>,
        forced: bool,
    ) -> Result<TaskFlow> {
        self.registry.abort_flow(flow_id, reason, forced)
    }
    pub fn restart_flow(&self, flow_id: &str, name: Option<&str>, start: bool) -> Result<TaskFlow> {
        self.registry.restart_flow(flow_id, name, start)
    }
    pub fn get_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        self.registry.get_flow(flow_id)
    }
    pub fn flow_set_run_mode(&self, flow_id: &str, mode: RunMode) -> Result<TaskFlow> {
        self.registry.flow_set_run_mode(flow_id, mode)
    }
    pub fn flow_add_dependency(&self, flow_id: &str, depends_on: &str) -> Result<TaskFlow> {
        self.registry.flow_add_dependency(flow_id, depends_on)
    }
    pub fn flow_runtime_clear(&self, flow_id: &str, role: RuntimeRole) -> Result<TaskFlow> {
        self.registry.flow_runtime_clear(flow_id, role)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn flow_runtime_set(
        &self,
        flow_id: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env_pairs: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<TaskFlow> {
        self.registry.flow_runtime_set(
            flow_id,
            role,
            adapter,
            binary_path,
            model,
            args,
            env_pairs,
            timeout_ms,
            max_parallel_tasks,
        )
    }
    pub fn delete_flow(&self, flow_id: &str) -> Result<Uuid> {
        self.registry.delete_flow(flow_id)
    }
}

pub struct GraphService {
    registry: Registry,
}

impl GraphService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }
    pub fn create_graph(
        &self,
        project: &str,
        name: &str,
        from_tasks: &[Uuid],
    ) -> Result<TaskGraph> {
        self.registry.create_graph(project, name, from_tasks)
    }
    pub fn add_graph_dependency(
        &self,
        graph_id: &str,
        from_task: &str,
        to_task: &str,
    ) -> Result<TaskGraph> {
        self.registry
            .add_graph_dependency(graph_id, from_task, to_task)
    }
    pub fn add_graph_task_check(
        &self,
        graph_id: &str,
        task_id: &str,
        check: CheckConfig,
    ) -> Result<TaskGraph> {
        self.registry.add_graph_task_check(graph_id, task_id, check)
    }
    pub fn validate_graph(&self, graph_id: &str) -> Result<GraphValidationResult> {
        self.registry.validate_graph(graph_id)
    }
    pub fn list_graphs(&self, project: Option<&str>) -> Result<Vec<TaskGraph>> {
        self.registry.list_graphs(project)
    }
    pub fn delete_graph(&self, graph_id: &str) -> Result<Uuid> {
        self.registry.delete_graph(graph_id)
    }
    pub fn graph_snapshot_refresh(
        &self,
        project: &str,
        trigger: &str,
    ) -> Result<GraphSnapshotRefreshResult> {
        self.registry.graph_snapshot_refresh(project, trigger)
    }
    pub fn graph_query_execute(
        &self,
        project: &str,
        request: &GraphQueryRequest,
        source: &str,
    ) -> Result<GraphQueryResult> {
        self.registry.graph_query_execute(project, request, source)
    }
}

pub struct RuntimeService {
    registry: Registry,
}

impl RuntimeService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }
    #[must_use]
    pub fn runtime_list(&self) -> Vec<RuntimeListEntry> {
        self.registry.runtime_list()
    }
    pub fn runtime_health_with_role(
        &self,
        project: Option<&str>,
        task_id: Option<&str>,
        flow_id: Option<&str>,
        role: RuntimeRole,
    ) -> Result<RuntimeHealthStatus> {
        self.registry
            .runtime_health_with_role(project, task_id, flow_id, role)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn runtime_defaults_set(
        &self,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<()> {
        self.registry.runtime_defaults_set(
            role,
            adapter,
            binary_path,
            model,
            args,
            env,
            timeout_ms,
            max_parallel_tasks,
        )
    }
    pub fn runtime_stream_items_with_detail(
        &self,
        flow_id: Option<&str>,
        attempt_id: Option<&str>,
        limit: usize,
        detail: RuntimeStreamDetailLevel,
    ) -> Result<Vec<RuntimeStreamItemView>> {
        self.registry
            .runtime_stream_items_with_detail(flow_id, attempt_id, limit, detail)
    }
}

pub struct ProjectService {
    registry: Registry,
}

impl ProjectService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn create_project(&self, name: &str, description: Option<&str>) -> Result<Project> {
        self.registry.create_project(name, description)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        self.registry.list_projects()
    }

    pub fn get_project(&self, id_or_name: &str) -> Result<Project> {
        self.registry.get_project(id_or_name)
    }

    pub fn update_project(
        &self,
        id_or_name: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Project> {
        self.registry.update_project(id_or_name, name, description)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn project_runtime_set_role(
        &self,
        id_or_name: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<Project> {
        self.registry.project_runtime_set_role(
            id_or_name,
            role,
            adapter,
            binary_path,
            model,
            args,
            env,
            timeout_ms,
            max_parallel_tasks,
        )
    }

    pub fn attach_repo(
        &self,
        id_or_name: &str,
        path: &str,
        name: Option<&str>,
        access_mode: RepoAccessMode,
    ) -> Result<Project> {
        self.registry
            .attach_repo(id_or_name, path, name, access_mode)
    }

    pub fn detach_repo(&self, id_or_name: &str, repo_name: &str) -> Result<Project> {
        self.registry.detach_repo(id_or_name, repo_name)
    }

    pub fn delete_project(&self, id_or_name: &str) -> Result<Uuid> {
        self.registry.delete_project(id_or_name)
    }
}

pub struct GovernanceService {
    registry: Registry,
}

impl GovernanceService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn constitution_init(
        &self,
        id_or_name: &str,
        content: Option<&str>,
        confirmed: bool,
        actor: Option<&str>,
        intent: Option<&str>,
    ) -> Result<ProjectConstitutionMutationResult> {
        self.registry
            .constitution_init(id_or_name, content, confirmed, actor, intent)
    }

    pub fn constitution_show(&self, id_or_name: &str) -> Result<ProjectConstitutionShowResult> {
        self.registry.constitution_show(id_or_name)
    }

    pub fn constitution_validate(
        &self,
        id_or_name: &str,
        validated_by: Option<&str>,
    ) -> Result<ProjectConstitutionValidationResult> {
        self.registry
            .constitution_validate(id_or_name, validated_by)
    }

    pub fn constitution_check(&self, id_or_name: &str) -> Result<ProjectConstitutionCheckResult> {
        self.registry.constitution_check(id_or_name)
    }

    pub fn constitution_update(
        &self,
        id_or_name: &str,
        content: &str,
        confirmed: bool,
        actor: Option<&str>,
        intent: Option<&str>,
    ) -> Result<ProjectConstitutionMutationResult> {
        self.registry
            .constitution_update(id_or_name, content, confirmed, actor, intent)
    }

    pub fn project_governance_init(&self, id_or_name: &str) -> Result<ProjectGovernanceInitResult> {
        self.registry.project_governance_init(id_or_name)
    }

    pub fn project_governance_migrate(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceMigrateResult> {
        self.registry.project_governance_migrate(id_or_name)
    }

    pub fn project_governance_inspect(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceInspectResult> {
        self.registry.project_governance_inspect(id_or_name)
    }

    pub fn project_governance_diagnose(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceDiagnosticsResult> {
        self.registry.project_governance_diagnose(id_or_name)
    }

    pub fn project_governance_replay(
        &self,
        id_or_name: &str,
        verify: bool,
    ) -> Result<ProjectGovernanceReplayResult> {
        self.registry.project_governance_replay(id_or_name, verify)
    }

    pub fn project_governance_snapshot_create(
        &self,
        id_or_name: &str,
        interval_minutes: Option<u64>,
    ) -> Result<ProjectGovernanceSnapshotCreateResult> {
        self.registry
            .project_governance_snapshot_create(id_or_name, interval_minutes)
    }

    pub fn project_governance_snapshot_list(
        &self,
        id_or_name: &str,
        limit: usize,
    ) -> Result<ProjectGovernanceSnapshotListResult> {
        self.registry
            .project_governance_snapshot_list(id_or_name, limit)
    }

    pub fn project_governance_snapshot_restore(
        &self,
        id_or_name: &str,
        snapshot_id: &str,
        confirm: bool,
    ) -> Result<ProjectGovernanceSnapshotRestoreResult> {
        self.registry
            .project_governance_snapshot_restore(id_or_name, snapshot_id, confirm)
    }

    pub fn project_governance_repair_detect(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceRepairPlanResult> {
        self.registry.project_governance_repair_detect(id_or_name)
    }

    pub fn project_governance_repair_preview(
        &self,
        id_or_name: &str,
        snapshot_id: Option<&str>,
    ) -> Result<ProjectGovernanceRepairPlanResult> {
        self.registry
            .project_governance_repair_preview(id_or_name, snapshot_id)
    }

    pub fn project_governance_repair_apply(
        &self,
        id_or_name: &str,
        snapshot_id: Option<&str>,
        confirm: bool,
    ) -> Result<ProjectGovernanceRepairApplyResult> {
        self.registry
            .project_governance_repair_apply(id_or_name, snapshot_id, confirm)
    }

    pub fn project_governance_document_create(
        &self,
        id_or_name: &str,
        document_id: &str,
        title: &str,
        owner: &str,
        tags: &[String],
        content: &str,
    ) -> Result<ProjectGovernanceDocumentWriteResult> {
        self.registry.project_governance_document_create(
            id_or_name,
            document_id,
            title,
            owner,
            tags,
            content,
        )
    }

    pub fn project_governance_document_list(
        &self,
        id_or_name: &str,
    ) -> Result<Vec<ProjectGovernanceDocumentSummary>> {
        self.registry.project_governance_document_list(id_or_name)
    }

    pub fn project_governance_document_inspect(
        &self,
        id_or_name: &str,
        document_id: &str,
    ) -> Result<ProjectGovernanceDocumentInspectResult> {
        self.registry
            .project_governance_document_inspect(id_or_name, document_id)
    }

    pub fn project_governance_document_update(
        &self,
        id_or_name: &str,
        document_id: &str,
        title: Option<&str>,
        owner: Option<&str>,
        tags: Option<&[String]>,
        content: Option<&str>,
    ) -> Result<ProjectGovernanceDocumentWriteResult> {
        self.registry.project_governance_document_update(
            id_or_name,
            document_id,
            title,
            owner,
            tags,
            content,
        )
    }

    pub fn project_governance_document_delete(
        &self,
        id_or_name: &str,
        document_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        self.registry
            .project_governance_document_delete(id_or_name, document_id)
    }

    pub fn project_governance_attachment_set_document(
        &self,
        id_or_name: &str,
        task_id: &str,
        document_id: &str,
        attached: bool,
    ) -> Result<GovernanceAttachmentUpdateResult> {
        self.registry.project_governance_attachment_set_document(
            id_or_name,
            task_id,
            document_id,
            attached,
        )
    }

    pub fn project_governance_notepad_create(
        &self,
        id_or_name: &str,
        content: &str,
    ) -> Result<GovernanceNotepadResult> {
        self.registry
            .project_governance_notepad_create(id_or_name, content)
    }

    pub fn project_governance_notepad_show(
        &self,
        id_or_name: &str,
    ) -> Result<GovernanceNotepadResult> {
        self.registry.project_governance_notepad_show(id_or_name)
    }

    pub fn project_governance_notepad_update(
        &self,
        id_or_name: &str,
        content: &str,
    ) -> Result<GovernanceNotepadResult> {
        self.registry
            .project_governance_notepad_update(id_or_name, content)
    }

    pub fn project_governance_notepad_delete(
        &self,
        id_or_name: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        self.registry.project_governance_notepad_delete(id_or_name)
    }

    pub fn global_skill_create(
        &self,
        skill_id: &str,
        name: &str,
        tags: &[String],
        content: &str,
    ) -> Result<GlobalSkillSummary> {
        self.registry
            .global_skill_create(skill_id, name, tags, content)
    }

    pub fn global_skill_list(&self) -> Result<Vec<GlobalSkillSummary>> {
        self.registry.global_skill_list()
    }

    pub fn global_skill_inspect(&self, skill_id: &str) -> Result<GlobalSkillInspectResult> {
        self.registry.global_skill_inspect(skill_id)
    }

    pub fn global_skill_update(
        &self,
        skill_id: &str,
        name: Option<&str>,
        tags: Option<&[String]>,
        content: Option<&str>,
    ) -> Result<GlobalSkillSummary> {
        self.registry
            .global_skill_update(skill_id, name, tags, content)
    }

    pub fn global_skill_delete(&self, skill_id: &str) -> Result<GovernanceArtifactDeleteResult> {
        self.registry.global_skill_delete(skill_id)
    }

    pub fn global_system_prompt_create(
        &self,
        prompt_id: &str,
        content: &str,
    ) -> Result<GlobalSystemPromptSummary> {
        self.registry
            .global_system_prompt_create(prompt_id, content)
    }

    pub fn global_system_prompt_list(&self) -> Result<Vec<GlobalSystemPromptSummary>> {
        self.registry.global_system_prompt_list()
    }

    pub fn global_system_prompt_inspect(
        &self,
        prompt_id: &str,
    ) -> Result<GlobalSystemPromptInspectResult> {
        self.registry.global_system_prompt_inspect(prompt_id)
    }

    pub fn global_system_prompt_update(
        &self,
        prompt_id: &str,
        content: &str,
    ) -> Result<GlobalSystemPromptSummary> {
        self.registry
            .global_system_prompt_update(prompt_id, content)
    }

    pub fn global_system_prompt_delete(
        &self,
        prompt_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        self.registry.global_system_prompt_delete(prompt_id)
    }

    pub fn global_template_create(
        &self,
        template_id: &str,
        system_prompt_id: &str,
        skill_ids: &[String],
        document_ids: &[String],
        description: Option<&str>,
    ) -> Result<GlobalTemplateSummary> {
        self.registry.global_template_create(
            template_id,
            system_prompt_id,
            skill_ids,
            document_ids,
            description,
        )
    }

    pub fn global_template_list(&self) -> Result<Vec<GlobalTemplateSummary>> {
        self.registry.global_template_list()
    }

    pub fn global_template_inspect(
        &self,
        template_id: &str,
    ) -> Result<GlobalTemplateInspectResult> {
        self.registry.global_template_inspect(template_id)
    }

    pub fn global_template_update(
        &self,
        template_id: &str,
        system_prompt_id: Option<&str>,
        skill_ids: Option<&[String]>,
        document_ids: Option<&[String]>,
        description: Option<&str>,
    ) -> Result<GlobalTemplateSummary> {
        self.registry.global_template_update(
            template_id,
            system_prompt_id,
            skill_ids,
            document_ids,
            description,
        )
    }

    pub fn global_template_delete(
        &self,
        template_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        self.registry.global_template_delete(template_id)
    }

    pub fn global_template_instantiate(
        &self,
        id_or_name: &str,
        template_id: &str,
    ) -> Result<TemplateInstantiationResult> {
        self.registry
            .global_template_instantiate(id_or_name, template_id)
    }

    pub fn global_notepad_create(&self, content: &str) -> Result<GovernanceNotepadResult> {
        self.registry.global_notepad_create(content)
    }

    pub fn global_notepad_show(&self) -> Result<GovernanceNotepadResult> {
        self.registry.global_notepad_show()
    }

    pub fn global_notepad_update(&self, content: &str) -> Result<GovernanceNotepadResult> {
        self.registry.global_notepad_update(content)
    }

    pub fn global_notepad_delete(&self) -> Result<GovernanceArtifactDeleteResult> {
        self.registry.global_notepad_delete()
    }

    #[must_use]
    pub fn governance_global_root(&self) -> PathBuf {
        self.registry.governance_global_root()
    }
}

pub struct MergeService {
    registry: Registry,
}

impl MergeService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn merge_prepare(&self, flow_id: &str, target_branch: Option<&str>) -> Result<MergeState> {
        self.registry.merge_prepare(flow_id, target_branch)
    }

    pub fn merge_approve(&self, flow_id: &str) -> Result<MergeState> {
        self.registry.merge_approve(flow_id)
    }

    pub fn merge_execute_with_options(
        &self,
        flow_id: &str,
        options: MergeExecuteOptions,
    ) -> Result<MergeState> {
        self.registry.merge_execute_with_options(flow_id, options)
    }
}

pub struct VerificationService {
    registry: Registry,
}

impl VerificationService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn verify_override(&self, task_id: &str, decision: &str, reason: &str) -> Result<TaskFlow> {
        self.registry.verify_override(task_id, decision, reason)
    }

    pub fn verify_run(&self, task_id: &str) -> Result<TaskFlow> {
        self.registry.verify_run(task_id)
    }

    pub fn get_attempt(&self, attempt_id: &str) -> Result<AttemptState> {
        self.registry.get_attempt(attempt_id)
    }
}

pub struct CheckpointService {
    registry: Registry,
}

impl CheckpointService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn list_checkpoints(&self, attempt_id: &str) -> Result<Vec<AttemptCheckpoint>> {
        self.registry.list_checkpoints(attempt_id)
    }

    pub fn checkpoint_complete(
        &self,
        attempt_id: &str,
        checkpoint_id: &str,
        summary: Option<&str>,
    ) -> Result<CheckpointCompletionResult> {
        self.registry
            .checkpoint_complete(attempt_id, checkpoint_id, summary)
    }
}

pub struct TaskService {
    registry: Registry,
}

impl TaskService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn create_task(
        &self,
        project_id_or_name: &str,
        title: &str,
        description: Option<&str>,
        checkpoints_required: bool,
        scope: Option<Scope>,
    ) -> Result<Task> {
        self.registry
            .create_task(
                project_id_or_name,
                title,
                description,
                checkpoints_required,
                scope,
            )
    }

    pub fn list_tasks(
        &self,
        project_id_or_name: &str,
        state_filter: Option<TaskState>,
    ) -> Result<Vec<Task>> {
        self.registry.list_tasks(project_id_or_name, state_filter)
    }

    pub fn get_task(&self, task_id: &str) -> Result<Task> {
        self.registry.get_task(task_id)
    }

    pub fn get_project(&self, id_or_name: &str) -> Result<Project> {
        self.registry.get_project(id_or_name)
    }

    pub fn update_task(
        &self,
        task_id: &str,
        title: Option<&str>,
        description: Option<&str>,
    ) -> Result<Task> {
        self.registry.update_task(task_id, title, description)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn task_runtime_set_role(
        &self,
        task_id: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
    ) -> Result<Task> {
        self.registry.task_runtime_set_role(
            task_id,
            role,
            adapter,
            binary_path,
            model,
            args,
            env,
            timeout_ms,
        )
    }

    pub fn task_runtime_clear_role(&self, task_id: &str, role: RuntimeRole) -> Result<Task> {
        self.registry.task_runtime_clear_role(task_id, role)
    }

    pub fn close_task(&self, task_id: &str, reason: Option<&str>) -> Result<Task> {
        self.registry.close_task(task_id, reason)
    }

    pub fn start_task_execution(&self, task_id: &str) -> Result<Uuid> {
        self.registry.start_task_execution(task_id)
    }

    pub fn complete_task_execution(&self, task_id: &str) -> Result<TaskFlow> {
        self.registry.complete_task_execution(task_id)
    }

    pub fn retry_task(
        &self,
        task_id: &str,
        reset_count: bool,
        mode: RetryMode,
    ) -> Result<TaskFlow> {
        self.registry.retry_task(task_id, reset_count, mode)
    }

    pub fn abort_task(&self, task_id: &str, reason: Option<&str>) -> Result<TaskFlow> {
        self.registry.abort_task(task_id, reason)
    }

    pub fn task_set_run_mode(&self, task_id: &str, mode: RunMode) -> Result<Task> {
        self.registry.task_set_run_mode(task_id, mode)
    }

    pub fn delete_task(&self, task_id: &str) -> Result<Uuid> {
        self.registry.delete_task(task_id)
    }

    pub fn resolve_task_id_with_legacy_project(
        &self,
        project_or_task: &str,
        legacy_task_id: Option<&str>,
        origin: &str,
    ) -> Result<String> {
        if let Some(task_id) = legacy_task_id {
            let project = self.get_project(project_or_task)?;
            let task = self.get_task(task_id)?;
            if task.project_id != project.id {
                return Err(HivemindError::user(
                    "task_project_mismatch",
                    format!("Task '{task_id}' does not belong to project '{project_or_task}'"),
                    origin,
                )
                .with_hint(
                    "Pass the matching project/task pair or use `hivemind task <op> <task-id>`",
                )
                .with_context("project", project_or_task)
                .with_context("task_id", task_id));
            }
            return Ok(task_id.to_string());
        }
        Ok(project_or_task.to_string())
    }
}

pub struct AttemptService {
    registry: Registry,
}

impl AttemptService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn list_attempts(
        &self,
        flow_id: Option<&str>,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AttemptListItem>> {
        self.registry.list_attempts(flow_id, task_id, limit)
    }

    pub fn get_attempt(&self, attempt_id: &str) -> Result<AttemptState> {
        self.registry.get_attempt(attempt_id)
    }

    pub fn get_attempt_diff(&self, attempt_id: &str) -> Result<Option<String>> {
        self.registry.get_attempt_diff(attempt_id)
    }

    pub fn read_events(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        self.registry.read_events(filter)
    }

    pub fn state(&self) -> Result<AppState> {
        self.registry.state()
    }
}

pub struct EventService {
    registry: Registry,
}

impl EventService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn get_project(&self, id_or_name: &str) -> Result<Project> {
        self.registry.get_project(id_or_name)
    }

    pub fn list_events(&self, project_id: Option<Uuid>, limit: usize) -> Result<Vec<Event>> {
        self.registry.list_events(project_id, limit)
    }

    pub fn read_events(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        self.registry.read_events(filter)
    }

    pub fn stream_events(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        self.registry.stream_events(filter)
    }

    pub fn get_event(&self, event_id: &str) -> Result<Event> {
        self.registry.get_event(event_id)
    }

    pub fn replay_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        self.registry.replay_flow(flow_id)
    }

    pub fn get_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        self.registry.get_flow(flow_id)
    }

    pub fn get_workflow_run(&self, workflow_run_id: &str) -> Result<WorkflowRun> {
        self.registry.get_workflow_run(workflow_run_id)
    }

    pub fn events_verify(&self) -> Result<EventsVerifyResult> {
        self.registry.events_verify()
    }
}

pub struct StateService {
    registry: Registry,
}

impl StateService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn state(&self) -> Result<AppState> {
        self.registry.state()
    }
}

pub struct ChatService {
    registry: Registry,
}

impl ChatService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }

    pub fn state(&self) -> Result<AppState> {
        self.registry.state()
    }

    pub fn get_project(&self, id_or_name: &str) -> Result<Project> {
        self.registry.get_project(id_or_name)
    }

    pub fn get_task(&self, task_id: &str) -> Result<Task> {
        self.registry.get_task(task_id)
    }

    pub fn get_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        self.registry.get_flow(flow_id)
    }

    pub fn append_event(&self, event: Event, origin: &'static str) -> Result<()> {
        self.registry.append_event(event, origin)
    }

    #[must_use]
    pub fn project_runtime_for_role_with_source(
        project: &Project,
        role: RuntimeRole,
    ) -> Option<(ProjectRuntimeConfig, RuntimeSelectionSource)> {
        Registry::project_runtime_for_role_with_source(project, role)
    }

    /// # Errors
    /// Returns an error if runtime environment preparation fails.
    pub fn prepare_runtime_environment(
        runtime: &mut ProjectRuntimeConfig,
        origin: &'static str,
    ) -> Result<()> {
        Registry::prepare_runtime_environment(runtime, origin).map(|_| ())
    }
}

pub struct WorktreeService {
    registry: Registry,
}

impl WorktreeService {
    fn new(registry: Registry) -> Self {
        Self { registry }
    }
    pub fn worktree_list(&self, flow_id: &str) -> Result<Vec<WorktreeStatus>> {
        self.registry.worktree_list(flow_id)
    }
    pub fn worktree_inspect(&self, task_id: &str) -> Result<WorktreeStatus> {
        self.registry.worktree_inspect(task_id)
    }
    pub fn worktree_cleanup(
        &self,
        flow_id: &str,
        force: bool,
        dry_run: bool,
    ) -> Result<WorktreeCleanupResult> {
        self.registry.worktree_cleanup(flow_id, force, dry_run)
    }
    pub fn worktree_restore_turn_ref(
        &self,
        attempt_id: &str,
        ordinal: u32,
        confirm: bool,
        force: bool,
    ) -> Result<WorktreeTurnRestoreResult> {
        self.registry
            .worktree_restore_turn_ref(attempt_id, ordinal, confirm, force)
    }
}
