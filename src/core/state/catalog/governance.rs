use super::*;

impl AppState {
    pub(super) fn apply_governance_catalog_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::GovernanceProjectStorageInitialized {
                project_id,
                schema_version,
                projection_version,
                root_path,
            } => {
                self.governance_projects.insert(
                    *project_id,
                    GovernanceProjectStorage {
                        project_id: *project_id,
                        schema_version: schema_version.clone(),
                        projection_version: *projection_version,
                        root_path: root_path.clone(),
                        initialized_at: timestamp,
                    },
                );
                true
            }
            EventPayload::GovernanceArtifactUpserted {
                project_id,
                scope,
                artifact_kind,
                artifact_key,
                path,
                revision,
                schema_version,
                projection_version,
            } => {
                self.governance_artifacts.insert(
                    governance_artifact_key(*project_id, scope, artifact_kind, artifact_key),
                    GovernanceArtifact {
                        project_id: *project_id,
                        scope: scope.clone(),
                        artifact_kind: artifact_kind.clone(),
                        artifact_key: artifact_key.clone(),
                        path: path.clone(),
                        revision: *revision,
                        schema_version: schema_version.clone(),
                        projection_version: *projection_version,
                        updated_at: timestamp,
                    },
                );
                true
            }
            EventPayload::GovernanceArtifactDeleted {
                project_id,
                scope,
                artifact_kind,
                artifact_key,
                path: _,
                schema_version: _,
                projection_version: _,
            } => {
                self.governance_artifacts.remove(&governance_artifact_key(
                    *project_id,
                    scope,
                    artifact_kind,
                    artifact_key,
                ));
                true
            }
            EventPayload::GovernanceAttachmentLifecycleUpdated {
                project_id,
                task_id,
                artifact_kind,
                artifact_key,
                attached,
                schema_version,
                projection_version,
            } => {
                let key = format!("{project_id}::{task_id}::{artifact_kind}::{artifact_key}");
                self.governance_attachments.insert(
                    key,
                    GovernanceAttachment {
                        project_id: *project_id,
                        task_id: *task_id,
                        artifact_kind: artifact_kind.clone(),
                        artifact_key: artifact_key.clone(),
                        attached: *attached,
                        schema_version: schema_version.clone(),
                        projection_version: *projection_version,
                        updated_at: timestamp,
                    },
                );
                true
            }
            EventPayload::GovernanceStorageMigrated {
                project_id,
                from_layout,
                to_layout,
                migrated_paths,
                rollback_hint,
                schema_version,
                projection_version,
            } => {
                self.governance_migrations.push(GovernanceMigration {
                    project_id: *project_id,
                    from_layout: from_layout.clone(),
                    to_layout: to_layout.clone(),
                    migrated_paths: migrated_paths.clone(),
                    rollback_hint: rollback_hint.clone(),
                    schema_version: schema_version.clone(),
                    projection_version: *projection_version,
                    migrated_at: timestamp,
                });
                true
            }
            EventPayload::ConstitutionInitialized {
                project_id,
                schema_version,
                constitution_version,
                digest,
                ..
            }
            | EventPayload::ConstitutionUpdated {
                project_id,
                schema_version,
                constitution_version,
                digest,
                ..
            } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    project.constitution_digest = Some(digest.clone());
                    project.constitution_schema_version = Some(schema_version.clone());
                    project.constitution_version = Some(*constitution_version);
                    project.constitution_updated_at = Some(timestamp);
                    project.updated_at = timestamp;
                }
                true
            }
            _ => false,
        }
    }
}
