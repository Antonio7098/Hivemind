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
        #[serde(default)]
        agent_mode: Option<String>,
        #[serde(default)]
        allowed_tools: Vec<String>,
        #[serde(default)]
        allowed_capabilities: Vec<String>,
        #[serde(default)]
        configured_max_turns: Option<u32>,
        #[serde(default)]
        configured_timeout_budget_ms: Option<u64>,
        #[serde(default)]
        configured_token_budget: Option<usize>,
        #[serde(default)]
        configured_prompt_headroom: Option<usize>,
    },
    AgentTurnStarted {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        from_state: String,
        #[serde(default)]
        agent_mode: Option<String>,
        #[serde(default)]
        prompt_hash: Option<String>,
        #[serde(default)]
        context_manifest_hash: Option<String>,
        #[serde(default)]
        delivered_context_hash: Option<String>,
        #[serde(default)]
        mode_contract_hash: Option<String>,
        #[serde(default)]
        inputs_hash: Option<String>,
    },
    ModelRequestPrepared {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        request: NativeBlobRef,
        #[serde(default)]
        prompt_headroom: Option<usize>,
        #[serde(default)]
        visible_item_count: usize,
        #[serde(default)]
        available_budget: usize,
        #[serde(default)]
        rendered_prompt_bytes: usize,
        #[serde(default)]
        runtime_context_bytes: usize,
        #[serde(default)]
        static_prompt_chars: usize,
        #[serde(default)]
        selected_history_count: usize,
        #[serde(default)]
        selected_history_chars: usize,
        #[serde(default)]
        code_navigation_count: usize,
        #[serde(default)]
        code_navigation_chars: usize,
        #[serde(default)]
        compacted_summary_count: usize,
        #[serde(default)]
        compacted_summary_chars: usize,
        #[serde(default)]
        tool_contract_count: usize,
        #[serde(default)]
        tool_contract_chars: usize,
        #[serde(default)]
        assembly_duration_ms: u64,
        #[serde(default)]
        skipped_item_count: usize,
        #[serde(default)]
        truncated_item_count: usize,
        #[serde(default)]
        tool_result_items_visible: usize,
        #[serde(default)]
        latest_tool_result_turn_index: Option<u32>,
        #[serde(default)]
        latest_tool_names_visible: Vec<String>,
        #[serde(default)]
        delivery_target: Option<String>,
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
        duration_ms: Option<u64>,
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
        duration_ms: Option<u64>,
        #[serde(default)]
        policy_source: Option<String>,
        #[serde(default)]
        denial_reason: Option<String>,
        #[serde(default)]
        recovery_hint: Option<String>,
        #[serde(default)]
        policy_tags: Vec<String>,
    },
    AgentTurnCompleted {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        to_state: String,
        #[serde(default)]
        tool_result_count: usize,
        #[serde(default)]
        model_latency_ms: u64,
        #[serde(default)]
        tool_latency_ms: u64,
        #[serde(default)]
        turn_duration_ms: u64,
        #[serde(default)]
        elapsed_since_invocation_ms: u64,
        #[serde(default)]
        request_tokens: usize,
        #[serde(default)]
        response_tokens: usize,
        #[serde(default)]
        budget_used_before: usize,
        #[serde(default)]
        budget_used_after: usize,
        #[serde(default)]
        budget_remaining: usize,
        #[serde(default)]
        summary: Option<String>,
    },
    NativeBudgetThresholdReached {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        threshold_percent: u8,
        used_budget: usize,
        total_budget: usize,
        remaining_budget: usize,
    },
    NativeHistoryCompactionRecorded {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        reason: String,
        #[serde(default)]
        rendered_prompt_bytes_before: usize,
        #[serde(default)]
        selected_history_count_before: usize,
        #[serde(default)]
        selected_history_chars_before: usize,
        #[serde(default)]
        visible_items_before: usize,
        #[serde(default)]
        visible_items_after: usize,
        #[serde(default)]
        prompt_tokens_before: usize,
        #[serde(default)]
        projected_budget_used: usize,
        #[serde(default)]
        token_budget: usize,
        #[serde(default)]
        elapsed_since_invocation_ms: u64,
    },
    NativeTurnSummaryRecorded {
        native_correlation: NativeEventCorrelation,
        invocation_id: String,
        turn_index: u32,
        #[serde(default)]
        agent_mode: Option<String>,
        from_state: String,
        to_state: String,
        #[serde(default)]
        prompt_hash: Option<String>,
        #[serde(default)]
        context_manifest_hash: Option<String>,
        #[serde(default)]
        delivered_context_hash: Option<String>,
        #[serde(default)]
        mode_contract_hash: Option<String>,
        #[serde(default)]
        inputs_hash: Option<String>,
        #[serde(default)]
        prompt_headroom: Option<usize>,
        #[serde(default)]
        available_budget: usize,
        #[serde(default)]
        rendered_prompt_bytes: usize,
        #[serde(default)]
        runtime_context_bytes: usize,
        #[serde(default)]
        static_prompt_chars: usize,
        #[serde(default)]
        selected_history_chars: usize,
        #[serde(default)]
        compacted_summary_chars: usize,
        #[serde(default)]
        code_navigation_chars: usize,
        #[serde(default)]
        tool_contract_chars: usize,
        #[serde(default)]
        assembly_duration_ms: u64,
        #[serde(default)]
        visible_item_count: usize,
        #[serde(default)]
        selected_history_count: usize,
        #[serde(default)]
        code_navigation_count: usize,
        #[serde(default)]
        compacted_summary_count: usize,
        #[serde(default)]
        tool_contract_count: usize,
        #[serde(default)]
        skipped_item_count: usize,
        #[serde(default)]
        truncated_item_count: usize,
        #[serde(default)]
        tool_result_items_visible: usize,
        #[serde(default)]
        latest_tool_result_turn_index: Option<u32>,
        #[serde(default)]
        latest_tool_names_visible: Vec<String>,
        #[serde(default)]
        tool_call_count: usize,
        #[serde(default)]
        tool_failure_count: usize,
        #[serde(default)]
        model_latency_ms: u64,
        #[serde(default)]
        tool_latency_ms: u64,
        #[serde(default)]
        turn_duration_ms: u64,
        #[serde(default)]
        elapsed_since_invocation_ms: u64,
        #[serde(default)]
        request_tokens: usize,
        #[serde(default)]
        response_tokens: usize,
        #[serde(default)]
        budget_used_before: usize,
        #[serde(default)]
        budget_used_after: usize,
        #[serde(default)]
        budget_remaining: usize,
        #[serde(default)]
        budget_thresholds_crossed: Vec<u8>,
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