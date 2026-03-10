use super::*;

const MAX_INLINE_NATIVE_BLOB_BYTES: usize = 64 * 1024;

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
            self.append_native_event(
                Self::projected_runtime_event_payload(attempt_id, observation),
                correlation,
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

    #[cfg(test)]
    pub(crate) fn native_blob_inline_payload_limit_bytes() -> usize {
        MAX_INLINE_NATIVE_BLOB_BYTES
    }

    fn inline_native_blob_payload(
        payload: &str,
        payload_bytes: &[u8],
        mode: NativePayloadCaptureMode,
    ) -> Option<String> {
        if !matches!(mode, NativePayloadCaptureMode::FullPayload) {
            return None;
        }
        if payload_bytes.len() > MAX_INLINE_NATIVE_BLOB_BYTES {
            return None;
        }
        Some(payload.to_string())
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

        Ok(NativeBlobRef {
            digest,
            byte_size: u64::try_from(payload_bytes.len()).unwrap_or(u64::MAX),
            media_type: media_type.to_string(),
            blob_path: blob_path.to_string_lossy().to_string(),
            payload: Self::inline_native_blob_payload(payload, payload_bytes, mode),
        })
    }

    pub(crate) fn append_native_event(
        &self,
        payload: EventPayload,
        correlation: &CorrelationIds,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(Event::new(payload, correlation.clone()), origin)
    }
}
