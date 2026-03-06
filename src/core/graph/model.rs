use super::*;

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            escalate_on_failure: true,
        }
    }
}

impl SuccessCriteria {
    /// Creates new success criteria.
    #[must_use]
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            checks: Vec::new(),
        }
    }

    /// Adds an automated check.
    #[must_use]
    pub fn with_check(mut self, check: impl Into<CheckConfig>) -> Self {
        self.checks.push(check.into());
        self
    }
}

impl GraphTask {
    pub(crate) fn default_checkpoints() -> Vec<String> {
        vec!["checkpoint-1".to_string()]
    }

    /// Creates a new graph task.
    #[must_use]
    pub fn new(title: impl Into<String>, criteria: SuccessCriteria) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: None,
            criteria,
            retry_policy: RetryPolicy::default(),
            checkpoints: Self::default_checkpoints(),
            scope: None,
        }
    }

    /// Sets the task description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Sets the retry policy.
    #[must_use]
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Sets ordered checkpoints for this task.
    #[must_use]
    pub fn with_checkpoints(mut self, checkpoints: Vec<String>) -> Self {
        self.checkpoints = if checkpoints.is_empty() {
            Self::default_checkpoints()
        } else {
            checkpoints
        };
        self
    }

    /// Sets the scope.
    #[must_use]
    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = Some(scope);
        self
    }
}

impl TaskGraph {
    /// Creates a new draft `TaskGraph`.
    #[must_use]
    pub fn new(project_id: Uuid, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            description: None,
            state: GraphState::Draft,
            tasks: HashMap::new(),
            dependencies: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Checks if the graph is modifiable.
    #[must_use]
    pub fn is_modifiable(&self) -> bool {
        self.state == GraphState::Draft
    }
}
