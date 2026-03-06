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