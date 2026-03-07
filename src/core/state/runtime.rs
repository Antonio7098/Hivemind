use super::*;

impl RuntimeRoleDefaults {
    pub fn set(&mut self, role: RuntimeRole, config: Option<ProjectRuntimeConfig>) {
        match role {
            RuntimeRole::Worker => self.worker = config,
            RuntimeRole::Validator => self.validator = config,
        }
    }
}

impl TaskRuntimeRoleOverrides {
    pub fn set(&mut self, role: RuntimeRole, config: Option<TaskRuntimeConfig>) {
        match role {
            RuntimeRole::Worker => self.worker = config,
            RuntimeRole::Validator => self.validator = config,
        }
    }
}
