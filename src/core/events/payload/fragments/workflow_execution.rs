    WorkflowDefinitionCreated {
        definition: WorkflowDefinition,
    },
    WorkflowDefinitionUpdated {
        definition: WorkflowDefinition,
    },
    WorkflowRunCreated {
        run: WorkflowRun,
    },
    WorkflowRunStarted {
        workflow_run_id: Uuid,
    },
    WorkflowRunPaused {
        workflow_run_id: Uuid,
    },
    WorkflowRunResumed {
        workflow_run_id: Uuid,
    },
    WorkflowRunCompleted {
        workflow_run_id: Uuid,
    },
    WorkflowRunAborted {
        workflow_run_id: Uuid,
        #[serde(default)]
        reason: Option<String>,
        forced: bool,
    },
    WorkflowContextInitialized {
        workflow_run_id: Uuid,
        context: WorkflowContextState,
    },
    WorkflowContextSnapshotCaptured {
        workflow_run_id: Uuid,
        snapshot: WorkflowContextSnapshot,
    },
    WorkflowStepInputsResolved {
        workflow_run_id: Uuid,
        step_id: Uuid,
        step_run_id: Uuid,
        snapshot: WorkflowStepContextSnapshot,
    },
    WorkflowOutputAppended {
        workflow_run_id: Uuid,
        step_id: Uuid,
        step_run_id: Uuid,
        entry: WorkflowOutputBagEntry,
    },
    WorkflowStepStateChanged {
        workflow_run_id: Uuid,
        step_id: Uuid,
        step_run_id: Uuid,
        state: WorkflowStepState,
        #[serde(default)]
        reason: Option<String>,
    },
