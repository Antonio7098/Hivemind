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