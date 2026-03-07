use super::*;

impl Registry {
    pub(crate) fn normalize_concatenated_json_objects(line: &str) -> String {
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

    pub(crate) fn read_mirror_events(path: &Path, origin: &str) -> Result<Vec<Event>> {
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
}
