use super::*;

/// Unique identifier for an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(Uuid);
impl EventId {
    /// Creates a new unique event ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a unique event ID that is ordered by the provided sequence.
    ///
    /// This preserves UUID wire format while allowing stores to guarantee a
    /// monotonic ordering property for event IDs within a log.
    #[must_use]
    pub fn from_ordered_u64(sequence: u64) -> Self {
        let mut bytes = *Uuid::new_v4().as_bytes();
        bytes[..8].copy_from_slice(&sequence.to_be_bytes());
        Self(Uuid::from_bytes(bytes))
    }

    /// Returns the inner UUID.
    #[must_use]
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}
impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
/// Correlation IDs for tracing event relationships.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationIds {
    /// Project this event belongs to.
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub graph_id: Option<Uuid>,
    /// Flow this event belongs to.
    #[serde(default)]
    pub flow_id: Option<Uuid>,
    /// Workflow definition this event belongs to.
    #[serde(default)]
    pub workflow_id: Option<Uuid>,
    /// Workflow run this event belongs to.
    #[serde(default)]
    pub workflow_run_id: Option<Uuid>,
    /// Root workflow run for nested workflow lineage.
    #[serde(default)]
    pub root_workflow_run_id: Option<Uuid>,
    /// Parent workflow run for nested workflow lineage.
    #[serde(default)]
    pub parent_workflow_run_id: Option<Uuid>,
    /// Task this event belongs to.
    #[serde(default)]
    pub task_id: Option<Uuid>,
    /// Workflow step definition this event belongs to.
    #[serde(default)]
    pub step_id: Option<Uuid>,
    /// Workflow step run this event belongs to.
    #[serde(default)]
    pub step_run_id: Option<Uuid>,
    /// Attempt this event belongs to.
    #[serde(default)]
    pub attempt_id: Option<Uuid>,
}
impl CorrelationIds {
    /// Creates empty correlation IDs.
    #[must_use]
    pub fn none() -> Self {
        Self {
            project_id: None,
            graph_id: None,
            flow_id: None,
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: None,
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    /// Creates correlation IDs with only a project ID.
    #[must_use]
    pub fn for_project(project_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: None,
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: None,
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_graph(project_id: Uuid, graph_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: None,
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: None,
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    /// Creates correlation IDs with project and task.
    #[must_use]
    pub fn for_task(project_id: Uuid, task_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: None,
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: Some(task_id),
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_flow(project_id: Uuid, flow_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: Some(flow_id),
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: None,
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_graph_flow(project_id: Uuid, graph_id: Uuid, flow_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: Some(flow_id),
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: None,
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_flow_task(project_id: Uuid, flow_id: Uuid, task_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: Some(flow_id),
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: Some(task_id),
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_graph_flow_task(
        project_id: Uuid,
        graph_id: Uuid,
        flow_id: Uuid,
        task_id: Uuid,
    ) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: Some(flow_id),
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: Some(task_id),
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_graph_flow_task_attempt(
        project_id: Uuid,
        graph_id: Uuid,
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
    ) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: Some(flow_id),
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: Some(task_id),
            step_id: None,
            step_run_id: None,
            attempt_id: Some(attempt_id),
        }
    }

    #[must_use]
    pub fn for_workflow(project_id: Uuid, workflow_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: None,
            workflow_id: Some(workflow_id),
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: None,
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_workflow_run(project_id: Uuid, workflow_id: Uuid, workflow_run_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: None,
            workflow_id: Some(workflow_id),
            workflow_run_id: Some(workflow_run_id),
            root_workflow_run_id: Some(workflow_run_id),
            parent_workflow_run_id: None,
            task_id: None,
            step_id: None,
            step_run_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_workflow_step(
        project_id: Uuid,
        workflow_id: Uuid,
        workflow_run_id: Uuid,
        root_workflow_run_id: Uuid,
        parent_workflow_run_id: Option<Uuid>,
        step_id: Uuid,
        step_run_id: Uuid,
    ) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: None,
            workflow_id: Some(workflow_id),
            workflow_run_id: Some(workflow_run_id),
            root_workflow_run_id: Some(root_workflow_run_id),
            parent_workflow_run_id,
            task_id: None,
            step_id: Some(step_id),
            step_run_id: Some(step_run_id),
            attempt_id: None,
        }
    }
}
