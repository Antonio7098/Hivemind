use super::*;
use crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_graph_query_executed_event(
        &self,
        project_id: Uuid,
        flow_id: Option<Uuid>,
        task_id: Option<Uuid>,
        attempt_id: Option<Uuid>,
        source: &str,
        result: &GraphQueryResult,
        duration_ms: u64,
        correlation: CorrelationIds,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::GraphQueryExecuted {
                    project_id,
                    flow_id,
                    task_id,
                    attempt_id,
                    source: source.to_string(),
                    query_kind: result.query_kind.clone(),
                    result_node_count: result.nodes.len(),
                    result_edge_count: result.edges.len(),
                    visited_node_count: result.cost.visited_nodes,
                    visited_edge_count: result.cost.visited_edges,
                    max_results: result.max_results,
                    truncated: result.truncated,
                    duration_ms,
                    canonical_fingerprint: result.canonical_fingerprint.clone(),
                },
                correlation,
            ),
            origin,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_graph_query_event_for_native_tool_call(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        tool_name: &str,
        response_payload: &str,
        correlation: CorrelationIds,
        origin: &'static str,
    ) -> Result<()> {
        if tool_name != "graph_query" {
            return Ok(());
        }
        let Ok(decoded) = serde_json::from_str::<serde_json::Value>(response_payload) else {
            return Ok(());
        };
        if decoded.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
            return Ok(());
        }
        let Some(output) = decoded.get("output") else {
            return Ok(());
        };
        let Ok(result) = serde_json::from_value::<GraphQueryResult>(output.clone()) else {
            return Ok(());
        };
        self.append_graph_query_executed_event(
            flow.project_id,
            Some(flow.id),
            Some(task_id),
            Some(attempt_id),
            "native_tool_graph_query",
            &result,
            result.duration_ms,
            correlation,
            origin,
        )
    }

    pub(crate) fn map_graph_query_error(
        err: crate::core::graph_query::GraphQueryError,
    ) -> HivemindError {
        let mut mapped = HivemindError::user(err.code, err.message, "registry:graph_query_execute");
        if matches!(
            mapped.code.as_str(),
            "graph_query_bounds_invalid"
                | "graph_query_bounds_exceeded"
                | "graph_query_depth_invalid"
                | "graph_query_depth_exceeded"
        ) {
            mapped =
                mapped.with_hint("Reduce --max-results/--depth to fit bounded graph query limits");
        } else if matches!(
            mapped.code.as_str(),
            "graph_query_node_not_found" | "graph_query_node_ambiguous"
        ) {
            mapped = mapped.with_hint(
                "Use 'hivemind graph query filter <project> --type file' to inspect available node IDs",
            );
        }
        mapped
    }

    pub(crate) fn encode_runtime_graph_query_gate_error(error: &HivemindError) -> String {
        let payload = RuntimeGraphQueryGateError {
            code: error.code.clone(),
            message: error.message.clone(),
            hint: error.recovery_hint.clone().or_else(|| {
                if error.code.starts_with("graph_snapshot_") {
                    Some(GRAPH_QUERY_REFRESH_HINT.to_string())
                } else {
                    None
                }
            }),
        };
        serde_json::to_string(&payload).unwrap_or_else(|_| {
            "{\"code\":\"graph_snapshot_invalid\",\"message\":\"graph query gate failed\"}"
                .to_string()
        })
    }

    pub(crate) fn set_native_graph_query_runtime_env(
        &self,
        project: &Project,
        runtime_env: &mut HashMap<String, String>,
        origin: &'static str,
    ) {
        runtime_env.remove(GRAPH_QUERY_ENV_GATE_ERROR);
        runtime_env.remove(GRAPH_QUERY_ENV_SNAPSHOT_PATH);
        runtime_env.remove(GRAPH_QUERY_ENV_CONSTITUTION_PATH);
        runtime_env.remove(GRAPH_QUERY_ENV_PROJECT_ID);

        if project.repositories.is_empty() {
            return;
        }

        match self.ensure_graph_snapshot_current_for_constitution(project, origin) {
            Ok(()) => {
                let constitution_path = self.constitution_path(project.id);
                let constitution = constitution_path
                    .is_file()
                    .then_some(constitution_path.as_path());
                let snapshot = self.read_graph_snapshot_artifact(project.id, origin);
                match snapshot.and_then(|artifact| {
                    artifact.map_or(Ok(()), |artifact| {
                        self.sync_graph_snapshot_into_runtime_registry(
                            project.id,
                            &artifact,
                            constitution,
                            origin,
                        )
                    })
                }) {
                    Ok(()) => {}
                    Err(error) => {
                        runtime_env.insert(
                            GRAPH_QUERY_ENV_GATE_ERROR.to_string(),
                            Self::encode_runtime_graph_query_gate_error(&error),
                        );
                        return;
                    }
                }
                let state_db_path =
                    crate::native::runtime_hardening::RuntimeHardeningConfig::for_state_dir(
                        &self.config.data_dir.join("native-runtime"),
                    )
                    .state_db_path;
                runtime_env.insert(
                    GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
                    self.graph_snapshot_path(project.id)
                        .to_string_lossy()
                        .to_string(),
                );
                runtime_env.insert(
                    GRAPH_QUERY_ENV_CONSTITUTION_PATH.to_string(),
                    constitution_path.to_string_lossy().to_string(),
                );
                runtime_env.insert(
                    GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
                    project.id.to_string(),
                );
                runtime_env.insert(
                    crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
                    state_db_path.to_string_lossy().to_string(),
                );
            }
            Err(error) => {
                if error.code.starts_with("graph_snapshot_") {
                    let _ =
                        self.mark_graph_snapshot_registry_freshness(project.id, "stale", origin);
                }
                runtime_env.insert(
                    GRAPH_QUERY_ENV_GATE_ERROR.to_string(),
                    Self::encode_runtime_graph_query_gate_error(&error),
                );
            }
        }
    }

    pub fn graph_query_execute(
        &self,
        id_or_name: &str,
        request: &GraphQueryRequest,
        source: &str,
    ) -> Result<GraphQueryResult> {
        let origin = "registry:graph_query_execute";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        if let Err(error) = self.ensure_graph_snapshot_current_for_constitution(&project, origin) {
            if error.code.starts_with("graph_snapshot_") {
                let _ = self.mark_graph_snapshot_registry_freshness(project.id, "stale", origin);
            }
            return Err(error);
        }
        let snapshot = self
            .read_authoritative_graph_snapshot_artifact(project.id, origin)?
            .ok_or_else(|| {
                HivemindError::user(
                    "graph_snapshot_missing",
                    "Graph snapshot is missing for this project",
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>")
            })?;

        let partition_paths =
            load_partition_paths_from_constitution(&self.constitution_path(project.id));
        let repositories = snapshot
            .repositories
            .iter()
            .map(|repo| GraphQueryRepository {
                repo_name: repo.repo_name.clone(),
                document: repo.document.clone(),
            })
            .collect::<Vec<_>>();
        let index = GraphQueryIndex::from_snapshot_repositories(&repositories, &partition_paths)
            .map_err(Self::map_graph_query_error)?;
        let started = Instant::now();
        let mut result = index
            .execute(
                request,
                &snapshot.canonical_fingerprint,
                &GraphQueryBounds::default(),
            )
            .map_err(Self::map_graph_query_error)?;
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        result.duration_ms = duration_ms;
        self.append_graph_query_executed_event(
            project.id,
            None,
            None,
            None,
            source,
            &result,
            duration_ms,
            CorrelationIds::for_project(project.id),
            origin,
        )?;
        Ok(result)
    }
}
