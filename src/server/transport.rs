use super::*;
use crate::app::EventService;
use std::sync::mpsc;

pub(super) fn build_runtime_stream_sse_response(
    url: &str,
    event_service: &EventService,
) -> Result<tiny_http::Response<ChannelReader>> {
    let query = parse_query(url);
    let flow_id = query
        .get("flow_id")
        .map(|value| {
            Uuid::parse_str(value).map_err(|e| {
                HivemindError::user(
                    "invalid_flow_id",
                    format!("Invalid flow_id: {e}"),
                    "server:runtime_stream:sse",
                )
            })
        })
        .transpose()?;
    let attempt_id = query
        .get("attempt_id")
        .map(|value| {
            Uuid::parse_str(value).map_err(|e| {
                HivemindError::user(
                    "invalid_attempt_id",
                    format!("Invalid attempt_id: {e}"),
                    "server:runtime_stream:sse",
                )
            })
        })
        .transpose()?;

    let mut filter = EventFilter::all();
    filter.flow_id = flow_id;
    filter.attempt_id = attempt_id;
    let rx_events = event_service.stream_events(&filter)?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let _ = tx.send(b": connected\n\n".to_vec());

    std::thread::spawn(move || {
        while let Ok(event) = rx_events.recv() {
            if let Some(item) = runtime_stream_item(event) {
                let payload = RuntimeStreamEnvelope {
                    cursor: item.sequence,
                    item,
                };
                match serde_json::to_string(&payload) {
                    Ok(json) => {
                        if tx
                            .send(format!("event: runtime\ndata: {json}\n\n").into_bytes())
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => {
                        if tx.send(b"event: error\ndata: {}\n\n".to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    let mut headers = cors_headers();
    headers.push(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream"[..])
            .expect("sse content-type header"),
    );
    headers.push(
        tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..])
            .expect("sse cache-control header"),
    );
    headers.push(
        tiny_http::Header::from_bytes(&b"Connection"[..], &b"keep-alive"[..])
            .expect("sse connection header"),
    );

    Ok(tiny_http::Response::new(
        tiny_http::StatusCode(200),
        headers,
        ChannelReader::new(rx),
        None,
        None,
    ))
}

pub(super) fn build_chat_stream_sse_response(
    url: &str,
    event_service: &EventService,
) -> Result<tiny_http::Response<ChannelReader>> {
    let query = parse_query(url);
    let session_id = query
        .get("session_id")
        .map(|value| {
            Uuid::parse_str(value).map_err(|e| {
                HivemindError::user(
                    "invalid_chat_session_id",
                    format!("Invalid session_id: {e}"),
                    "server:chat_stream:sse",
                )
            })
        })
        .transpose()?;

    let rx_events = event_service.stream_events(&EventFilter::all())?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let _ = tx.send(b": connected\n\n".to_vec());

    std::thread::spawn(move || {
        while let Ok(event) = rx_events.recv() {
            if let Some(payload) = routes::chat::stream_envelope(&event, session_id) {
                match serde_json::to_string(&payload) {
                    Ok(json) => {
                        if tx
                            .send(format!("event: chat\ndata: {json}\n\n").into_bytes())
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => {
                        if tx.send(b"event: error\ndata: {}\n\n".to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    let mut headers = cors_headers();
    headers.push(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream"[..])
            .expect("sse content-type header"),
    );
    headers.push(
        tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..])
            .expect("sse cache-control header"),
    );
    headers.push(
        tiny_http::Header::from_bytes(&b"Connection"[..], &b"keep-alive"[..])
            .expect("sse connection header"),
    );

    Ok(tiny_http::Response::new(
        tiny_http::StatusCode(200),
        headers,
        ChannelReader::new(rx),
        None,
        None,
    ))
}

pub(super) fn api_response_to_tiny(
    response: ApiResponse,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let mut tiny = tiny_http::Response::from_data(response.body)
        .with_status_code(response.status_code)
        .with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], response.content_type.as_bytes())
                .expect("content-type header"),
        );

    for h in response.extra_headers {
        tiny = tiny.with_header(h);
    }

    tiny
}
