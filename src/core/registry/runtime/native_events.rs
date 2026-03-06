use super::*;

impl Registry {
    pub(crate) fn projected_runtime_event_payload(
        attempt_id: Uuid,
        observation: ProjectedRuntimeObservation,
    ) -> EventPayload {
        match observation {
            ProjectedRuntimeObservation::CommandObserved { stream, command } => {
                EventPayload::RuntimeCommandObserved {
                    attempt_id,
                    stream,
                    command,
                }
            }
            ProjectedRuntimeObservation::ToolCallObserved {
                stream,
                tool_name,
                details,
            } => EventPayload::RuntimeToolCallObserved {
                attempt_id,
                stream,
                tool_name,
                details,
            },
            ProjectedRuntimeObservation::TodoSnapshotUpdated { stream, items } => {
                EventPayload::RuntimeTodoSnapshotUpdated {
                    attempt_id,
                    stream,
                    items,
                }
            }
            ProjectedRuntimeObservation::NarrativeOutputObserved { stream, content } => {
                EventPayload::RuntimeNarrativeOutputObserved {
                    attempt_id,
                    stream,
                    content,
                }
            }
        }
    }

    pub(crate) fn append_projected_runtime_observations(
        &self,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        observations: Vec<ProjectedRuntimeObservation>,
        origin: &'static str,
    ) -> Result<()> {
        for observation in observations {
            self.append_event(
                Event::new(
                    Self::projected_runtime_event_payload(attempt_id, observation),
                    correlation.clone(),
                ),
                origin,
            )?;
        }
        Ok(())
    }

    pub(crate) fn native_capture_mode_for_event(
        mode: NativePayloadCaptureMode,
    ) -> NativeEventPayloadCaptureMode {
        match mode {
            NativePayloadCaptureMode::MetadataOnly => NativeEventPayloadCaptureMode::MetadataOnly,
            NativePayloadCaptureMode::FullPayload => NativeEventPayloadCaptureMode::FullPayload,
        }
    }

    pub(crate) fn native_event_correlation(
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
    ) -> NativeEventCorrelation {
        NativeEventCorrelation {
            project_id: flow.project_id,
            graph_id: flow.graph_id,
            flow_id: flow.id,
            task_id,
            attempt_id,
        }
    }

    pub(crate) fn native_blob_sha256(payload: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(payload);
        let digest = hasher.finalize();
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(&mut hex, "{byte:02x}");
        }
        hex
    }

    pub(crate) fn persist_native_blob(
        &self,
        media_type: &str,
        payload: &str,
        mode: NativePayloadCaptureMode,
        origin: &'static str,
    ) -> Result<NativeBlobRef> {
        let payload_bytes = payload.as_bytes();
        let digest = Self::native_blob_sha256(payload_bytes);
        let blob_path = self.blob_path_for_digest(&digest);
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HivemindError::system("blob_write_failed", e.to_string(), origin)
                    .with_context("path", parent.display().to_string())
            })?;
        }
        if !blob_path.exists() {
            fs::write(&blob_path, payload_bytes).map_err(|e| {
                HivemindError::system("blob_write_failed", e.to_string(), origin)
                    .with_context("path", blob_path.display().to_string())
            })?;
        }

        let byte_size = u64::try_from(payload_bytes.len()).unwrap_or(u64::MAX);

        Ok(NativeBlobRef {
            digest,
            byte_size,
            media_type: media_type.to_string(),
            blob_path: blob_path.to_string_lossy().to_string(),
            payload: if matches!(mode, NativePayloadCaptureMode::FullPayload) {
                Some(payload.to_string())
            } else {
                None
            },
        })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn append_native_invocation_events(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        adapter_name: &str,
        invocation: &NativeInvocationTrace,
        origin: &'static str,
    ) -> Result<()> {
        let native_correlation = Self::native_event_correlation(flow, task_id, attempt_id);
        let capture_mode = Self::native_capture_mode_for_event(invocation.capture_mode);

        self.append_event(
            Event::new(
                EventPayload::AgentInvocationStarted {
                    native_correlation: native_correlation.clone(),
                    invocation_id: invocation.invocation_id.clone(),
                    adapter_name: adapter_name.to_string(),
                    provider: invocation.provider.clone(),
                    model: invocation.model.clone(),
                    runtime_version: invocation.runtime_version.clone(),
                    capture_mode,
                },
                correlation.clone(),
            ),
            origin,
        )?;

        let mut saw_tool_failure = false;

        for turn in &invocation.turns {
            self.append_event(
                Event::new(
                    EventPayload::AgentTurnStarted {
                        native_correlation: native_correlation.clone(),
                        invocation_id: invocation.invocation_id.clone(),
                        turn_index: turn.turn_index,
                        from_state: turn.from_state.clone(),
                    },
                    correlation.clone(),
                ),
                origin,
            )?;

            let request = self.persist_native_blob(
                "application/json",
                &turn.model_request,
                invocation.capture_mode,
                origin,
            )?;
            self.append_event(
                Event::new(
                    EventPayload::ModelRequestPrepared {
                        native_correlation: native_correlation.clone(),
                        invocation_id: invocation.invocation_id.clone(),
                        turn_index: turn.turn_index,
                        request,
                    },
                    correlation.clone(),
                ),
                origin,
            )?;

            let response = self.persist_native_blob(
                "text/plain; charset=utf-8",
                &turn.model_response,
                invocation.capture_mode,
                origin,
            )?;
            self.append_event(
                Event::new(
                    EventPayload::ModelResponseReceived {
                        native_correlation: native_correlation.clone(),
                        invocation_id: invocation.invocation_id.clone(),
                        turn_index: turn.turn_index,
                        response,
                    },
                    correlation.clone(),
                ),
                origin,
            )?;

            for tool_call in &turn.tool_calls {
                let request = self.persist_native_blob(
                    "text/plain; charset=utf-8",
                    &tool_call.request,
                    invocation.capture_mode,
                    origin,
                )?;
                self.append_event(
                    Event::new(
                        EventPayload::ToolCallRequested {
                            native_correlation: native_correlation.clone(),
                            task_id: Some(task_id),
                            invocation_id: invocation.invocation_id.clone(),
                            turn_index: turn.turn_index,
                            call_id: tool_call.call_id.clone(),
                            tool_name: tool_call.tool_name.clone(),
                            request,
                            policy_tags: tool_call.policy_tags.clone(),
                        },
                        correlation.clone(),
                    ),
                    origin,
                )?;

                self.append_event(
                    Event::new(
                        EventPayload::ToolCallStarted {
                            native_correlation: native_correlation.clone(),
                            task_id: Some(task_id),
                            invocation_id: invocation.invocation_id.clone(),
                            turn_index: turn.turn_index,
                            call_id: tool_call.call_id.clone(),
                            tool_name: tool_call.tool_name.clone(),
                            policy_tags: tool_call.policy_tags.clone(),
                        },
                        correlation.clone(),
                    ),
                    origin,
                )?;

                if let Some(failure) = &tool_call.failure {
                    saw_tool_failure = true;
                    self.append_event(
                        Event::new(
                            EventPayload::ToolCallFailed {
                                native_correlation: native_correlation.clone(),
                                task_id: Some(task_id),
                                invocation_id: invocation.invocation_id.clone(),
                                turn_index: turn.turn_index,
                                call_id: tool_call.call_id.clone(),
                                tool_name: tool_call.tool_name.clone(),
                                code: failure.code.clone(),
                                message: failure.message.clone(),
                                recoverable: failure.recoverable,
                                policy_tags: tool_call.policy_tags.clone(),
                            },
                            correlation.clone(),
                        ),
                        origin,
                    )?;
                } else if let Some(response_payload) = tool_call.response.as_deref() {
                    let response = self.persist_native_blob(
                        "text/plain; charset=utf-8",
                        response_payload,
                        invocation.capture_mode,
                        origin,
                    )?;
                    self.append_event(
                        Event::new(
                            EventPayload::ToolCallCompleted {
                                native_correlation: native_correlation.clone(),
                                task_id: Some(task_id),
                                invocation_id: invocation.invocation_id.clone(),
                                turn_index: turn.turn_index,
                                call_id: tool_call.call_id.clone(),
                                tool_name: tool_call.tool_name.clone(),
                                response,
                                policy_tags: tool_call.policy_tags.clone(),
                            },
                            correlation.clone(),
                        ),
                        origin,
                    )?;
                    self.append_graph_query_event_for_native_tool_call(
                        flow,
                        task_id,
                        attempt_id,
                        &tool_call.tool_name,
                        response_payload,
                        correlation.clone(),
                        origin,
                    )?;
                }
            }

            self.append_event(
                Event::new(
                    EventPayload::AgentTurnCompleted {
                        native_correlation: native_correlation.clone(),
                        invocation_id: invocation.invocation_id.clone(),
                        turn_index: turn.turn_index,
                        to_state: turn.to_state.clone(),
                        summary: turn.turn_summary.clone(),
                    },
                    correlation.clone(),
                ),
                origin,
            )?;
        }

        for attempt in &invocation.transport.attempts {
            let category = if attempt.rate_limited {
                "rate_limit"
            } else if attempt.code.starts_with("native_stream_") {
                "transport_stream"
            } else {
                "transport"
            };
            self.append_event(
                Event::new(
                    EventPayload::RuntimeErrorClassified {
                        attempt_id,
                        adapter_name: adapter_name.to_string(),
                        code: attempt.code.clone(),
                        category: category.to_string(),
                        message: attempt.message.clone(),
                        recoverable: attempt.retryable,
                        retryable: attempt.retryable,
                        rate_limited: attempt.rate_limited,
                    },
                    correlation.clone(),
                ),
                origin,
            )?;

            if let Some(backoff_ms) = attempt.backoff_ms {
                self.append_event(
                    Event::new(
                        EventPayload::RuntimeRecoveryScheduled {
                            attempt_id,
                            from_adapter: format!("{adapter_name}:{}", attempt.transport),
                            to_adapter: format!("{adapter_name}:{}", attempt.transport),
                            strategy: "native_transport_retry".to_string(),
                            reason: format!(
                                "turn={} code={} retryable={}",
                                attempt.turn_index, attempt.code, attempt.retryable
                            ),
                            backoff_ms,
                        },
                        correlation.clone(),
                    ),
                    origin,
                )?;
            }
        }

        for fallback in &invocation.transport.fallback_activations {
            self.append_event(
                Event::new(
                    EventPayload::RuntimeRecoveryScheduled {
                        attempt_id,
                        from_adapter: format!("{adapter_name}:{}", fallback.from_transport),
                        to_adapter: format!("{adapter_name}:{}", fallback.to_transport),
                        strategy: "native_transport_fallback".to_string(),
                        reason: format!("turn={} reason={}", fallback.turn_index, fallback.reason),
                        backoff_ms: 0,
                    },
                    correlation.clone(),
                ),
                origin,
            )?;
        }

        if let Some(runtime_state) = invocation.runtime_state.as_ref() {
            self.append_event(
                Event::new(
                    EventPayload::RuntimeRecoveryScheduled {
                        attempt_id,
                        from_adapter: format!("{adapter_name}:runtime_state:init"),
                        to_adapter: format!("{adapter_name}:runtime_state:ready"),
                        strategy: "native_runtime_state_bootstrap".to_string(),
                        reason: format!(
                            "db_path={} busy_timeout_ms={} log_batch_size={} retention_days={}",
                            runtime_state.db_path,
                            runtime_state.busy_timeout_ms,
                            runtime_state.log_batch_size,
                            runtime_state.log_retention_days
                        ),
                        backoff_ms: 0,
                    },
                    correlation.clone(),
                ),
                origin,
            )?;
        }

        for transition in &invocation.readiness_transitions {
            self.append_event(
                Event::new(
                    EventPayload::RuntimeRecoveryScheduled {
                        attempt_id,
                        from_adapter: format!(
                            "{adapter_name}:{}:{}",
                            transition.component, transition.from_state
                        ),
                        to_adapter: format!(
                            "{adapter_name}:{}:{}",
                            transition.component, transition.to_state
                        ),
                        strategy: "native_component_readiness".to_string(),
                        reason: format!(
                            "token={} ts_ms={} reason={}",
                            transition.token, transition.timestamp_ms, transition.reason
                        ),
                        backoff_ms: 0,
                    },
                    correlation.clone(),
                ),
                origin,
            )?;

            if transition.to_state == "failed" {
                self.append_event(
                    Event::new(
                        EventPayload::RuntimeErrorClassified {
                            attempt_id,
                            adapter_name: adapter_name.to_string(),
                            code: "native_component_not_ready".to_string(),
                            category: "runtime_setup".to_string(),
                            message: format!(
                                "Native component '{}' failed readiness transition: {}",
                                transition.component, transition.reason
                            ),
                            recoverable: false,
                            retryable: false,
                            rate_limited: false,
                        },
                        correlation.clone(),
                    ),
                    origin,
                )?;
            }
        }

        let success = invocation.failure.is_none() && !saw_tool_failure;
        let (error_code, error_message, recoverable) = invocation.failure.as_ref().map_or_else(
            || {
                if saw_tool_failure {
                    (
                        Some("native_tool_call_failed".to_string()),
                        Some("One or more native tool calls failed".to_string()),
                        Some(false),
                    )
                } else {
                    (None, None, None)
                }
            },
            |failure| {
                (
                    Some(failure.code.clone()),
                    Some(failure.message.clone()),
                    Some(failure.recoverable),
                )
            },
        );

        self.append_event(
            Event::new(
                EventPayload::AgentInvocationCompleted {
                    native_correlation,
                    invocation_id: invocation.invocation_id.clone(),
                    total_turns: u32::try_from(invocation.turns.len()).unwrap_or(u32::MAX),
                    final_state: invocation.final_state.clone(),
                    success,
                    final_summary: invocation.final_summary.clone(),
                    error_code,
                    error_message,
                    recoverable,
                },
                correlation.clone(),
            ),
            origin,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_blob_sha256_is_stable() {
        let digest = Registry::native_blob_sha256(b"hello world");
        assert_eq!(digest.len(), 64);
        assert_eq!(digest, Registry::native_blob_sha256(b"hello world"));
    }

    #[test]
    fn native_capture_mode_maps_to_event_mode() {
        assert_eq!(
            Registry::native_capture_mode_for_event(NativePayloadCaptureMode::MetadataOnly),
            NativeEventPayloadCaptureMode::MetadataOnly
        );
        assert_eq!(
            Registry::native_capture_mode_for_event(NativePayloadCaptureMode::FullPayload),
            NativeEventPayloadCaptureMode::FullPayload
        );
    }
}
