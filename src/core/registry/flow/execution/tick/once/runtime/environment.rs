use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_tick_runtime_environment(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        worktree_status: &WorktreeStatus,
        repo_worktrees: &[(String, WorktreeStatus)],
        runtime_for_adapter: &mut ProjectRuntimeConfig,
        task_scope: Option<Scope>,
        attempt_id: Uuid,
        origin: &'static str,
    ) -> Result<()> {
        let target_dir = self
            .config
            .data_dir
            .join("cargo-target")
            .join(flow.id.to_string())
            .join(task_id.to_string())
            .join(attempt_id.to_string());
        let _ = fs::create_dir_all(&target_dir);
        runtime_for_adapter
            .env
            .entry("CARGO_TARGET_DIR".to_string())
            .or_insert_with(|| target_dir.to_string_lossy().to_string());
        runtime_for_adapter
            .env
            .insert("HIVEMIND_ATTEMPT_ID".to_string(), attempt_id.to_string());

        if let Some(scope) = task_scope {
            let scope_json = serde_json::to_string(&scope).map_err(|e| {
                HivemindError::system("scope_serialize_failed", e.to_string(), origin)
            })?;
            runtime_for_adapter
                .env
                .insert("HIVEMIND_TASK_SCOPE_JSON".to_string(), scope_json);

            let trace_path = self.scope_trace_path(attempt_id);
            let _ = fs::create_dir_all(self.scope_traces_dir());
            runtime_for_adapter.env.insert(
                "HIVEMIND_SCOPE_TRACE_LOG".to_string(),
                trace_path.to_string_lossy().to_string(),
            );
        }

        runtime_for_adapter
            .env
            .insert("HIVEMIND_TASK_ID".to_string(), task_id.to_string());
        runtime_for_adapter
            .env
            .insert("HIVEMIND_FLOW_ID".to_string(), flow.id.to_string());
        Self::attach_resume_session_if_supported(
            state,
            flow,
            task_id,
            attempt_id,
            runtime_for_adapter,
        );
        runtime_for_adapter.env.insert(
            "HIVEMIND_DATA_DIR".to_string(),
            self.config.data_dir.to_string_lossy().to_string(),
        );

        if let Some(worktree_base_dir) = worktree_status
            .path
            .parent()
            .and_then(|parent| parent.parent())
        {
            runtime_for_adapter.env.insert(
                "HIVEMIND_WORKTREE_DIR".to_string(),
                worktree_base_dir.to_string_lossy().to_string(),
            );
        }
        runtime_for_adapter.env.insert(
            "HIVEMIND_PRIMARY_WORKTREE".to_string(),
            worktree_status.path.to_string_lossy().to_string(),
        );
        runtime_for_adapter.env.insert(
            "HIVEMIND_ALL_WORKTREES".to_string(),
            repo_worktrees
                .iter()
                .map(|(name, wt)| format!("{name}={}", wt.path.display()))
                .collect::<Vec<_>>()
                .join(";"),
        );

        for (repo_name, wt) in repo_worktrees {
            let env_key = format!(
                "HIVEMIND_REPO_{}_WORKTREE",
                repo_name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() {
                        c.to_ascii_uppercase()
                    } else {
                        '_'
                    })
                    .collect::<String>()
            );
            runtime_for_adapter
                .env
                .insert(env_key, wt.path.to_string_lossy().to_string());
        }

        if runtime_for_adapter.adapter_name == "native" {
            runtime_for_adapter
                .env
                .entry("HIVEMIND_NATIVE_STATE_DIR".to_string())
                .or_insert_with(|| {
                    let native_state_dir = self.config.data_dir.join("native-runtime");
                    let _ = fs::create_dir_all(&native_state_dir);
                    native_state_dir.to_string_lossy().to_string()
                });
            let project = state.projects.get(&flow.project_id).ok_or_else(|| {
                HivemindError::system(
                    "project_not_found",
                    "Project missing while preparing native runtime graph query context",
                    origin,
                )
            })?;
            self.set_native_graph_query_runtime_env(project, &mut runtime_for_adapter.env, origin);
        }

        if let Ok(bin) = std::env::current_exe() {
            let hivemind_bin = bin.to_string_lossy().to_string();
            runtime_for_adapter
                .env
                .insert("HIVEMIND_BIN".to_string(), hivemind_bin);

            let agent_path = bin
                .parent()
                .map(|p| p.join("hivemind-agent"))
                .filter(|p| p.exists())
                .unwrap_or(bin);
            runtime_for_adapter.env.insert(
                "HIVEMIND_AGENT_BIN".to_string(),
                agent_path.to_string_lossy().to_string(),
            );
        }

        Ok(())
    }

    fn attach_resume_session_if_supported(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        runtime_for_adapter: &mut ProjectRuntimeConfig,
    ) {
        if !matches!(
            runtime_for_adapter.adapter_name.as_str(),
            "opencode" | "codex" | "kilo"
        ) {
            return;
        }

        let Some(current_attempt) = state.attempts.get(&attempt_id) else {
            return;
        };

        if let Some(session) = current_attempt.runtime_session.as_ref().filter(|session| {
            session.adapter_name == runtime_for_adapter.adapter_name
        }) {
            runtime_for_adapter.env.insert(
                "HIVEMIND_RUNTIME_RESUME_SESSION_ID".to_string(),
                session.session_id.clone(),
            );
            runtime_for_adapter.env.insert(
                "HIVEMIND_RUNTIME_RESUME_PARENT_ATTEMPT_ID".to_string(),
                current_attempt.id.to_string(),
            );
            return;
        }

        let Some(exec) = flow.task_executions.get(&task_id) else {
            return;
        };
        if exec.retry_mode != RetryMode::Continue {
            return;
        }

        let previous = state
            .attempts
            .values()
            .filter(|attempt| {
                attempt.flow_id == flow.id
                    && attempt.task_id == task_id
                    && attempt.attempt_number < current_attempt.attempt_number
            })
            .filter_map(|attempt| {
                attempt.runtime_session.as_ref().and_then(|session| {
                    (session.adapter_name == runtime_for_adapter.adapter_name)
                        .then_some((attempt, session))
                })
            })
            .max_by_key(|(attempt, _)| attempt.attempt_number);

        let Some((previous_attempt, session)) = previous else {
            return;
        };

        runtime_for_adapter.env.insert(
            "HIVEMIND_RUNTIME_RESUME_SESSION_ID".to_string(),
            session.session_id.clone(),
        );
        runtime_for_adapter.env.insert(
            "HIVEMIND_RUNTIME_RESUME_PARENT_ATTEMPT_ID".to_string(),
            previous_attempt.id.to_string(),
        );
    }
}
