//! HTTP API server for Hivemind.
//!
//! This module provides a lightweight HTTP server for UI integration and
//! programmatic access to Hivemind's capabilities. The server exposes
//! a REST API for querying state and triggering actions.
//!
//! # Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | GET | `/api/projects` | List all projects |
//! | GET | `/api/projects/{id}` | Get project details |
//! | POST | `/api/projects` | Create a new project |
//! | GET | `/api/tasks` | List all tasks |
//! | GET | `/api/graphs` | List all task graphs |
//! | GET | `/api/flows` | List all task flows |
//! | GET | `/api/events` | Query events |
//! | GET | `/api/ui-state` | Get full UI state |
//!
//! # Configuration
//!
//! The server can be configured via [`ServeConfig`]:
//!
//! ```no_run
//! use hivemind::server::ServeConfig;
//!
//! let config = ServeConfig {
//!     host: "127.0.0.1".to_string(),
//!     port: 8787,
//!     events_limit: 200,
//! };
//! ```
//!
//! # Example
//!
//! Start the server via CLI:
//!
//! ```bash,no_run
//! hivemind serve --port 8787
//! ```
//!
//! Or programmatically:
//!
//! ```no_run
//! use hivemind::server::{serve, ServeConfig};
//!
//! let config = ServeConfig::default();
//! serve(&config)?;
//! # Ok::<(), hivemind::core::error::HivemindError>(())
//! ```
//!
//! # Response Format
//!
//! All API responses are JSON. The [`ApiResponse`] type provides:
//! - Status code
//! - Content type
//! - Body (JSON or text)
//! - Optional custom headers

use crate::cli::output::CliResponse;
use crate::core::error::{HivemindError, Result};
use crate::core::events::{CorrelationIds, Event, EventPayload, RuntimeRole};
use crate::core::flow::{RetryMode, RunMode};
use crate::core::registry::{MergeExecuteMode, MergeExecuteOptions, Registry};
use crate::core::state::{MergeState, Project, Task};
use crate::core::verification::CheckConfig;
use crate::core::{flow::TaskFlow, graph::TaskGraph};
use crate::storage::event_store::EventFilter;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::mpsc::{self, Receiver};
use uuid::Uuid;

mod api_types;
mod event_ui;
mod http_parse;
mod query_views;
mod routes;
#[cfg(test)]
mod tests;
use api_types::*;
use event_ui::*;
use http_parse::*;
use query_views::*;
use routes::*;

#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: String,
    pub port: u16,
    pub events_limit: usize,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8787,
            events_limit: 200,
        }
    }
}

pub fn handle_api_request(
    method: ApiMethod,
    url: &str,
    default_events_limit: usize,
    body: Option<&[u8]>,
) -> Result<ApiResponse> {
    let registry = Registry::open()?;
    handle_api_request_inner(method, url, default_events_limit, body, &registry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiMethod {
    Get,
    Post,
    Options,
}

impl ApiMethod {
    fn from_http(method: &tiny_http::Method) -> Option<Self> {
        match method {
            tiny_http::Method::Get => Some(Self::Get),
            tiny_http::Method::Post => Some(Self::Post),
            tiny_http::Method::Options => Some(Self::Options),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status_code: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub extra_headers: Vec<tiny_http::Header>,
}

impl ApiResponse {
    fn json<T: Serialize>(status_code: u16, value: &T) -> Result<Self> {
        let body = serde_json::to_vec_pretty(value).map_err(|e| {
            HivemindError::system("json_serialize_failed", e.to_string(), "server:json")
        })?;
        Ok(Self {
            status_code,
            content_type: "application/json",
            body,
            extra_headers: Vec::new(),
        })
    }

    fn text(status_code: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status_code,
            content_type,
            body: body.into(),
            extra_headers: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UiState {
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
    pub graphs: Vec<TaskGraph>,
    pub flows: Vec<TaskFlow>,
    pub merge_states: Vec<MergeState>,
    pub events: Vec<UiEvent>,
}

#[derive(Debug, Serialize)]
pub struct UiEvent {
    pub id: String,
    pub r#type: String,
    pub category: String,
    pub timestamp: DateTime<Utc>,
    pub sequence: Option<u64>,
    pub correlation: CorrelationIds,
    pub payload: HashMap<String, Value>,
}

struct ChannelReader {
    rx: Receiver<Vec<u8>>,
    current: std::io::Cursor<Vec<u8>>,
}

impl ChannelReader {
    fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            current: std::io::Cursor::new(Vec::new()),
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let pos = usize::try_from(self.current.position()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "cursor position overflow")
            })?;
            let backing = self.current.get_ref();
            if pos < backing.len() {
                return self.current.read(buf);
            }

            match self.rx.recv() {
                Ok(chunk) => self.current = std::io::Cursor::new(chunk),
                Err(_) => return Ok(0),
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn serve(config: &ServeConfig) -> Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| HivemindError::system("server_bind_failed", e.to_string(), "server:serve"))?;

    eprintln!("hivemind serve listening on http://{addr}");

    for mut req in server.incoming_requests() {
        let url = req.url().to_string();
        let Some(method) = ApiMethod::from_http(req.method()) else {
            let _ = req.respond(tiny_http::Response::empty(405));
            continue;
        };

        if method == ApiMethod::Get && url.starts_with("/api/runtime-stream/stream") {
            match Registry::open()
                .and_then(|registry| build_runtime_stream_sse_response(&url, &registry))
            {
                Ok(response) => {
                    let _ = req.respond(response);
                }
                Err(e) => {
                    let wrapped = CliResponse::<()>::error(&e);
                    let mut response = ApiResponse::json(500, &wrapped).unwrap_or_else(|_| {
                        ApiResponse::text(500, "text/plain", "internal error\n")
                    });
                    response.extra_headers.extend(cors_headers());
                    let _ = req.respond(api_response_to_tiny(response));
                }
            }
            continue;
        }

        if method == ApiMethod::Get && url.starts_with("/api/chat/sessions/stream") {
            match Registry::open()
                .and_then(|registry| build_chat_stream_sse_response(&url, &registry))
            {
                Ok(response) => {
                    let _ = req.respond(response);
                }
                Err(e) => {
                    let wrapped = CliResponse::<()>::error(&e);
                    let mut response = ApiResponse::json(500, &wrapped).unwrap_or_else(|_| {
                        ApiResponse::text(500, "text/plain", "internal error\n")
                    });
                    response.extra_headers.extend(cors_headers());
                    let _ = req.respond(api_response_to_tiny(response));
                }
            }
            continue;
        }

        let mut request_body = Vec::new();
        if method == ApiMethod::Post {
            let _ = req.as_reader().read_to_end(&mut request_body);
        }

        let response = match handle_api_request(
            method,
            &url,
            config.events_limit,
            if request_body.is_empty() {
                None
            } else {
                Some(request_body.as_slice())
            },
        ) {
            Ok(r) => r,
            Err(e) => {
                let wrapped = CliResponse::<()>::error(&e);
                match ApiResponse::json(500, &wrapped) {
                    Ok(mut r) => {
                        r.extra_headers.extend(cors_headers());
                        r
                    }
                    Err(_) => ApiResponse::text(500, "text/plain", "internal error\n"),
                }
            }
        };

        let mut tiny = tiny_http::Response::from_data(response.body)
            .with_status_code(response.status_code)
            .with_header(
                tiny_http::Header::from_bytes(
                    &b"Content-Type"[..],
                    response.content_type.as_bytes(),
                )
                .expect("content-type header"),
            );

        for h in response.extra_headers {
            tiny = tiny.with_header(h);
        }

        let _ = req.respond(tiny);
    }

    Ok(())
}

fn build_runtime_stream_sse_response(
    url: &str,
    registry: &Registry,
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
    let rx_events = registry.stream_events(&filter)?;
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

fn build_chat_stream_sse_response(
    url: &str,
    registry: &Registry,
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

    let rx_events = registry.stream_events(&EventFilter::all())?;
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

fn api_response_to_tiny(response: ApiResponse) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
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
