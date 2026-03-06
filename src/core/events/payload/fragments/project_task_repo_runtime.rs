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