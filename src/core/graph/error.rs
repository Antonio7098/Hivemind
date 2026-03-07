use super::*;

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GraphLocked => write!(f, "Graph is locked and cannot be modified"),
            Self::TaskNotFound(id) => write!(f, "Task not found: {id}"),
            Self::CycleDetected => write!(f, "Cycle detected in task dependencies"),
            Self::InvalidStateTransition => write!(f, "Invalid state transition"),
            Self::EmptyGraph => write!(f, "Graph must contain at least one task"),
        }
    }
}

impl std::error::Error for GraphError {}
