use super::*;

/// Filter for querying events.
#[derive(Debug, Default, Clone)]
pub struct EventFilter {
    /// Filter by project ID.
    pub project_id: Option<Uuid>,
    /// Filter by graph ID.
    pub graph_id: Option<Uuid>,
    /// Filter by task ID.
    pub task_id: Option<Uuid>,
    /// Filter by flow ID.
    pub flow_id: Option<Uuid>,
    /// Filter by attempt ID.
    pub attempt_id: Option<Uuid>,
    /// Filter by governance artifact ID/key.
    pub artifact_id: Option<String>,
    /// Filter by template ID.
    pub template_id: Option<String>,
    /// Filter by constitution rule ID.
    pub rule_id: Option<String>,
    /// Filter `error_occurred` events by error category.
    pub error_type: Option<String>,
    /// Include events at or after this timestamp.
    pub since: Option<DateTime<Utc>>,
    /// Include events at or before this timestamp.
    pub until: Option<DateTime<Utc>>,
    /// Maximum number of events to return.
    pub limit: Option<usize>,
}
impl EventFilter {
    /// Creates an empty filter (matches all events).
    #[must_use]
    pub fn all() -> Self {
        Self::default()
    }

    /// Filter by project ID.
    #[must_use]
    pub fn for_project(project_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn for_graph(graph_id: Uuid) -> Self {
        Self {
            graph_id: Some(graph_id),
            ..Default::default()
        }
    }

    /// Checks if an event matches this filter.
    #[must_use]
    pub fn matches(&self, event: &Event) -> bool {
        if let Some(pid) = self.project_id {
            if event.metadata.correlation.project_id != Some(pid) {
                return false;
            }
        }
        if let Some(gid) = self.graph_id {
            if event.metadata.correlation.graph_id != Some(gid) {
                return false;
            }
        }
        if let Some(tid) = self.task_id {
            if event.metadata.correlation.task_id != Some(tid) {
                return false;
            }
        }
        if let Some(fid) = self.flow_id {
            if event.metadata.correlation.flow_id != Some(fid) {
                return false;
            }
        }
        if let Some(aid) = self.attempt_id {
            if event.metadata.correlation.attempt_id != Some(aid) {
                return false;
            }
        }
        if let Some(artifact_id) = self.artifact_id.as_deref() {
            if !Self::matches_artifact_id(&event.payload, artifact_id) {
                return false;
            }
        }
        if let Some(template_id) = self.template_id.as_deref() {
            if !Self::matches_template_id(&event.payload, template_id) {
                return false;
            }
        }
        if let Some(rule_id) = self.rule_id.as_deref() {
            if !Self::matches_rule_id(&event.payload, rule_id) {
                return false;
            }
        }
        if let Some(error_type) = self.error_type.as_deref() {
            if !Self::matches_error_type(&event.payload, error_type) {
                return false;
            }
        }
        if let Some(since) = self.since {
            if event.metadata.timestamp < since {
                return false;
            }
        }
        if let Some(until) = self.until {
            if event.metadata.timestamp > until {
                return false;
            }
        }
        true
    }

    fn matches_artifact_id(payload: &EventPayload, artifact_id: &str) -> bool {
        match payload {
            EventPayload::GovernanceArtifactUpserted { artifact_key, .. }
            | EventPayload::GovernanceArtifactDeleted { artifact_key, .. }
            | EventPayload::GovernanceAttachmentLifecycleUpdated { artifact_key, .. } => {
                artifact_key == artifact_id
            }
            EventPayload::TemplateInstantiated {
                template_id,
                system_prompt_id,
                skill_ids,
                document_ids,
                ..
            } => {
                template_id == artifact_id
                    || system_prompt_id == artifact_id
                    || skill_ids.iter().any(|id| id == artifact_id)
                    || document_ids.iter().any(|id| id == artifact_id)
            }
            EventPayload::AttemptContextOverridesApplied {
                template_document_ids,
                included_document_ids,
                excluded_document_ids,
                resolved_document_ids,
                ..
            } => {
                template_document_ids.iter().any(|id| id == artifact_id)
                    || included_document_ids.iter().any(|id| id == artifact_id)
                    || excluded_document_ids.iter().any(|id| id == artifact_id)
                    || resolved_document_ids.iter().any(|id| id == artifact_id)
            }
            _ => false,
        }
    }

    fn matches_template_id(payload: &EventPayload, template_id: &str) -> bool {
        match payload {
            EventPayload::TemplateInstantiated {
                template_id: id, ..
            } => id == template_id,
            EventPayload::GovernanceArtifactUpserted {
                artifact_kind,
                artifact_key,
                ..
            }
            | EventPayload::GovernanceArtifactDeleted {
                artifact_kind,
                artifact_key,
                ..
            } => artifact_kind == "template" && artifact_key == template_id,
            EventPayload::AttemptContextAssembled { manifest_json, .. } => {
                serde_json::from_str::<serde_json::Value>(manifest_json)
                    .ok()
                    .and_then(|manifest| {
                        manifest
                            .get("template_id")
                            .and_then(serde_json::Value::as_str)
                            .map(std::string::ToString::to_string)
                    })
                    .as_deref()
                    .is_some_and(|id| id == template_id)
            }
            _ => false,
        }
    }

    fn matches_rule_id(payload: &EventPayload, rule_id: &str) -> bool {
        matches!(
            payload,
            EventPayload::ConstitutionViolationDetected { rule_id: id, .. } if id == rule_id
        )
    }

    fn matches_error_type(payload: &EventPayload, error_type: &str) -> bool {
        matches!(
            payload,
            EventPayload::ErrorOccurred { error } if error.category.to_string() == error_type
        )
    }
}
