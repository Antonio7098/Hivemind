use super::*;

impl Registry {
    pub(crate) fn attempt_runtime_outcome(
        &self,
        attempt_id: Uuid,
    ) -> Result<(Option<i32>, Option<String>)> {
        let filter = EventFilter {
            attempt_id: Some(attempt_id),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;

        let mut exit_code: Option<i32> = None;
        let mut terminated: Option<String> = None;
        for ev in events {
            match ev.payload {
                EventPayload::RuntimeExited { exit_code: ec, .. } => {
                    exit_code = Some(ec);
                }
                EventPayload::RuntimeTerminated { reason, .. } => {
                    terminated = Some(reason);
                }
                _ => {}
            }
        }

        if exit_code.is_none() && terminated.is_none() {
            return Ok((None, None));
        }
        Ok((exit_code, terminated))
    }
}
