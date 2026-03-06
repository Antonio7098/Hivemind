use super::*;

impl RuntimeLogIngestor {
    pub(crate) fn start(
        store: NativeRuntimeStateStore,
        owner_token: String,
        config: &RuntimeHardeningConfig,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<LogIngestorMessage>();
        let batch_size = config.log_batch_size;
        let flush_interval = Duration::from_millis(config.log_flush_interval_ms);
        let retention_days = config.log_retention_days;
        let lease_ttl_ms = config.lease_ttl_ms;
        let retention_interval = Duration::from_secs(config.retention_sweep_interval_secs);
        let thread_owner = owner_token.clone();

        let handle = thread::Builder::new()
            .name("hm-native-log-ingestor".to_string())
            .spawn(move || {
                run_log_ingestor_loop(
                    receiver,
                    store,
                    thread_owner,
                    batch_size,
                    flush_interval,
                    retention_days,
                    lease_ttl_ms,
                    retention_interval,
                );
            })
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_start_failed",
                    format!("Failed to start native runtime log ingestor thread: {error}"),
                )
            })?;

        Ok(Self {
            sender,
            handle: Some(handle),
        })
    }

    pub(crate) fn log(&self, log: RuntimeLogRecord) -> Result<()> {
        self.sender
            .send(LogIngestorMessage::Log(log))
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_send_failed",
                    format!("Failed to enqueue runtime log for ingestion: {error}"),
                )
            })
    }

    pub(crate) fn flush(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(LogIngestorMessage::Flush(tx))
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_send_failed",
                    format!("Failed to request runtime log flush: {error}"),
                )
            })?;
        rx.recv().map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_log_ingestor_recv_failed",
                format!("Failed to receive runtime log flush result: {error}"),
            )
        })?
    }

    pub(crate) fn shutdown(mut self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(LogIngestorMessage::Shutdown(tx))
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_send_failed",
                    format!("Failed to request runtime log ingestor shutdown: {error}"),
                )
            })?;
        let shutdown_result = rx.recv().map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_log_ingestor_recv_failed",
                format!("Failed to receive runtime log ingestor shutdown result: {error}"),
            )
        })?;
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        shutdown_result
    }
}

fn run_log_ingestor_loop(
    receiver: Receiver<LogIngestorMessage>,
    store: NativeRuntimeStateStore,
    owner_token: String,
    batch_size: usize,
    flush_interval: Duration,
    retention_days: u64,
    lease_ttl_ms: u64,
    retention_interval: Duration,
) {
    let mut pending = Vec::<RuntimeLogRecord>::new();
    let mut last_retention = Instant::now();

    loop {
        let message = receiver.recv_timeout(flush_interval);
        match message {
            Ok(LogIngestorMessage::Log(log)) => {
                pending.push(log);
                if pending.len() >= batch_size {
                    let _ = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                }
            }
            Ok(LogIngestorMessage::Flush(reply)) => {
                let result = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                let _ = reply.send(result);
            }
            Ok(LogIngestorMessage::Shutdown(reply)) => {
                let result = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                let _ = store.release_lease(LOG_INGESTOR_LEASE_NAME, &owner_token);
                let _ = store.release_lease(RETENTION_LEASE_NAME, &owner_token);
                let _ = reply.send(result);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !pending.is_empty() {
                    let _ = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                let _ = store.release_lease(LOG_INGESTOR_LEASE_NAME, &owner_token);
                let _ = store.release_lease(RETENTION_LEASE_NAME, &owner_token);
                break;
            }
        }

        if last_retention.elapsed() >= retention_interval {
            if let Ok(true) = store.acquire_lease(RETENTION_LEASE_NAME, &owner_token, lease_ttl_ms)
            {
                let _ = store.cleanup_logs(retention_days);
                let _ = store.heartbeat_lease(RETENTION_LEASE_NAME, &owner_token, lease_ttl_ms);
            }
            last_retention = Instant::now();
        }
    }
}

fn flush_pending_logs(
    store: &NativeRuntimeStateStore,
    owner_token: &str,
    lease_ttl_ms: u64,
    pending: &mut Vec<RuntimeLogRecord>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    store.heartbeat_lease(LOG_INGESTOR_LEASE_NAME, owner_token, lease_ttl_ms)?;
    store.ingest_logs(pending)?;
    pending.clear();
    Ok(())
}
