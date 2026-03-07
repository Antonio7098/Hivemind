use super::*;

impl AppState {
    pub(super) fn apply_project_catalog_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::ProjectCreated {
                id,
                name,
                description,
            } => {
                self.projects.insert(
                    *id,
                    Project {
                        id: *id,
                        name: name.clone(),
                        description: description.clone(),
                        created_at: timestamp,
                        updated_at: timestamp,
                        repositories: Vec::new(),
                        runtime: None,
                        runtime_defaults: RuntimeRoleDefaults::default(),
                        constitution_digest: None,
                        constitution_schema_version: None,
                        constitution_version: None,
                        constitution_updated_at: None,
                    },
                );
                true
            }
            EventPayload::ProjectUpdated {
                id,
                name,
                description,
            } => {
                if let Some(project) = self.projects.get_mut(id) {
                    if let Some(n) = name {
                        n.clone_into(&mut project.name);
                    }
                    if let Some(d) = description {
                        project.description = Some(d.clone());
                    }
                    project.updated_at = timestamp;
                }
                true
            }
            EventPayload::ProjectDeleted { project_id } => {
                self.apply_project_deleted(*project_id);
                true
            }
            EventPayload::RepositoryAttached {
                project_id,
                path,
                name,
                access_mode,
            } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    project.repositories.push(Repository {
                        name: name.clone(),
                        path: path.clone(),
                        access_mode: *access_mode,
                    });
                    project.updated_at = timestamp;
                }
                true
            }
            EventPayload::RepositoryDetached { project_id, name } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    project.repositories.retain(|r| r.name != *name);
                    project.updated_at = timestamp;
                }
                true
            }
            _ => false,
        }
    }
}
