use super::*;

impl Registry {
    pub(crate) fn auto_progress_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        const MAX_AUTO_ITERATIONS: usize = 1024;

        for _ in 0..MAX_AUTO_ITERATIONS {
            let latest = self.get_flow(flow_id)?;
            if latest.state != FlowState::Running {
                return Ok(latest);
            }

            let state = self.state()?;
            let graph = state.graphs.get(&latest.graph_id).ok_or_else(|| {
                HivemindError::system(
                    "graph_not_found",
                    "Graph not found",
                    "registry:auto_progress_flow",
                )
            })?;

            let has_verifying = !latest.tasks_in_state(TaskExecState::Verifying).is_empty();
            let has_auto_runnable = latest
                .tasks_in_state(TaskExecState::Retry)
                .into_iter()
                .chain(latest.tasks_in_state(TaskExecState::Ready))
                .any(|task_id| {
                    graph.tasks.contains_key(&task_id) && Self::can_auto_run_task(&state, task_id)
                });

            let before_state = latest.state;
            let before_counts = latest.task_state_counts();
            let next = self.tick_flow(flow_id, false, None)?;
            let after_counts = next.task_state_counts();
            if before_state == next.state
                && before_counts == after_counts
                && !has_verifying
                && !has_auto_runnable
            {
                return Ok(next);
            }
        }

        Err(HivemindError::system(
            "auto_progress_limit_exceeded",
            "Auto flow progression exceeded safety iteration limit",
            "registry:auto_progress_flow",
        ))
    }

    /// Closes a task.
    ///
    /// # Errors
    /// Returns an error if the task is not found or already closed.
    pub fn close_task(&self, task_id: &str, reason: Option<&str>) -> Result<Task> {
        let task = self
            .get_task(task_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let state = self.state()?;
        let in_active_flow = state.flows.values().any(|f| {
            f.task_executions.contains_key(&task.id)
                && !matches!(
                    f.state,
                    FlowState::Completed | FlowState::Merged | FlowState::Aborted
                )
        });
        if in_active_flow {
            let err = HivemindError::user(
                "task_in_active_flow",
                "Task is part of an active flow",
                "registry:close_task",
            );
            self.record_error_event(&err, CorrelationIds::for_task(task.project_id, task.id));
            return Err(err);
        }

        if task.state == TaskState::Closed {
            // Idempotent: closing an already closed task is a no-op.
            return Ok(task);
        }

        let event = Event::new(
            EventPayload::TaskClosed {
                id: task.id,
                reason: reason.map(String::from),
            },
            CorrelationIds::for_task(task.project_id, task.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:close_task")
        })?;

        self.get_task(task_id)
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod tests {
    use super::*;
    use crate::core::registry::RegistryConfig;
    use crate::core::scope::RepoAccessMode;
    use tempfile::tempdir;

    #[test]
    fn auto_progress_tick_unblocks_dependent_task_after_success() {
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let repo_dir = temp.path().join("repo");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::create_dir_all(&repo_dir).expect("create repo dir");
        std::fs::write(repo_dir.join("README.md"), "seed\n").expect("write seed file");
        init_git_repo(&repo_dir);

        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(data_dir)).expect("registry");
        let project = registry
            .create_project("auto-progress-test", None)
            .expect("project");
        let project_id = project.id.to_string();
        registry
            .attach_repo(
                &project_id,
                repo_dir.to_str().expect("repo path"),
                Some("main"),
                RepoAccessMode::ReadWrite,
            )
            .expect("attach repo");
        let task_a = registry
            .create_task(&project_id, "Task A", None, None)
            .expect("task a");
        let task_b = registry
            .create_task(&project_id, "Task B", None, None)
            .expect("task b");
        let graph = registry
            .create_graph(&project_id, "graph", &[task_a.id, task_b.id])
            .expect("graph");
        let graph_id = graph.id.to_string();
        registry
            .add_graph_dependency(&graph_id, &task_a.id.to_string(), &task_b.id.to_string())
            .expect("dependency");
        let flow = registry.create_flow(&graph_id, Some("flow")).expect("flow");
        let flow_id = flow.id.to_string();
        registry.start_flow(&flow_id).expect("start flow");
        registry
            .task_runtime_set(
                &task_b.id.to_string(),
                "native",
                "builtin-native",
                None,
                &[],
                &[],
                60_000,
            )
            .expect("configure task b runtime");
        registry
            .start_task_execution(&task_a.id.to_string())
            .expect("start task a execution");
        registry
            .flow_set_run_mode(&flow_id, RunMode::Auto)
            .expect("set auto mode");

        let attempts = registry
            .list_attempts(Some(&flow_id), None, 10)
            .expect("attempts");
        assert_eq!(
            attempts.len(),
            1,
            "task A should be running before completion"
        );
        let attempt_id = attempts[0].attempt_id.to_string();
        let checkpoints = registry
            .list_checkpoints(&attempt_id)
            .expect("list checkpoints");
        registry
            .checkpoint_complete(
                &attempt_id,
                &checkpoints[0].checkpoint_id,
                Some("checkpoint complete"),
            )
            .expect("complete checkpoint");

        let updated_flow = registry
            .complete_task_execution(&task_a.id.to_string())
            .expect("complete task a");

        assert_eq!(
            updated_flow
                .task_executions
                .get(&task_a.id)
                .expect("task a execution")
                .state,
            TaskExecState::Success
        );
        assert_ne!(
            updated_flow
                .task_executions
                .get(&task_b.id)
                .expect("task b execution")
                .state,
            TaskExecState::Pending,
            "auto mode should immediately advance the newly-unblocked dependent task out of pending"
        );
        let attempts = registry
            .list_attempts(Some(&flow_id), None, 10)
            .expect("attempts after");
        assert_eq!(
            attempts.len(),
            2,
            "task B should have started automatically"
        );
    }

    #[test]
    fn verifying_success_in_auto_mode_unblocks_dependent_task() {
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let repo_dir = temp.path().join("repo");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::create_dir_all(&repo_dir).expect("create repo dir");
        std::fs::write(repo_dir.join("README.md"), "seed\n").expect("write seed file");
        init_git_repo(&repo_dir);

        let registry =
            Registry::open_with_config(RegistryConfig::with_dir(data_dir)).expect("registry");
        let project = registry
            .create_project("verify-progress-test", None)
            .expect("project");
        let project_id = project.id.to_string();
        registry
            .attach_repo(
                &project_id,
                repo_dir.to_str().expect("repo path"),
                Some("main"),
                RepoAccessMode::ReadWrite,
            )
            .expect("attach repo");
        let task_a = registry
            .create_task(&project_id, "Task A", None, None)
            .expect("task a");
        let task_b = registry
            .create_task(&project_id, "Task B", None, None)
            .expect("task b");
        let graph = registry
            .create_graph(&project_id, "graph", &[task_a.id, task_b.id])
            .expect("graph");
        let graph_id = graph.id.to_string();
        registry
            .add_graph_dependency(&graph_id, &task_a.id.to_string(), &task_b.id.to_string())
            .expect("dependency");
        let flow = registry.create_flow(&graph_id, Some("flow")).expect("flow");
        let flow_id = flow.id.to_string();
        registry.start_flow(&flow_id).expect("start flow");
        registry
            .task_runtime_set(
                &task_b.id.to_string(),
                "native",
                "builtin-native",
                None,
                &[],
                &[],
                60_000,
            )
            .expect("configure task b runtime");
        registry
            .start_task_execution(&task_a.id.to_string())
            .expect("start task a execution");

        let attempts = registry
            .list_attempts(Some(&flow_id), None, 10)
            .expect("attempts");
        let attempt_id = attempts[0].attempt_id.to_string();
        let checkpoints = registry
            .list_checkpoints(&attempt_id)
            .expect("list checkpoints");
        registry
            .checkpoint_complete(
                &attempt_id,
                &checkpoints[0].checkpoint_id,
                Some("checkpoint complete"),
            )
            .expect("complete checkpoint");
        let flow_after_completion = registry
            .complete_task_execution(&task_a.id.to_string())
            .expect("complete task a");
        assert_eq!(
            flow_after_completion
                .task_executions
                .get(&task_a.id)
                .expect("task a execution")
                .state,
            TaskExecState::Verifying
        );

        registry
            .flow_set_run_mode(&flow_id, RunMode::Auto)
            .expect("set auto mode");
        let updated_flow = registry
            .tick_flow(&flow_id, false, None)
            .expect("tick flow after verification success");

        assert_eq!(
            updated_flow
                .task_executions
                .get(&task_a.id)
                .expect("task a execution")
                .state,
            TaskExecState::Success
        );
        assert_ne!(
            updated_flow
                .task_executions
                .get(&task_b.id)
                .expect("task b execution")
                .state,
            TaskExecState::Pending,
            "auto mode should advance the newly-unblocked dependent task after verification succeeds"
        );
        let attempts = registry
            .list_attempts(Some(&flow_id), None, 10)
            .expect("attempts after verify success");
        assert_eq!(
            attempts.len(),
            2,
            "task B should have started automatically after verification success"
        );
    }

    fn init_git_repo(repo_dir: &std::path::Path) {
        run_git(repo_dir, &["init", "-q"]);
        run_git(repo_dir, &["config", "user.name", "Augment Agent"]);
        run_git(repo_dir, &["config", "user.email", "augment@example.com"]);
        run_git(repo_dir, &["add", "README.md"]);
        run_git(repo_dir, &["commit", "-qm", "seed"]);
    }

    fn run_git(repo_dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(repo_dir)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }
}
