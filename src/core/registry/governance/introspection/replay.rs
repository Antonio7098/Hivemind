use super::*;

impl Registry {
    pub fn project_governance_replay(
        &self,
        id_or_name: &str,
        verify: bool,
    ) -> Result<ProjectGovernanceReplayResult> {
        let origin = "registry:project_governance_replay";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let all_events = self
            .store
            .read_all()
            .map_err(|e| HivemindError::system("event_read_failed", e.to_string(), origin))?;
        let replayed_once = AppState::replay(&all_events);
        let replayed_twice = AppState::replay(&all_events);
        let current = self.state()?;

        let projection_once =
            Self::governance_projection_map_for_project(&replayed_once, project.id);
        let projection_twice =
            Self::governance_projection_map_for_project(&replayed_twice, project.id);
        let projection_current = Self::governance_projection_map_for_project(&current, project.id);

        let idempotent = projection_once == projection_twice;
        let current_matches_replay = projection_once == projection_current;
        let missing_on_disk = projection_once
            .values()
            .filter(|artifact| !Path::new(&artifact.path).exists())
            .count();

        if verify && (!idempotent || !current_matches_replay || missing_on_disk > 0) {
            let err = HivemindError::system(
                "governance_replay_verification_failed",
                "Governance replay verification failed",
                origin,
            )
            .with_context("missing_on_disk", missing_on_disk.to_string())
            .with_hint(
                "Run 'hivemind project governance repair detect <project>' to inspect projection drift",
            );
            return Err(err);
        }

        let projections = projection_once
            .values()
            .map(|artifact| GovernanceProjectionEntry {
                project_id: artifact.project_id,
                scope: artifact.scope.clone(),
                artifact_kind: artifact.artifact_kind.clone(),
                artifact_key: artifact.artifact_key.clone(),
                path: artifact.path.clone(),
                revision: artifact.revision,
                exists_on_disk: Path::new(&artifact.path).exists(),
            })
            .collect();

        Ok(ProjectGovernanceReplayResult {
            project_id: project.id,
            replayed_at: Utc::now(),
            projection_count: projection_once.len(),
            idempotent,
            current_matches_replay,
            projections,
        })
    }
}
