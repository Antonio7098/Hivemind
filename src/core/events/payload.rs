use super::*;

/// Payload types for different events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    /// A failure occurred and was recorded.
    ErrorOccurred {
        error: HivemindError,
    },

    /// A new project was created.
    ProjectCreated {
        id: Uuid,
        name: String,
        description: Option<String>,
    },
    /// A project was updated.
    ProjectUpdated {
        id: Uuid,
        name: Option<String>,
        description: Option<String>,
    },
    /// A project was deleted.
    ProjectDeleted {
        project_id: Uuid,
    },
    ProjectRuntimeConfigured {
        project_id: Uuid,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
        #[serde(default = "default_max_parallel_tasks")]
        max_parallel_tasks: u16,
    },
    ProjectRuntimeRoleConfigured {
        project_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
        #[serde(default = "default_max_parallel_tasks")]
        max_parallel_tasks: u16,
    },
    GlobalRuntimeConfigured {
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
        #[serde(default = "default_max_parallel_tasks")]
        max_parallel_tasks: u16,
    },
    /// A new task was created.
    TaskCreated {
        id: Uuid,
        project_id: Uuid,
        title: String,
        description: Option<String>,
        #[serde(default)]
        scope: Option<Scope>,
    },
    /// A task was updated.
    TaskUpdated {
        id: Uuid,
        title: Option<String>,
        description: Option<String>,
    },
    TaskRuntimeConfigured {
        task_id: Uuid,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
    },
    TaskRuntimeRoleConfigured {
        task_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
    },
    TaskRuntimeCleared {
        task_id: Uuid,
    },
    TaskRuntimeRoleCleared {
        task_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
    },
    TaskRunModeSet {
        task_id: Uuid,
        mode: RunMode,
    },
    /// A task was closed.
    TaskClosed {
        id: Uuid,
        #[serde(default)]
        reason: Option<String>,
    },
    /// A task was deleted.
    TaskDeleted {
        task_id: Uuid,
        project_id: Uuid,
    },
    /// A repository was attached to a project.
    RepositoryAttached {
        project_id: Uuid,
        path: String,
        name: String,
        #[serde(default)]
        access_mode: RepoAccessMode,
    },
    /// A repository was detached from a project.
    RepositoryDetached {
        project_id: Uuid,
        name: String,
    },

    GovernanceProjectStorageInitialized {
        project_id: Uuid,
        schema_version: String,
        projection_version: u32,
        root_path: String,
    },

    GovernanceArtifactUpserted {
        #[serde(default)]
        project_id: Option<Uuid>,
        scope: String,
        artifact_kind: String,
        artifact_key: String,
        path: String,
        #[serde(default)]
        revision: u64,
        schema_version: String,
        projection_version: u32,
    },

    GovernanceArtifactDeleted {
        #[serde(default)]
        project_id: Option<Uuid>,
        scope: String,
        artifact_kind: String,
        artifact_key: String,
        path: String,
        schema_version: String,
        projection_version: u32,
    },

    GovernanceAttachmentLifecycleUpdated {
        project_id: Uuid,
        task_id: Uuid,
        artifact_kind: String,
        artifact_key: String,
        attached: bool,
        schema_version: String,
        projection_version: u32,
    },

    GovernanceStorageMigrated {
        #[serde(default)]
        project_id: Option<Uuid>,
        from_layout: String,
        to_layout: String,
        #[serde(default)]
        migrated_paths: Vec<String>,
        rollback_hint: String,
        schema_version: String,
        projection_version: u32,
    },

    GovernanceSnapshotCreated {
        project_id: Uuid,
        snapshot_id: String,
        path: String,
        artifact_count: usize,
        total_bytes: u64,
        #[serde(default)]
        source_event_sequence: Option<u64>,
    },

    GovernanceSnapshotRestored {
        project_id: Uuid,
        snapshot_id: String,
        path: String,
        artifact_count: usize,
        restored_files: usize,
        skipped_files: usize,
        #[serde(default)]
        stale_files: usize,
        #[serde(default)]
        repaired_projection_count: usize,
    },

    GovernanceDriftDetected {
        project_id: Uuid,
        issue_count: usize,
        recoverable_count: usize,
        unrecoverable_count: usize,
    },

    GovernanceRepairApplied {
        project_id: Uuid,
        operation_count: usize,
        repaired_count: usize,
        remaining_issue_count: usize,
        #[serde(default)]
        snapshot_id: Option<String>,
    },

    GraphSnapshotStarted {
        project_id: Uuid,
        trigger: String,
        repository_count: usize,
    },

    GraphSnapshotCompleted {
        project_id: Uuid,
        trigger: String,
        path: String,
        revision: u64,
        repository_count: usize,
        ucp_engine_version: String,
        profile_version: String,
        canonical_fingerprint: String,
    },

    GraphSnapshotFailed {
        project_id: Uuid,
        trigger: String,
        reason: String,
        #[serde(default)]
        hint: Option<String>,
    },

    GraphSnapshotDiffDetected {
        project_id: Uuid,
        trigger: String,
        #[serde(default)]
        previous_fingerprint: Option<String>,
        canonical_fingerprint: String,
    },

    GraphQueryExecuted {
        project_id: Uuid,
        #[serde(default)]
        flow_id: Option<Uuid>,
        #[serde(default)]
        task_id: Option<Uuid>,
        #[serde(default)]
        attempt_id: Option<Uuid>,
        source: String,
        query_kind: String,
        result_node_count: usize,
        result_edge_count: usize,
        visited_node_count: usize,
        visited_edge_count: usize,
        max_results: usize,
        truncated: bool,
        duration_ms: u64,
        canonical_fingerprint: String,
    },

    ConstitutionInitialized {
        project_id: Uuid,
        path: String,
        schema_version: String,
        constitution_version: u32,
        digest: String,
        #[serde(default)]
        revision: u64,
        actor: String,
        mutation_intent: String,
        confirmed: bool,
    },

    ConstitutionUpdated {
        project_id: Uuid,
        path: String,
        schema_version: String,
        constitution_version: u32,
        previous_digest: String,
        digest: String,
        #[serde(default)]
        revision: u64,
        actor: String,
        mutation_intent: String,
        confirmed: bool,
    },

    ConstitutionValidated {
        project_id: Uuid,
        path: String,
        schema_version: String,
        constitution_version: u32,
        digest: String,
        valid: bool,
        #[serde(default)]
        issues: Vec<String>,
        validated_by: String,
    },

    ConstitutionViolationDetected {
        project_id: Uuid,
        #[serde(default)]
        flow_id: Option<Uuid>,
        #[serde(default)]
        task_id: Option<Uuid>,
        #[serde(default)]
        attempt_id: Option<Uuid>,
        gate: String,
        rule_id: String,
        rule_type: String,
        severity: String,
        message: String,
        #[serde(default)]
        evidence: Vec<String>,
        #[serde(default)]
        remediation_hint: Option<String>,
        blocked: bool,
    },

    TemplateInstantiated {
        project_id: Uuid,
        template_id: String,
        system_prompt_id: String,
        #[serde(default)]
        skill_ids: Vec<String>,
        #[serde(default)]
        document_ids: Vec<String>,
        schema_version: String,
        projection_version: u32,
    },

    AttemptContextOverridesApplied {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        #[serde(default)]
        template_document_ids: Vec<String>,
        #[serde(default)]
        included_document_ids: Vec<String>,
        #[serde(default)]
        excluded_document_ids: Vec<String>,
        #[serde(default)]
        resolved_document_ids: Vec<String>,
    },

    ContextWindowCreated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        window_id: String,
        policy: String,
        state_hash: String,
    },

    ContextOpApplied {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        window_id: String,
        op: String,
        actor: String,
        #[serde(default)]
        runtime: Option<String>,
        #[serde(default)]
        tool: Option<String>,
        reason: String,
        before_hash: String,
        after_hash: String,
        #[serde(default)]
        added_ids: Vec<String>,
        #[serde(default)]
        removed_ids: Vec<String>,
        #[serde(default)]
        truncated_sections: Vec<String>,
        #[serde(default)]
        section_reasons: BTreeMap<String, Vec<String>>,
    },

    ContextWindowSnapshotCreated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        window_id: String,
        state_hash: String,
        rendered_prompt_hash: String,
        delivered_input_hash: String,
        snapshot_json: String,
    },

    AttemptContextAssembled {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        attempt_number: u32,
        manifest_hash: String,
        inputs_hash: String,
        context_hash: String,
        context_size_bytes: usize,
        #[serde(default)]
        truncated_sections: Vec<String>,
        manifest_json: String,
    },

    AttemptContextTruncated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        budget_bytes: usize,
        original_size_bytes: usize,
        truncated_size_bytes: usize,
        #[serde(default)]
        sections: Vec<String>,
        #[serde(default)]
        section_reasons: BTreeMap<String, Vec<String>>,
        policy: String,
    },

    AttemptContextDelivered {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        manifest_hash: String,
        inputs_hash: String,
        context_hash: String,
        delivery_target: String,
        #[serde(default)]
        prior_attempt_ids: Vec<Uuid>,
        #[serde(default)]
        prior_manifest_hashes: Vec<String>,
    },

    TaskGraphCreated {
        graph_id: Uuid,
        project_id: Uuid,
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    TaskAddedToGraph {
        graph_id: Uuid,
        task: GraphTask,
    },
    DependencyAdded {
        graph_id: Uuid,
        from_task: Uuid,
        to_task: Uuid,
    },
    GraphTaskCheckAdded {
        graph_id: Uuid,
        task_id: Uuid,
        check: CheckConfig,
    },
    ScopeAssigned {
        graph_id: Uuid,
        task_id: Uuid,
        scope: Scope,
    },

    TaskGraphValidated {
        graph_id: Uuid,
        project_id: Uuid,
        valid: bool,
        #[serde(default)]
        issues: Vec<String>,
    },

    TaskGraphLocked {
        graph_id: Uuid,
        project_id: Uuid,
    },
    TaskGraphDeleted {
        graph_id: Uuid,
        project_id: Uuid,
    },

    TaskFlowCreated {
        flow_id: Uuid,
        graph_id: Uuid,
        project_id: Uuid,
        #[serde(default)]
        name: Option<String>,
        task_ids: Vec<Uuid>,
    },
    TaskFlowDependencyAdded {
        flow_id: Uuid,
        depends_on_flow_id: Uuid,
    },
    TaskFlowRunModeSet {
        flow_id: Uuid,
        mode: RunMode,
    },
    TaskFlowRuntimeConfigured {
        flow_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
        #[serde(default = "default_max_parallel_tasks")]
        max_parallel_tasks: u16,
    },
    TaskFlowRuntimeCleared {
        flow_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
    },
    TaskFlowStarted {
        flow_id: Uuid,
        #[serde(default)]
        base_revision: Option<String>,
    },
    TaskFlowPaused {
        flow_id: Uuid,
        #[serde(default)]
        running_tasks: Vec<Uuid>,
    },
    TaskFlowResumed {
        flow_id: Uuid,
    },
    TaskFlowCompleted {
        flow_id: Uuid,
    },
    TaskFlowAborted {
        flow_id: Uuid,
        #[serde(default)]
        reason: Option<String>,
        forced: bool,
    },
    TaskFlowDeleted {
        flow_id: Uuid,
        graph_id: Uuid,
        project_id: Uuid,
    },

    TaskReady {
        flow_id: Uuid,
        task_id: Uuid,
    },
    TaskBlocked {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        reason: Option<String>,
    },
    ScopeConflictDetected {
        flow_id: Uuid,
        task_id: Uuid,
        conflicting_task_id: Uuid,
        severity: String,
        action: String,
        reason: String,
    },
    TaskSchedulingDeferred {
        flow_id: Uuid,
        task_id: Uuid,
        reason: String,
    },
    TaskExecutionStateChanged {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        attempt_id: Option<Uuid>,
        from: TaskExecState,
        to: TaskExecState,
    },

    TaskExecutionStarted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        attempt_number: u32,
    },
    TaskExecutionSucceeded {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        attempt_id: Option<Uuid>,
    },
    TaskExecutionFailed {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        attempt_id: Option<Uuid>,
        #[serde(default)]
        reason: Option<String>,
    },

    AttemptStarted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        attempt_number: u32,
    },

    BaselineCaptured {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        baseline_id: Uuid,
        #[serde(default)]
        git_head: Option<String>,
        file_count: usize,
    },

    FileModified {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        path: String,
        change_type: ChangeType,
        #[serde(default)]
        old_hash: Option<String>,
        #[serde(default)]
        new_hash: Option<String>,
    },

    DiffComputed {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        diff_id: Uuid,
        baseline_id: Uuid,
        change_count: usize,
    },

    CheckStarted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        check_name: String,
        required: bool,
    },

    CheckCompleted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        check_name: String,
        passed: bool,
        exit_code: i32,
        output: String,
        duration_ms: u64,
        required: bool,
    },

    MergeCheckStarted {
        flow_id: Uuid,
        #[serde(default)]
        task_id: Option<Uuid>,
        check_name: String,
        required: bool,
    },

    MergeCheckCompleted {
        flow_id: Uuid,
        #[serde(default)]
        task_id: Option<Uuid>,
        check_name: String,
        passed: bool,
        exit_code: i32,
        output: String,
        duration_ms: u64,
        required: bool,
    },

    TaskExecutionFrozen {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        commit_sha: Option<String>,
    },

    TaskIntegratedIntoFlow {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        commit_sha: Option<String>,
    },

    MergeConflictDetected {
        flow_id: Uuid,
        #[serde(default)]
        task_id: Option<Uuid>,
        details: String,
    },

    FlowFrozenForMerge {
        flow_id: Uuid,
    },

    FlowIntegrationLockAcquired {
        flow_id: Uuid,
        operation: String,
    },

    CheckpointDeclared {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        checkpoint_id: String,
        order: u32,
        total: u32,
    },

    CheckpointActivated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        checkpoint_id: String,
        order: u32,
    },

    CheckpointCompleted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        checkpoint_id: String,
        order: u32,
        commit_hash: String,
        timestamp: DateTime<Utc>,
        #[serde(default)]
        summary: Option<String>,
    },

    AllCheckpointsCompleted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
    },

    CheckpointCommitCreated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        commit_sha: String,
    },

    ScopeValidated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        verification_id: Uuid,
        verified_at: DateTime<Utc>,
        scope: Scope,
    },

    ScopeViolationDetected {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        verification_id: Uuid,
        verified_at: DateTime<Utc>,
        scope: Scope,
        #[serde(default)]
        violations: Vec<ScopeViolation>,
    },

    RetryContextAssembled {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        attempt_number: u32,
        max_attempts: u32,
        #[serde(default)]
        prior_attempt_ids: Vec<Uuid>,
        #[serde(default)]
        required_check_failures: Vec<String>,
        #[serde(default)]
        optional_check_failures: Vec<String>,
        #[serde(default)]
        runtime_exit_code: Option<i32>,
        #[serde(default)]
        runtime_terminated_reason: Option<String>,
        context: String,
    },

    #[serde(rename = "task_retried", alias = "task_retry_requested")]
    TaskRetryRequested {
        task_id: Uuid,
        reset_count: bool,
        #[serde(default)]
        retry_mode: RetryMode,
    },
    TaskAborted {
        task_id: Uuid,
        #[serde(default)]
        reason: Option<String>,
    },

    HumanOverride {
        task_id: Uuid,
        override_type: String,
        decision: String,
        reason: String,
        #[serde(default)]
        user: Option<String>,
    },

    MergePrepared {
        flow_id: Uuid,
        #[serde(default)]
        target_branch: Option<String>,
        #[serde(default)]
        conflicts: Vec<String>,
    },
    MergeApproved {
        flow_id: Uuid,
        #[serde(default)]
        user: Option<String>,
    },
    MergeCompleted {
        flow_id: Uuid,
        #[serde(default)]
        commits: Vec<String>,
    },
    WorktreeCleanupPerformed {
        flow_id: Uuid,
        cleaned_worktrees: usize,
        forced: bool,
        dry_run: bool,
    },
    RuntimeStarted {
        adapter_name: String,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        task_id: Uuid,
        attempt_id: Uuid,
        #[serde(default)]
        prompt: String,
        #[serde(default)]
        flags: Vec<String>,
    },
    RuntimeEnvironmentPrepared {
        attempt_id: Uuid,
        adapter_name: String,
        inherit_mode: String,
        #[serde(default)]
        inherited_keys: Vec<String>,
        #[serde(default)]
        overlay_keys: Vec<String>,
        #[serde(default)]
        explicit_sensitive_overlay_keys: Vec<String>,
        #[serde(default)]
        dropped_sensitive_inherited_keys: Vec<String>,
        #[serde(default)]
        dropped_reserved_inherited_keys: Vec<String>,
    },
    RuntimeCapabilitiesEvaluated {
        adapter_name: String,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        selection_source: RuntimeSelectionSource,
        #[serde(default)]
        capabilities: Vec<String>,
    },
    AgentInvocationStarted {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        adapter_name: String,
        provider: String,
        model: String,
        runtime_version: String,
        capture_mode: NativeEventPayloadCaptureMode,
    },
    AgentTurnStarted {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        from_state: String,
    },
    ModelRequestPrepared {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        request: NativeBlobRef,
    },
    ModelResponseReceived {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        response: NativeBlobRef,
    },
    ToolCallRequested {
        native_correlation: NativeEventCorrelation,
        #[serde(default)]
        task_id: Option<Uuid>,
        invocation_id: String,
        turn_index: u32,
        call_id: String,
        tool_name: String,
        request: NativeBlobRef,
        #[serde(default)]
        policy_tags: Vec<String>,
    },
    ToolCallStarted {
        native_correlation: NativeEventCorrelation,
        #[serde(default)]
        task_id: Option<Uuid>,
        invocation_id: String,
        turn_index: u32,
        call_id: String,
        tool_name: String,
        #[serde(default)]
        policy_tags: Vec<String>,
    },
    ToolCallCompleted {
        native_correlation: NativeEventCorrelation,
        #[serde(default)]
        task_id: Option<Uuid>,
        invocation_id: String,
        turn_index: u32,
        call_id: String,
        tool_name: String,
        response: NativeBlobRef,
        #[serde(default)]
        policy_tags: Vec<String>,
    },
    ToolCallFailed {
        native_correlation: NativeEventCorrelation,
        #[serde(default)]
        task_id: Option<Uuid>,
        invocation_id: String,
        turn_index: u32,
        call_id: String,
        tool_name: String,
        code: String,
        message: String,
        recoverable: bool,
        #[serde(default)]
        policy_tags: Vec<String>,
    },
    AgentTurnCompleted {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        to_state: String,
        #[serde(default)]
        summary: Option<String>,
    },
    AgentInvocationCompleted {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        total_turns: u32,
        final_state: String,
        success: bool,
        #[serde(default)]
        final_summary: Option<String>,
        #[serde(default)]
        error_code: Option<String>,
        #[serde(default)]
        error_message: Option<String>,
        #[serde(default)]
        recoverable: Option<bool>,
    },
    RuntimeOutputChunk {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        content: String,
    },
    RuntimeInputProvided {
        attempt_id: Uuid,
        content: String,
    },
    RuntimeInterrupted {
        attempt_id: Uuid,
    },
    RuntimeExited {
        attempt_id: Uuid,
        exit_code: i32,
        duration_ms: u64,
    },
    RuntimeTerminated {
        attempt_id: Uuid,
        reason: String,
    },
    RuntimeErrorClassified {
        attempt_id: Uuid,
        adapter_name: String,
        code: String,
        category: String,
        message: String,
        recoverable: bool,
        retryable: bool,
        rate_limited: bool,
    },
    RuntimeRecoveryScheduled {
        attempt_id: Uuid,
        from_adapter: String,
        to_adapter: String,
        strategy: String,
        reason: String,
        backoff_ms: u64,
    },
    RuntimeFilesystemObserved {
        attempt_id: Uuid,
        #[serde(default)]
        files_created: Vec<PathBuf>,
        #[serde(default)]
        files_modified: Vec<PathBuf>,
        #[serde(default)]
        files_deleted: Vec<PathBuf>,
    },
    RuntimeCommandObserved {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        command: String,
    },
    RuntimeToolCallObserved {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        tool_name: String,
        details: String,
    },
    RuntimeTodoSnapshotUpdated {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        #[serde(default)]
        items: Vec<String>,
    },
    RuntimeNarrativeOutputObserved {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        content: String,
    },

    #[serde(other)]
    Unknown,
}
