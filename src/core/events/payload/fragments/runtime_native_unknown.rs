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