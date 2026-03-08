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
    WorktreeTurnRefRestored {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        ordinal: u32,
        #[serde(default)]
        git_ref: Option<String>,
        #[serde(default)]
        commit_sha: Option<String>,
        forced: bool,
    },