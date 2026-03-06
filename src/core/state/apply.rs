use super::*;

impl AppState {
    /// Applies an event to the state in place.
    #[allow(clippy::too_many_lines)]
    pub fn apply_mut(&mut self, event: &Event) {
        let timestamp = event.timestamp();

        if self.apply_catalog_event(&event.payload, timestamp) {
            return;
        }

        match &event.payload {
            EventPayload::TaskExecutionFrozen {
                flow_id,
                task_id,
                commit_sha,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.frozen_commit_sha.clone_from(commit_sha);
                        exec.updated_at = timestamp;
                        flow.updated_at = timestamp;
                    }
                }
            }
            EventPayload::TaskIntegratedIntoFlow {
                flow_id,
                task_id,
                commit_sha,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.integrated_commit_sha.clone_from(commit_sha);
                        exec.updated_at = timestamp;
                        flow.updated_at = timestamp;
                    }
                }
            }
            EventPayload::TaskGraphCreated {
                graph_id,
                project_id,
                name,
                description,
            } => {
                self.graphs.insert(
                    *graph_id,
                    TaskGraph {
                        id: *graph_id,
                        project_id: *project_id,
                        name: name.clone(),
                        description: description.clone(),
                        state: GraphState::Draft,
                        tasks: HashMap::new(),
                        dependencies: HashMap::<Uuid, HashSet<Uuid>>::new(),
                        created_at: timestamp,
                        updated_at: timestamp,
                    },
                );
            }
            EventPayload::TaskAddedToGraph { graph_id, task } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.tasks.insert(task.id, task.clone());
                    graph.dependencies.entry(task.id).or_default();
                    graph.updated_at = timestamp;
                }
            }
            EventPayload::DependencyAdded {
                graph_id,
                from_task,
                to_task,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph
                        .dependencies
                        .entry(*to_task)
                        .or_default()
                        .insert(*from_task);
                    graph.updated_at = timestamp;
                }
            }
            EventPayload::GraphTaskCheckAdded {
                graph_id,
                task_id,
                check,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    if let Some(task) = graph.tasks.get_mut(task_id) {
                        task.criteria.checks.push(check.clone());
                        graph.updated_at = timestamp;
                    }
                }
            }
            EventPayload::ScopeAssigned {
                graph_id,
                task_id,
                scope,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    if let Some(task) = graph.tasks.get_mut(task_id) {
                        task.scope = Some(scope.clone());
                        graph.updated_at = timestamp;
                    }
                }
            }
            EventPayload::TaskGraphValidated {
                graph_id,
                project_id: _,
                valid,
                issues: _,
            } => {
                if *valid {
                    if let Some(graph) = self.graphs.get_mut(graph_id) {
                        graph.state = GraphState::Validated;
                        graph.updated_at = timestamp;
                    }
                }
            }
            EventPayload::TaskGraphLocked {
                graph_id,
                project_id: _,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.state = GraphState::Locked;
                    graph.updated_at = timestamp;
                }
            }
            EventPayload::TaskGraphDeleted {
                graph_id,
                project_id: _,
            } => {
                self.graphs.remove(graph_id);
            }
            EventPayload::TaskFlowCreated {
                flow_id,
                graph_id,
                project_id,
                name: _,
                task_ids,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.state = GraphState::Locked;
                    graph.updated_at = timestamp;
                }

                let mut task_executions = HashMap::new();
                for task_id in task_ids {
                    task_executions.insert(
                        *task_id,
                        TaskExecution {
                            task_id: *task_id,
                            state: TaskExecState::Pending,
                            attempt_count: 0,
                            retry_mode: RetryMode::default(),
                            frozen_commit_sha: None,
                            integrated_commit_sha: None,
                            updated_at: timestamp,
                            blocked_reason: None,
                        },
                    );
                }

                self.flows.insert(
                    *flow_id,
                    TaskFlow {
                        id: *flow_id,
                        graph_id: *graph_id,
                        project_id: *project_id,
                        base_revision: None,
                        run_mode: RunMode::Manual,
                        depends_on_flows: HashSet::new(),
                        state: FlowState::Created,
                        task_executions,
                        created_at: timestamp,
                        started_at: None,
                        completed_at: None,
                        updated_at: timestamp,
                    },
                );
                self.flow_runtime_defaults.entry(*flow_id).or_default();
            }
            EventPayload::TaskFlowDependencyAdded {
                flow_id,
                depends_on_flow_id,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.depends_on_flows.insert(*depends_on_flow_id);
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowRunModeSet { flow_id, mode } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.run_mode = *mode;
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowRuntimeConfigured {
                flow_id,
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                let configured = ProjectRuntimeConfig {
                    adapter_name: adapter_name.clone(),
                    binary_path: binary_path.clone(),
                    model: model.clone(),
                    args: args.clone(),
                    env: env.clone(),
                    timeout_ms: *timeout_ms,
                    max_parallel_tasks: *max_parallel_tasks,
                };
                self.flow_runtime_defaults
                    .entry(*flow_id)
                    .or_default()
                    .set(*role, Some(configured));
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowRuntimeCleared { flow_id, role } => {
                self.flow_runtime_defaults
                    .entry(*flow_id)
                    .or_default()
                    .set(*role, None);
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowStarted {
                flow_id,
                base_revision,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Running;
                    flow.started_at = Some(timestamp);
                    flow.base_revision.clone_from(base_revision);
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowPaused {
                flow_id,
                running_tasks: _,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Paused;
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowResumed { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Running;
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowCompleted { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Completed;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::FlowFrozenForMerge { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::FrozenForMerge;
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowAborted {
                flow_id,
                reason: _,
                forced: _,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Aborted;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskFlowDeleted {
                flow_id,
                graph_id: _,
                project_id: _,
            } => {
                self.flows.remove(flow_id);
                self.flow_runtime_defaults.remove(flow_id);
                self.merge_states.remove(flow_id);
                self.attempts
                    .retain(|_, attempt| attempt.flow_id != *flow_id);
            }
            EventPayload::TaskReady { flow_id, task_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = TaskExecState::Ready;
                        exec.blocked_reason = None;
                        exec.updated_at = timestamp;
                    }
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskBlocked {
                flow_id,
                task_id,
                reason,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = TaskExecState::Pending;
                        exec.blocked_reason.clone_from(reason);
                        exec.updated_at = timestamp;
                    }
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::TaskExecutionStateChanged {
                flow_id,
                task_id,
                attempt_id: _,
                from: _,
                to,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = *to;
                        exec.updated_at = timestamp;
                        exec.blocked_reason = None;
                        if *to == TaskExecState::Running {
                            exec.attempt_count += 1;
                        }
                    }
                    flow.updated_at = timestamp;
                }
            }
            EventPayload::AttemptStarted {
                flow_id,
                task_id,
                attempt_id,
                attempt_number,
            } => {
                self.attempts.insert(
                    *attempt_id,
                    AttemptState {
                        id: *attempt_id,
                        flow_id: *flow_id,
                        task_id: *task_id,
                        attempt_number: *attempt_number,
                        started_at: timestamp,
                        baseline_id: None,
                        diff_id: None,
                        check_results: Vec::new(),
                        checkpoints: Vec::new(),
                        all_checkpoints_completed: false,
                    },
                );
            }
            EventPayload::CheckpointDeclared {
                attempt_id,
                checkpoint_id,
                order,
                total,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    let exists = attempt
                        .checkpoints
                        .iter()
                        .any(|cp| cp.checkpoint_id == *checkpoint_id);
                    if !exists {
                        attempt.checkpoints.push(AttemptCheckpoint {
                            checkpoint_id: checkpoint_id.clone(),
                            order: *order,
                            total: *total,
                            state: AttemptCheckpointState::Declared,
                            commit_hash: None,
                            completed_at: None,
                            summary: None,
                        });
                        attempt.checkpoints.sort_by_key(|cp| cp.order);
                    }
                }
            }
            EventPayload::CheckpointActivated {
                attempt_id,
                checkpoint_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    for cp in &mut attempt.checkpoints {
                        if cp.checkpoint_id == *checkpoint_id {
                            cp.state = AttemptCheckpointState::Active;
                        } else if cp.state != AttemptCheckpointState::Completed {
                            cp.state = AttemptCheckpointState::Declared;
                        }
                    }
                }
            }
            EventPayload::CheckpointCompleted {
                attempt_id,
                checkpoint_id,
                commit_hash,
                timestamp,
                summary,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    if let Some(cp) = attempt
                        .checkpoints
                        .iter_mut()
                        .find(|cp| cp.checkpoint_id == *checkpoint_id)
                    {
                        cp.state = AttemptCheckpointState::Completed;
                        cp.commit_hash = Some(commit_hash.clone());
                        cp.completed_at = Some(*timestamp);
                        cp.summary.clone_from(summary);
                    }
                }
            }
            EventPayload::AllCheckpointsCompleted { attempt_id, .. } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.all_checkpoints_completed = true;
                }
            }
            EventPayload::BaselineCaptured {
                attempt_id,
                baseline_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.baseline_id = Some(*baseline_id);
                }
            }
            EventPayload::DiffComputed {
                attempt_id,
                diff_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.diff_id = Some(*diff_id);
                }
            }
            EventPayload::CheckCompleted {
                attempt_id,
                check_name,
                passed,
                exit_code,
                output,
                duration_ms,
                required,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.check_results.push(CheckResult {
                        name: check_name.clone(),
                        passed: *passed,
                        exit_code: *exit_code,
                        output: output.clone(),
                        duration_ms: *duration_ms,
                        required: *required,
                    });
                }
            }
            EventPayload::CheckStarted { .. }
            | EventPayload::GovernanceSnapshotCreated { .. }
            | EventPayload::GovernanceSnapshotRestored { .. }
            | EventPayload::GovernanceDriftDetected { .. }
            | EventPayload::GovernanceRepairApplied { .. }
            | EventPayload::GraphSnapshotStarted { .. }
            | EventPayload::GraphSnapshotCompleted { .. }
            | EventPayload::GraphSnapshotFailed { .. }
            | EventPayload::GraphSnapshotDiffDetected { .. }
            | EventPayload::GraphQueryExecuted { .. }
            | EventPayload::ConstitutionValidated { .. }
            | EventPayload::ConstitutionViolationDetected { .. }
            | EventPayload::TemplateInstantiated { .. }
            | EventPayload::AttemptContextOverridesApplied { .. }
            | EventPayload::ContextWindowCreated { .. }
            | EventPayload::ContextOpApplied { .. }
            | EventPayload::ContextWindowSnapshotCreated { .. }
            | EventPayload::AttemptContextAssembled { .. }
            | EventPayload::AttemptContextTruncated { .. }
            | EventPayload::AttemptContextDelivered { .. }
            | EventPayload::ErrorOccurred { .. }
            | EventPayload::TaskExecutionStarted { .. }
            | EventPayload::TaskExecutionSucceeded { .. }
            | EventPayload::TaskExecutionFailed { .. }
            | EventPayload::MergeConflictDetected { .. }
            | EventPayload::MergeCheckStarted { .. }
            | EventPayload::MergeCheckCompleted { .. }
            | EventPayload::RuntimeStarted { .. }
            | EventPayload::RuntimeEnvironmentPrepared { .. }
            | EventPayload::RuntimeCapabilitiesEvaluated { .. }
            | EventPayload::AgentInvocationStarted { .. }
            | EventPayload::AgentTurnStarted { .. }
            | EventPayload::ModelRequestPrepared { .. }
            | EventPayload::ModelResponseReceived { .. }
            | EventPayload::ToolCallRequested { .. }
            | EventPayload::ToolCallStarted { .. }
            | EventPayload::ToolCallCompleted { .. }
            | EventPayload::ToolCallFailed { .. }
            | EventPayload::AgentTurnCompleted { .. }
            | EventPayload::AgentInvocationCompleted { .. }
            | EventPayload::RuntimeOutputChunk { .. }
            | EventPayload::RuntimeInputProvided { .. }
            | EventPayload::RuntimeInterrupted { .. }
            | EventPayload::RuntimeExited { .. }
            | EventPayload::RuntimeTerminated { .. }
            | EventPayload::RuntimeErrorClassified { .. }
            | EventPayload::RuntimeRecoveryScheduled { .. }
            | EventPayload::RuntimeFilesystemObserved { .. }
            | EventPayload::RuntimeCommandObserved { .. }
            | EventPayload::RuntimeToolCallObserved { .. }
            | EventPayload::RuntimeTodoSnapshotUpdated { .. }
            | EventPayload::RuntimeNarrativeOutputObserved { .. }
            | EventPayload::FileModified { .. }
            | EventPayload::CheckpointCommitCreated { .. }
            | EventPayload::ScopeValidated { .. }
            | EventPayload::ScopeViolationDetected { .. }
            | EventPayload::ScopeConflictDetected { .. }
            | EventPayload::TaskSchedulingDeferred { .. }
            | EventPayload::RetryContextAssembled { .. }
            | EventPayload::FlowIntegrationLockAcquired { .. }
            | EventPayload::WorktreeCleanupPerformed { .. }
            | EventPayload::Unknown => {}
            EventPayload::TaskRetryRequested {
                task_id,
                reset_count,
                retry_mode,
            } => {
                for fid in
                    self.candidate_flow_ids_for_task(event.metadata.correlation.flow_id, task_id)
                {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = TaskExecState::Pending;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            exec.retry_mode = *retry_mode;
                            if *reset_count {
                                exec.attempt_count = 0;
                            }
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
            }
            EventPayload::TaskAborted { task_id, reason: _ } => {
                for fid in
                    self.candidate_flow_ids_for_task(event.metadata.correlation.flow_id, task_id)
                {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = TaskExecState::Failed;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
            }
            EventPayload::HumanOverride {
                task_id,
                override_type: _,
                decision,
                reason: _,
                user: _,
            } => {
                let new_state = if decision == "pass" {
                    TaskExecState::Success
                } else {
                    TaskExecState::Failed
                };

                for fid in
                    self.candidate_flow_ids_for_task(event.metadata.correlation.flow_id, task_id)
                {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = new_state;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
            }
            EventPayload::MergePrepared {
                flow_id,
                target_branch,
                conflicts,
            } => {
                self.merge_states.insert(
                    *flow_id,
                    MergeState {
                        flow_id: *flow_id,
                        status: MergeStatus::Prepared,
                        target_branch: target_branch.clone(),
                        conflicts: conflicts.clone(),
                        commits: Vec::new(),
                        updated_at: timestamp,
                    },
                );
            }
            EventPayload::MergeApproved { flow_id, user: _ } => {
                if let Some(ms) = self.merge_states.get_mut(flow_id) {
                    ms.status = MergeStatus::Approved;
                    ms.updated_at = timestamp;
                }
            }
            EventPayload::MergeCompleted { flow_id, commits } => {
                if let Some(ms) = self.merge_states.get_mut(flow_id) {
                    ms.status = MergeStatus::Completed;
                    commits.clone_into(&mut ms.commits);
                    ms.updated_at = timestamp;
                }

                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Merged;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
            }
            _ => {}
        }
    }

    /// Replays a sequence of events to produce state.
    /// Deterministic: same events → same state.
    #[must_use]
    pub fn replay(events: &[Event]) -> Self {
        let mut state = Self::new();
        for event in events {
            state.apply_mut(event);
        }
        state
    }

    fn candidate_flow_ids_for_task(&self, flow_id: Option<Uuid>, task_id: &Uuid) -> Vec<Uuid> {
        let mut candidate_flow_ids = Vec::new();

        if let Some(fid) = flow_id {
            candidate_flow_ids.push(fid);
        } else {
            for (fid, flow) in &self.flows {
                if flow.task_executions.contains_key(task_id) {
                    candidate_flow_ids.push(*fid);
                }
            }
        }

        candidate_flow_ids
    }
}
