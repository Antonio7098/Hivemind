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
        let mut exit_code_zero_before_failure = false;
        for ev in events {
            match ev.payload {
                EventPayload::RuntimeExited { exit_code: ec, .. } => {
                    if ec == 0 {
                        exit_code_zero_before_failure = true;
                    }
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

        let final_exit_code = if exit_code_zero_before_failure
            && terminated
                .as_ref()
                .is_some_and(|r| r.starts_with("checkpoints_incomplete:"))
        {
            Some(0)
        } else {
            exit_code
        };

        Ok((final_exit_code, terminated))
    }
}
