use super::*;

pub(super) fn payload_map(payload: &EventPayload) -> Result<HashMap<String, Value>> {
    let mut v = serde_json::to_value(payload).map_err(|e| {
        HivemindError::system(
            "payload_serialize_failed",
            e.to_string(),
            "server:payload_map",
        )
    })?;

    match &mut v {
        Value::Object(map) => {
            map.remove("type");
            Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        _ => Ok(HashMap::new()),
    }
}
