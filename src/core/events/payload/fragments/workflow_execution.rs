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
    WorkflowStepStateChanged {
        workflow_run_id: Uuid,
        step_id: Uuid,
        step_run_id: Uuid,
        state: WorkflowStepState,
        #[serde(default)]
        reason: Option<String>,
    },