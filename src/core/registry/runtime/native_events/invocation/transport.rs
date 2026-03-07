use super::*;

impl Registry {
    pub(super) fn append_native_transport_events(
        &self,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        adapter_name: &str,
        invocation: &NativeInvocationTrace,
        origin: &'static str,
    ) -> Result<()> {
        for attempt in &invocation.transport.attempts {
            let category = if attempt.rate_limited {
                "rate_limit"
            } else if attempt.code.starts_with("native_stream_") {
                "transport_stream"
            } else {
                "transport"
            };
            self.append_native_event(
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
                correlation,
                origin,
            )?;

            if let Some(backoff_ms) = attempt.backoff_ms {
                self.append_native_event(
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
                    correlation,
                    origin,
                )?;
            }
        }

        for fallback in &invocation.transport.fallback_activations {
            self.append_native_event(
                EventPayload::RuntimeRecoveryScheduled {
                    attempt_id,
                    from_adapter: format!("{adapter_name}:{}", fallback.from_transport),
                    to_adapter: format!("{adapter_name}:{}", fallback.to_transport),
                    strategy: "native_transport_fallback".to_string(),
                    reason: format!("turn={} reason={}", fallback.turn_index, fallback.reason),
                    backoff_ms: 0,
                },
                correlation,
                origin,
            )?;
        }

        Ok(())
    }
}
