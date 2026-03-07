use super::*;

#[allow(clippy::significant_drop_tightening)]
impl EventStore for SqliteEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        use std::fs::OpenOptions;

        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&self.write_lock)?;
        lock_file.lock_exclusive()?;

        let rows = self.run_sql_json_query(
            "SELECT sequence, timestamp_nanos FROM events ORDER BY sequence DESC LIMIT 1;",
        )?;
        let last_row = rows.first();
        let last_seq = last_row
            .map(|row| Self::parse_i64_field(row, "sequence"))
            .transpose()?;
        let last_nanos = last_row
            .map(|row| Self::parse_i64_field(row, "timestamp_nanos"))
            .transpose()?;

        let next_seq = if let Some(seq) = last_seq {
            let seq_u64 = u64::try_from(seq).map_err(|_| {
                EventStoreError::Invariant("event sequence out of range".to_string())
            })?;
            seq_u64.saturating_add(1)
        } else {
            0
        };
        event.metadata.sequence = Some(next_seq);
        event.metadata.id = EventId::from_ordered_u64(next_seq);

        if let Some(last_nanos) = last_nanos {
            let mut event_nanos = timestamp_to_nanos(event.metadata.timestamp);
            if event_nanos <= last_nanos {
                event_nanos = last_nanos.saturating_add(1);
                event.metadata.timestamp = nanos_to_timestamp(event_nanos);
            }
        }

        let event_json = serde_json::to_string(&event)?;
        let corr = &event.metadata.correlation;
        let seq_i64 = i64::try_from(next_seq).map_err(|_| {
            EventStoreError::Invariant("event sequence exceeds sqlite integer range".to_string())
        })?;
        let insert_sql = format!(
            "INSERT INTO events (
                sequence, event_id, timestamp_nanos, timestamp_rfc3339,
                project_id, graph_id, flow_id, task_id, attempt_id, event_json
            ) VALUES (
                {seq_i64},
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
            timestamp_to_nanos(event.metadata.timestamp),
            Self::sql_text_expr(&event.metadata.timestamp.to_rfc3339()),
            Self::sql_optional_uuid_expr(corr.project_id),
            Self::sql_optional_uuid_expr(corr.graph_id),
            Self::sql_optional_uuid_expr(corr.flow_id),
            Self::sql_optional_uuid_expr(corr.task_id),
            Self::sql_optional_uuid_expr(corr.attempt_id),
            Self::sql_text_expr(&event_json),
        );

        let result = self.run_sql_batch(&insert_sql);
        let _ = lock_file.unlock();
        result?;
        self.append_legacy_mirror(&event_json)?;
        Ok(event.id())
    }

    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        let mut query = String::from("SELECT event_json FROM events");
        let mut conditions = Vec::new();

        if let Some(project_id) = filter.project_id {
            conditions.push(format!(
                "project_id = {}",
                Self::sql_text_expr(&project_id.to_string())
            ));
        }
        if let Some(graph_id) = filter.graph_id {
            conditions.push(format!(
                "graph_id = {}",
                Self::sql_text_expr(&graph_id.to_string())
            ));
        }
        if let Some(flow_id) = filter.flow_id {
            conditions.push(format!(
                "flow_id = {}",
                Self::sql_text_expr(&flow_id.to_string())
            ));
        }
        if let Some(task_id) = filter.task_id {
            conditions.push(format!(
                "task_id = {}",
                Self::sql_text_expr(&task_id.to_string())
            ));
        }
        if let Some(attempt_id) = filter.attempt_id {
            conditions.push(format!(
                "attempt_id = {}",
                Self::sql_text_expr(&attempt_id.to_string())
            ));
        }
        if let Some(since) = filter.since {
            conditions.push(format!("timestamp_nanos >= {}", timestamp_to_nanos(since)));
        }
        if let Some(until) = filter.until {
            conditions.push(format!("timestamp_nanos <= {}", timestamp_to_nanos(until)));
        }

        if !conditions.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&conditions.join(" AND "));
        }
        query.push_str(" ORDER BY sequence ASC;");

        let rows = self.run_sql_json_query(&query)?;
        let mut result = Vec::new();
        for row in rows {
            let event = Self::parse_event_field(&row)?;
            if !filter.matches(&event) {
                continue;
            }
            result.push(event);
            if let Some(limit) = filter.limit {
                if result.len() >= limit {
                    break;
                }
            }
        }
        Ok(result)
    }

    fn read_all(&self) -> Result<Vec<Event>> {
        let rows =
            self.run_sql_json_query("SELECT event_json FROM events ORDER BY sequence ASC;")?;
        let mut events = Vec::new();
        for row in rows {
            events.push(Self::parse_event_field(&row)?);
        }
        Ok(events)
    }

    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        let (tx, rx) = mpsc::channel();
        let filter = filter.clone();
        let db_path = self.db.clone();

        thread::spawn(move || {
            let mut sent = 0usize;
            let mut seen_sequence = -1_i64;

            loop {
                let query = format!(
                    "SELECT sequence, event_json FROM events WHERE sequence > {seen_sequence} ORDER BY sequence ASC;"
                );
                let Ok(rows) = Self::run_sql_json_query_on_path(&db_path, &query) else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };

                for row in rows {
                    let sequence = Self::parse_i64_field(&row, "sequence");
                    let raw = row
                        .get("event_json")
                        .and_then(serde_json::Value::as_str)
                        .map(std::string::ToString::to_string);
                    let (Ok(sequence), Some(raw)) = (sequence, raw) else {
                        continue;
                    };
                    seen_sequence = sequence;

                    let Ok(event) = serde_json::from_str::<Event>(&raw) else {
                        continue;
                    };
                    if !filter.matches(&event) {
                        continue;
                    }
                    if tx.send(event).is_err() {
                        return;
                    }

                    sent += 1;
                    if let Some(limit) = filter.limit {
                        if sent >= limit {
                            return;
                        }
                    }
                }

                thread::sleep(Duration::from_millis(200));
            }
        });

        Ok(rx)
    }
}
