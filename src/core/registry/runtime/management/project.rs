use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub fn project_runtime_set(
        &self,
        id_or_name: &str,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<Project> {
        self.project_runtime_set_role(
            id_or_name,
            RuntimeRole::Worker,
            adapter,
            binary_path,
            model,
            args,
            env,
            timeout_ms,
            max_parallel_tasks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn project_runtime_set_role(
        &self,
        id_or_name: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if max_parallel_tasks == 0 {
            let err = HivemindError::user(
                "invalid_max_parallel_tasks",
                "max_parallel_tasks must be at least 1",
                "registry:project_runtime_set",
            )
            .with_hint("Use --max-parallel-tasks 1 or higher");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        if let Err(err) =
            Self::ensure_supported_runtime_adapter(adapter, "registry:project_runtime_set")
        {
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let env_map = match Self::parse_runtime_env_pairs(env, "registry:project_runtime_set") {
            Ok(parsed) => parsed,
            Err(err) => {
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
        };

        let desired = ProjectRuntimeConfig {
            adapter_name: adapter.to_string(),
            binary_path: binary_path.to_string(),
            model: model.clone(),
            args: args.to_vec(),
            env: env_map.clone(),
            timeout_ms,
            max_parallel_tasks,
        };
        let current = Self::project_runtime_for_role(&project, role);
        if current.as_ref() == Some(&desired) {
            return Ok(project);
        }

        let event = match role {
            RuntimeRole::Worker => Event::new(
                EventPayload::ProjectRuntimeConfigured {
                    project_id: project.id,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::for_project(project.id),
            ),
            RuntimeRole::Validator => Event::new(
                EventPayload::ProjectRuntimeRoleConfigured {
                    project_id: project.id,
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::for_project(project.id),
            ),
        };

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:project_runtime_set",
            )
        })?;

        self.get_project(&project.id.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn runtime_defaults_set(
        &self,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<()> {
        if max_parallel_tasks == 0 {
            return Err(HivemindError::user(
                "invalid_max_parallel_tasks",
                "max_parallel_tasks must be at least 1",
                "registry:runtime_defaults_set",
            )
            .with_hint("Use --max-parallel-tasks 1 or higher"));
        }
        Self::ensure_supported_runtime_adapter(adapter, "registry:runtime_defaults_set")?;
        let env_map = Self::parse_runtime_env_pairs(env, "registry:runtime_defaults_set")?;

        self.append_event(
            Event::new(
                EventPayload::GlobalRuntimeConfigured {
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::none(),
            ),
            "registry:runtime_defaults_set",
        )
    }

    #[must_use]
    pub fn runtime_list(&self) -> Vec<RuntimeListEntry> {
        Self::supported_runtime_descriptors()
            .into_iter()
            .map(|d| RuntimeListEntry {
                adapter_name: d.adapter_name.to_string(),
                default_binary: d.default_binary.to_string(),
                available: if d.requires_binary {
                    Self::binary_available(d.default_binary)
                } else {
                    true
                },
                opencode_compatible: d.opencode_compatible,
                capabilities: d
                    .capabilities
                    .iter()
                    .map(|capability| (*capability).to_string())
                    .collect(),
            })
            .collect()
    }

    pub fn runtime_health(
        &self,
        project: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<RuntimeHealthStatus> {
        self.runtime_health_with_role(project, task_id, None, RuntimeRole::Worker)
    }

    #[allow(clippy::too_many_lines)]
    pub fn runtime_health_with_role(
        &self,
        project: Option<&str>,
        task_id: Option<&str>,
        flow_id: Option<&str>,
        role: RuntimeRole,
    ) -> Result<RuntimeHealthStatus> {
        if let Some(flow_id) = flow_id {
            let flow = self.get_flow(flow_id)?;
            let state = self.state()?;
            let flow_defaults = state
                .flow_runtime_defaults
                .get(&flow.id)
                .cloned()
                .unwrap_or_default();
            let runtime = match role {
                RuntimeRole::Worker => flow_defaults.worker,
                RuntimeRole::Validator => flow_defaults.validator,
            }
            .map(|runtime| (runtime, RuntimeSelectionSource::FlowDefault))
            .or_else(|| {
                state
                    .projects
                    .get(&flow.project_id)
                    .and_then(|project| Self::project_runtime_for_role_with_source(project, role))
            })
            .or_else(|| match role {
                RuntimeRole::Worker => state
                    .global_runtime_defaults
                    .worker
                    .map(|runtime| (runtime, RuntimeSelectionSource::GlobalDefault)),
                RuntimeRole::Validator => state
                    .global_runtime_defaults
                    .validator
                    .map(|runtime| (runtime, RuntimeSelectionSource::GlobalDefault)),
            })
            .ok_or_else(|| {
                HivemindError::new(
                    ErrorCategory::Runtime,
                    "runtime_not_configured",
                    "Flow has no effective runtime configured for this role",
                    "registry:runtime_health",
                )
            })?;
            return Ok(Self::health_for_runtime(
                &runtime.0,
                Some(format!("flow:{flow_id}:{role:?}")),
                Some(runtime.1),
            ));
        }

        if let Some(task_id) = task_id {
            let task_uuid = Uuid::parse_str(task_id).map_err(|_| {
                HivemindError::user(
                    "invalid_task_id",
                    format!("'{task_id}' is not a valid task ID"),
                    "registry:runtime_health",
                )
            })?;
            let state = self.state()?;
            let flow = state
                .flows
                .values()
                .filter(|f| f.task_executions.contains_key(&task_uuid))
                .max_by_key(|f| f.updated_at)
                .cloned()
                .ok_or_else(|| {
                    HivemindError::user(
                        "task_not_in_flow",
                        "Task is not part of any flow",
                        "registry:runtime_health",
                    )
                })?;
            let runtime = Self::effective_runtime_for_task_with_source(
                &state,
                &flow,
                task_uuid,
                role,
                "registry:runtime_health",
            )?;

            return Ok(Self::health_for_runtime(
                &runtime.0,
                Some(format!("task:{task_id}:{role:?}")),
                Some(runtime.1),
            ));
        }

        if let Some(project_id_or_name) = project {
            let project = self.get_project(project_id_or_name)?;
            let runtime =
                Self::project_runtime_for_role_with_source(&project, role).ok_or_else(|| {
                    HivemindError::new(
                        ErrorCategory::Runtime,
                        "runtime_not_configured",
                        "Project has no runtime configured",
                        "registry:runtime_health",
                    )
                })?;
            return Ok(Self::health_for_runtime(
                &runtime.0,
                Some(format!("project:{project_id_or_name}:{role:?}")),
                Some(runtime.1),
            ));
        }

        Ok(RuntimeHealthStatus {
            adapter_name: "all".to_string(),
            binary_path: "builtin-defaults".to_string(),
            healthy: self.runtime_list().iter().all(|r| r.available),
            capabilities: vec!["catalog".to_string()],
            selection_source: None,
            target: None,
            details: Some(
                self.runtime_list()
                    .into_iter()
                    .map(|r| {
                        format!(
                            "{}={} ({})",
                            r.adapter_name,
                            if r.available { "ok" } else { "missing" },
                            r.default_binary
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        })
    }
}
