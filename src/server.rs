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
//! ```ignore
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
//! ```ignore
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

use crate::app::AppContext;
use crate::cli::output::CliResponse;
use crate::core::error::{HivemindError, Result};
use crate::core::events::{CorrelationIds, Event, EventPayload, RuntimeRole};
use crate::core::flow::{RetryMode, RunMode};
use crate::core::registry::{MergeExecuteMode, MergeExecuteOptions};
use crate::core::state::{MergeState, Project, Task};
use crate::core::verification::CheckConfig;
use crate::core::{
    flow::TaskFlow,
    graph::TaskGraph,
    workflow::{WorkflowDefinition, WorkflowRun},
};
use crate::storage::event_store::EventFilter;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::mpsc::Receiver;
use uuid::Uuid;

mod api_types;
mod event_ui;
mod http_parse;
mod listener;
mod query_views;
mod routes;
#[cfg(test)]
mod tests;
mod transport;
use api_types::*;
use event_ui::*;
use http_parse::*;
use listener::process::process_request;
use query_views::*;
use routes::*;
use transport::*;

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
    let app = AppContext::default();
    handle_api_request_inner(&app, method, url, default_events_limit, body)
}

fn handle_api_request_with_app(
    app: &AppContext,
    method: ApiMethod,
    url: &str,
    default_events_limit: usize,
    body: Option<&[u8]>,
) -> Result<ApiResponse> {
    handle_api_request_inner(app, method, url, default_events_limit, body)
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
    pub workflows: Vec<WorkflowDefinition>,
    pub workflow_runs: Vec<WorkflowRun>,
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

pub fn serve(config: &ServeConfig) -> Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| HivemindError::system("server_bind_failed", e.to_string(), "server:serve"))?;
    let app = AppContext::default();

    eprintln!("hivemind serve listening on http://{addr}");

    for req in server.incoming_requests() {
        process_request(req, &app, config.events_limit);
    }

    Ok(())
}
