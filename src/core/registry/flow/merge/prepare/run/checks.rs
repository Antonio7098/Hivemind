use super::*;

impl Registry {
    pub(super) fn run_aggregated_merge_checks(
        &self,
        flow: &TaskFlow,
        graph: &TaskGraph,
        successful_task_ids: &[Uuid],
        merge_path: &Path,
        origin: &'static str,
    ) -> Result<Vec<String>> {
        let target_dir = self
            .config
            .data_dir
            .join("cargo-target")
            .join(flow.id.to_string())
            .join("_integration_prepare")
            .join("checks");
        let _ = fs::create_dir_all(&target_dir);

        let unique_checks = Self::collect_unique_merge_checks(graph, successful_task_ids);
        let mut conflicts = Vec::new();

        for check in &unique_checks {
            self.append_event(
                Event::new(
                    EventPayload::MergeCheckStarted {
                        flow_id: flow.id,
                        task_id: None,
                        check_name: check.name.clone(),
                        required: check.required,
                    },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                ),
                origin,
            )?;

            let started = Instant::now();
            let (exit_code, combined) = match Self::run_check_command(
                merge_path,
                &target_dir,
                &check.command,
                check.timeout_ms,
            ) {
                Ok((exit_code, output, _timed_out)) => (exit_code, output),
                Err(e) => (127, e.to_string()),
            };
            let duration_ms =
                u64::try_from(started.elapsed().as_millis().min(u128::from(u64::MAX)))
                    .unwrap_or(u64::MAX);
            let passed = exit_code == 0;

            let safe_name = check
                .name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>();
            let out_path = target_dir.join(format!("merge_check_{safe_name}.log"));
            if let Err(e) = fs::write(&out_path, &combined) {
                let details = format!(
                    "failed to write check output for {} to {}: {}",
                    check.name,
                    out_path.display(),
                    e
                );
                conflicts.push(details.clone());
                self.emit_merge_conflict(flow, None, details, origin)?;
                break;
            }

            self.append_event(
                Event::new(
                    EventPayload::MergeCheckCompleted {
                        flow_id: flow.id,
                        task_id: None,
                        check_name: check.name.clone(),
                        passed,
                        exit_code,
                        output: combined.clone(),
                        duration_ms,
                        required: check.required,
                    },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                ),
                origin,
            )?;

            if check.required && !passed {
                let details = format!(
                    "required check failed: {} (exit={exit_code}, duration={}ms)",
                    check.name, duration_ms
                );
                conflicts.push(details.clone());
                self.emit_merge_conflict(flow, None, details, origin)?;
                if !combined.trim().is_empty() {
                    let snippet = combined.lines().take(10).collect::<Vec<_>>().join("\n");
                    conflicts.push(format!("check output (first lines): {snippet}"));
                }
                break;
            }
        }

        Ok(conflicts)
    }

    fn collect_unique_merge_checks(
        graph: &TaskGraph,
        successful_task_ids: &[Uuid],
    ) -> Vec<crate::core::verification::CheckConfig> {
        let mut unique_checks = Vec::new();
        for &task_id in successful_task_ids {
            if let Some(task) = graph.tasks.get(&task_id) {
                for check in &task.criteria.checks {
                    if let Some(existing) = unique_checks.iter_mut().find(
                        |c: &&mut crate::core::verification::CheckConfig| {
                            c.name == check.name && c.command == check.command
                        },
                    ) {
                        existing.required = existing.required || check.required;
                        if existing.timeout_ms.is_none() {
                            existing.timeout_ms = check.timeout_ms;
                        }
                    } else {
                        unique_checks.push(check.clone());
                    }
                }
            }
        }
        unique_checks
    }
}
