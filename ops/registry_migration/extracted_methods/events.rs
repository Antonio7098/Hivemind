// AUTO-GENERATED from src/core/registry_original.rs
// Module bucket: events

// append_event (1742-1747)
    fn append_event(&self, event: Event, origin: &'static str) -> Result<()> {
        self.store
            .append(event)
            .map(|_| ())
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))
    }

// record_error_event (2731-2736)
    fn record_error_event(&self, err: &HivemindError, correlation: CorrelationIds) {
        let _ = self.store.append(Event::new(
            EventPayload::ErrorOccurred { error: err.clone() },
            correlation,
        ));
    }

// read_events (6331-6335)
    pub fn read_events(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        self.store.read(filter).map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:read_events")
        })
    }

// stream_events (6337-6345)
    pub fn stream_events(&self, filter: &EventFilter) -> Result<std::sync::mpsc::Receiver<Event>> {
        self.store.stream(filter).map_err(|e| {
            HivemindError::system(
                "event_stream_failed",
                e.to_string(),
                "registry:stream_events",
            )
        })
    }

// get_event (6347-6374)
    /// Gets a specific event by ID.
    ///
    /// # Errors
    /// Returns an error if the event cannot be read or is not found.
    pub fn get_event(&self, event_id: &str) -> Result<Event> {
        let id = Uuid::parse_str(event_id).map_err(|_| {
            HivemindError::user(
                "invalid_event_id",
                format!("'{event_id}' is not a valid event ID"),
                "registry:get_event",
            )
        })?;

        let events = self.store.read_all().map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:get_event")
        })?;

        events
            .into_iter()
            .find(|e| e.metadata.id.as_uuid() == id)
            .ok_or_else(|| {
                HivemindError::user(
                    "event_not_found",
                    format!("Event '{event_id}' not found"),
                    "registry:get_event",
                )
            })
    }

// normalize_concatenated_json_objects (6376-6403)
    fn normalize_concatenated_json_objects(line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        let mut in_string = false;
        let mut escape = false;

        while let Some(c) = chars.next() {
            if in_string {
                if escape {
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            }

            out.push(c);

            if !in_string && c == '}' && chars.peek().copied() == Some('{') {
                out.push('\n');
            }
        }

        out
    }

// read_mirror_events (6405-6442)
    fn read_mirror_events(path: &Path, origin: &str) -> Result<Vec<Event>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let raw = fs::read_to_string(path).map_err(|err| {
            HivemindError::system(
                "events_mirror_read_failed",
                format!("Failed to read mirror at '{}': {err}", path.display()),
                origin,
            )
        })?;

        let mut events = Vec::new();
        for (line_idx, line) in raw.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let normalized = Self::normalize_concatenated_json_objects(line);
            let stream = serde_json::Deserializer::from_str(&normalized).into_iter::<Event>();
            for item in stream {
                let event = item.map_err(|err| {
                    HivemindError::system(
                        "events_mirror_parse_failed",
                        format!(
                            "Failed to parse mirror event at '{}', line {}: {err}",
                            path.display(),
                            line_idx + 1
                        ),
                        origin,
                    )
                })?;
                events.push(event);
            }
        }

        Ok(events)
    }

// summarize_event_log (6444-6481)
    fn summarize_event_log(events: &[Event]) -> EventLogIntegritySummary {
        let mut event_ids = HashSet::new();
        let mut duplicate_event_id_count = 0usize;
        let mut sequences = HashSet::new();
        let mut missing_sequence_count = 0usize;

        for event in events {
            if !event_ids.insert(event.id().as_uuid()) {
                duplicate_event_id_count = duplicate_event_id_count.saturating_add(1);
            }

            if let Some(sequence) = event.metadata.sequence {
                sequences.insert(sequence);
            } else {
                missing_sequence_count = missing_sequence_count.saturating_add(1);
            }
        }

        let sequence_min = sequences.iter().min().copied();
        let sequence_max = sequences.iter().max().copied();
        let sequence_gap_count =
            sequence_min
                .zip(sequence_max)
                .map_or(0usize, |(min_seq, max_seq)| {
                    let expected = max_seq.saturating_sub(min_seq).saturating_add(1);
                    let actual = sequences.len() as u64;
                    usize::try_from(expected.saturating_sub(actual)).unwrap_or(usize::MAX)
                });

        EventLogIntegritySummary {
            event_count: events.len(),
            sequence_min,
            sequence_max,
            sequence_gap_count,
            duplicate_event_id_count,
            missing_sequence_count,
        }
    }

// first_event_mismatch (6483-6502)
    fn first_event_mismatch(
        sqlite_events: &[Event],
        mirror_events: &[Event],
    ) -> (Option<usize>, Option<String>, Option<String>) {
        let limit = sqlite_events.len().max(mirror_events.len());
        for idx in 0..limit {
            let sqlite_event = sqlite_events.get(idx);
            let mirror_event = mirror_events.get(idx);
            if sqlite_event == mirror_event {
                continue;
            }
            return (
                Some(idx),
                sqlite_event.map(|event| event.id().to_string()),
                mirror_event.map(|event| event.id().to_string()),
            );
        }

        (None, None, None)
    }

// validate_mirror_recovery_source (6504-6547)
    fn validate_mirror_recovery_source(events: &[Event], origin: &str) -> Result<()> {
        if events.is_empty() {
            return Err(HivemindError::user(
                "events_recover_mirror_empty",
                "Cannot recover from an empty events mirror",
                origin,
            )
            .with_hint("Populate `events.jsonl` or restore it from backup before recovery"));
        }

        let mut ids = HashSet::new();
        for (index, event) in events.iter().enumerate() {
            let expected_sequence = u64::try_from(index).unwrap_or(u64::MAX);
            let sequence = event.metadata.sequence.ok_or_else(|| {
                HivemindError::user(
                    "events_recover_missing_sequence",
                    format!("Mirror event at index {index} is missing sequence metadata"),
                    origin,
                )
            })?;
            if sequence != expected_sequence {
                return Err(HivemindError::user(
                    "events_recover_sequence_mismatch",
                    format!(
                        "Mirror event at index {index} has sequence {sequence}, expected {expected_sequence}"
                    ),
                    origin,
                )
                .with_hint("Run `hivemind events verify` and inspect mirror ordering before recovery"));
            }
            if !ids.insert(event.id().as_uuid()) {
                return Err(HivemindError::user(
                    "events_recover_duplicate_event_id",
                    format!(
                        "Mirror contains duplicate event ID '{}' at index {index}",
                        event.id()
                    ),
                    origin,
                ));
            }
        }

        Ok(())
    }

// timestamp_to_nanos (6549-6553)
    fn timestamp_to_nanos(timestamp: chrono::DateTime<Utc>) -> i64 {
        timestamp
            .timestamp_nanos_opt()
            .unwrap_or_else(|| timestamp.timestamp_micros().saturating_mul(1_000))
    }

// sql_text_expr (6555-6561)
    fn sql_text_expr(value: &str) -> String {
        let mut hex = String::with_capacity(value.len().saturating_mul(2));
        for byte in value.as_bytes() {
            let _ = write!(&mut hex, "{byte:02x}");
        }
        format!("CAST(X'{hex}' AS TEXT)")
    }

// sql_optional_uuid_expr (6563-6568)
    fn sql_optional_uuid_expr(value: Option<Uuid>) -> String {
        value.map_or_else(
            || "NULL".to_string(),
            |id| Self::sql_text_expr(&id.to_string()),
        )
    }

// sqlite_exec (6570-6629)
    fn sqlite_exec(db_path: &Path, sql: &str, origin: &str) -> Result<()> {
        let mut child = Command::new("sqlite3")
            .arg("-cmd")
            .arg(".timeout 5000")
            .arg(db_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                HivemindError::system(
                    "events_sqlite_exec_failed",
                    format!(
                        "Failed to invoke sqlite3 for '{}': {err}",
                        db_path.display()
                    ),
                    origin,
                )
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(sql.as_bytes()).map_err(|err| {
                HivemindError::system(
                    "events_sqlite_exec_failed",
                    format!(
                        "Failed to send SQL to sqlite3 for '{}': {err}",
                        db_path.display()
                    ),
                    origin,
                )
            })?;
        }

        let output = child.wait_with_output().map_err(|err| {
            HivemindError::system(
                "events_sqlite_exec_failed",
                format!(
                    "Failed to wait for sqlite3 completion for '{}': {err}",
                    db_path.display()
                ),
                origin,
            )
        })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "sqlite3 command failed with empty stderr".to_string()
        } else {
            stderr
        };
        Err(HivemindError::system(
            "events_sqlite_exec_failed",
            format!("sqlite3 failed for '{}': {message}", db_path.display()),
            origin,
        ))
    }

// build_sqlite_recovery_sql (6631-6688)
    fn build_sqlite_recovery_sql(events: &[Event], origin: &str) -> Result<String> {
        let mut sql = String::from("BEGIN IMMEDIATE;\n");
        for event in events {
            let sequence = event.metadata.sequence.ok_or_else(|| {
                HivemindError::user(
                    "events_recover_missing_sequence",
                    "Cannot recover event without sequence metadata",
                    origin,
                )
            })?;
            let sequence = i64::try_from(sequence).map_err(|_| {
                HivemindError::system(
                    "events_recover_sequence_out_of_range",
                    "Mirror event sequence exceeds sqlite integer range",
                    origin,
                )
            })?;

            let corr = &event.metadata.correlation;
            let event_json = serde_json::to_string(event).map_err(|err| {
                HivemindError::system(
                    "events_recover_serialize_failed",
                    format!("Failed to serialize event during recovery: {err}"),
                    origin,
                )
            })?;

            let _ = writeln!(
                sql,
                "INSERT INTO events (
                    sequence, event_id, timestamp_nanos, timestamp_rfc3339,
                    project_id, graph_id, flow_id, task_id, attempt_id, event_json
                ) VALUES (
                    {sequence},
                    {},
                    {},
                    {},
                    {},
                    {},
                    {},
                    {},
                    {},
                    {}
                );",
                Self::sql_text_expr(&event.id().to_string()),
                Self::timestamp_to_nanos(event.metadata.timestamp),
                Self::sql_text_expr(&event.metadata.timestamp.to_rfc3339()),
                Self::sql_optional_uuid_expr(corr.project_id),
                Self::sql_optional_uuid_expr(corr.graph_id),
                Self::sql_optional_uuid_expr(corr.flow_id),
                Self::sql_optional_uuid_expr(corr.task_id),
                Self::sql_optional_uuid_expr(corr.attempt_id),
                Self::sql_text_expr(&event_json),
            );
        }
        sql.push_str("COMMIT;");
        Ok(sql)
    }

// events_verify (6690-6720)
    /// Verifies canonical `SQLite` and mirror event-store integrity/parity.
    ///
    /// # Errors
    /// Returns an error if either event source cannot be read.
    pub fn events_verify(&self) -> Result<EventsVerifyResult> {
        let origin = "registry:events_verify";
        let sqlite_events = self
            .store
            .read_all()
            .map_err(|err| HivemindError::system("event_read_failed", err.to_string(), origin))?;
        let mirror_path = self.config.events_path();
        let mirror_events = Self::read_mirror_events(&mirror_path, origin)?;

        let sqlite = Self::summarize_event_log(&sqlite_events);
        let mirror = Self::summarize_event_log(&mirror_events);
        let (first_mismatch_index, first_mismatch_sqlite_event_id, first_mismatch_mirror_event_id) =
            Self::first_event_mismatch(&sqlite_events, &mirror_events);
        let parity_ok = sqlite.event_count == mirror.event_count && first_mismatch_index.is_none();

        Ok(EventsVerifyResult {
            checked_at: Utc::now(),
            sqlite_path: self.config.db_path().to_string_lossy().to_string(),
            mirror_path: mirror_path.to_string_lossy().to_string(),
            parity_ok,
            first_mismatch_index,
            first_mismatch_sqlite_event_id,
            first_mismatch_mirror_event_id,
            sqlite,
            mirror,
        })
    }

// events_recover_from_mirror (6722-6875)
    /// Rebuilds canonical `SQLite` state from the append-only mirror.
    ///
    /// # Errors
    /// Returns an error if confirmation is missing, mirror input is invalid, or rebuild fails.
    #[allow(clippy::too_many_lines)]
    pub fn events_recover_from_mirror(&self, confirm: bool) -> Result<EventsRecoverResult> {
        let origin = "registry:events_recover_from_mirror";
        if !confirm {
            return Err(HivemindError::user(
                "events_recover_confirmation_required",
                "Recovering canonical event storage requires explicit confirmation",
                origin,
            )
            .with_hint("Re-run with `hivemind events recover --from-mirror --confirm`"));
        }

        let mirror_path = self.config.events_path();
        let mirror_events = Self::read_mirror_events(&mirror_path, origin)?;
        Self::validate_mirror_recovery_source(&mirror_events, origin)?;

        let recovery_root = self.config.data_dir.join("recovery");
        fs::create_dir_all(&recovery_root).map_err(|err| {
            HivemindError::system(
                "events_recover_prepare_failed",
                format!(
                    "Failed to create recovery directory '{}': {err}",
                    recovery_root.display()
                ),
                origin,
            )
        })?;

        let temp_root = recovery_root.join(format!("events-rebuild-{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_root).map_err(|err| {
            HivemindError::system(
                "events_recover_prepare_failed",
                format!(
                    "Failed to create recovery workspace '{}': {err}",
                    temp_root.display()
                ),
                origin,
            )
        })?;

        SqliteEventStore::open(&temp_root).map_err(|err| {
            HivemindError::system(
                "events_recover_prepare_failed",
                format!(
                    "Failed to initialize temporary sqlite store '{}': {err}",
                    temp_root.display()
                ),
                origin,
            )
        })?;

        let temp_db_path = temp_root.join("db.sqlite");
        let insert_sql = Self::build_sqlite_recovery_sql(&mirror_events, origin)?;
        Self::sqlite_exec(&temp_db_path, &insert_sql, origin)?;

        let db_path = self.config.db_path();
        let backup_path = if db_path.exists() {
            let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
            let path = recovery_root.join(format!("db.sqlite.{stamp}.bak"));
            fs::copy(&db_path, &path).map_err(|err| {
                HivemindError::system(
                    "events_recover_backup_failed",
                    format!(
                        "Failed to back up sqlite file '{}' to '{}': {err}",
                        db_path.display(),
                        path.display()
                    ),
                    origin,
                )
            })?;
            Some(path)
        } else {
            None
        };

        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                HivemindError::system(
                    "events_recover_prepare_failed",
                    format!(
                        "Failed to ensure sqlite parent directory '{}': {err}",
                        parent.display()
                    ),
                    origin,
                )
            })?;
        }

        for suffix in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{}", db_path.to_string_lossy(), suffix));
            if sidecar.exists() {
                fs::remove_file(&sidecar).map_err(|err| {
                    HivemindError::system(
                        "events_recover_replace_failed",
                        format!(
                            "Failed to remove sqlite sidecar '{}': {err}",
                            sidecar.display()
                        ),
                        origin,
                    )
                })?;
            }
        }

        if db_path.exists() {
            fs::remove_file(&db_path).map_err(|err| {
                HivemindError::system(
                    "events_recover_replace_failed",
                    format!(
                        "Failed to remove existing sqlite file '{}': {err}",
                        db_path.display()
                    ),
                    origin,
                )
            })?;
        }
        fs::copy(&temp_db_path, &db_path).map_err(|err| {
            HivemindError::system(
                "events_recover_replace_failed",
                format!(
                    "Failed to replace sqlite file '{}' from '{}': {err}",
                    db_path.display(),
                    temp_db_path.display()
                ),
                origin,
            )
        })?;

        let _ = fs::remove_dir_all(&temp_root);

        let verification = self.events_verify()?;
        if !verification.parity_ok {
            return Err(HivemindError::system(
                "events_recover_verification_failed",
                "Recovery completed but canonical/mirror parity verification failed",
                origin,
            )
            .with_hint("Run `hivemind events verify` and inspect mismatch details"));
        }

        Ok(EventsRecoverResult {
            recovered_at: Utc::now(),
            source: "mirror".to_string(),
            sqlite_path: db_path.to_string_lossy().to_string(),
            mirror_path: mirror_path.to_string_lossy().to_string(),
            backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
            recovered_event_count: mirror_events.len(),
            verification,
        })
    }

