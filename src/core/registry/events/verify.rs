use super::*;

impl Registry {
    pub(crate) fn summarize_event_log(events: &[Event]) -> EventLogIntegritySummary {
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

    pub(crate) fn first_event_mismatch(
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

    pub(crate) fn validate_mirror_recovery_source(events: &[Event], origin: &str) -> Result<()> {
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
}
