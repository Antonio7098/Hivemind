use super::*;

impl TaskFlow {
    #[must_use]
    pub fn new(graph_id: Uuid, project_id: Uuid, task_ids: &[Uuid]) -> Self {
        let now = Utc::now();
        let mut task_executions = HashMap::new();
        for &task_id in task_ids {
            task_executions.insert(task_id, TaskExecution::new(task_id));
        }

        Self {
            id: Uuid::new_v4(),
            graph_id,
            project_id,
            base_revision: None,
            run_mode: RunMode::Manual,
            depends_on_flows: HashSet::new(),
            state: FlowState::Created,
            task_executions,
            created_at: now,
            started_at: None,
            completed_at: None,
            updated_at: now,
        }
    }

    pub fn start(&mut self) -> Result<(), FlowError> {
        if self.state != FlowState::Created {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Running,
            });
        }
        self.state = FlowState::Running;
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), FlowError> {
        if self.state != FlowState::Running {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Paused,
            });
        }
        self.state = FlowState::Paused;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), FlowError> {
        if self.state != FlowState::Paused {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Running,
            });
        }
        self.state = FlowState::Running;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn abort(&mut self) -> Result<(), FlowError> {
        if matches!(
            self.state,
            FlowState::Completed | FlowState::Merged | FlowState::Aborted
        ) {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Aborted,
            });
        }
        self.state = FlowState::Aborted;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn complete(&mut self) -> Result<(), FlowError> {
        if self.state != FlowState::Running {
            return Err(FlowError::InvalidFlowTransition {
                from: self.state,
                to: FlowState::Completed,
            });
        }
        for exec in self.task_executions.values() {
            if !matches!(
                exec.state,
                TaskExecState::Success | TaskExecState::Failed | TaskExecState::Escalated
            ) {
                return Err(FlowError::TasksNotComplete);
            }
        }
        self.state = FlowState::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    #[must_use]
    pub fn get_task_execution(&self, task_id: Uuid) -> Option<&TaskExecution> {
        self.task_executions.get(&task_id)
    }

    pub fn get_task_execution_mut(&mut self, task_id: Uuid) -> Option<&mut TaskExecution> {
        self.task_executions.get_mut(&task_id)
    }

    pub fn transition_task(
        &mut self,
        task_id: Uuid,
        new_state: TaskExecState,
    ) -> Result<(), FlowError> {
        let exec = self
            .task_executions
            .get_mut(&task_id)
            .ok_or(FlowError::TaskNotFound(task_id))?;
        exec.transition(new_state)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            FlowState::Completed | FlowState::Merged | FlowState::Aborted
        )
    }

    #[must_use]
    pub fn tasks_in_state(&self, state: TaskExecState) -> Vec<Uuid> {
        self.task_executions
            .iter()
            .filter(|(_, exec)| exec.state == state)
            .map(|(id, _)| *id)
            .collect()
    }

    #[must_use]
    pub fn task_state_counts(&self) -> HashMap<TaskExecState, usize> {
        let mut counts = HashMap::new();
        for exec in self.task_executions.values() {
            *counts.entry(exec.state).or_insert(0) += 1;
        }
        counts
    }
}
