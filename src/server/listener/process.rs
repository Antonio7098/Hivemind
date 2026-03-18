use crate::app::AppContext;
use crate::cli::output::CliResponse;
use crate::server::{
    api_response_to_tiny, build_chat_stream_sse_response, build_runtime_stream_sse_response,
    cors_headers, handle_api_request_with_app, ApiMethod, ApiResponse,
};

pub(crate) fn process_request(mut req: tiny_http::Request, app: &AppContext, events_limit: usize) {
    let url = req.url().to_string();
    let Some(method) = ApiMethod::from_http(req.method()) else {
        let _ = req.respond(tiny_http::Response::empty(405));
        return;
    };

    if method == ApiMethod::Get && url.starts_with("/api/runtime-stream/stream") {
        match app
            .event_service()
            .and_then(|service| build_runtime_stream_sse_response(&url, &service))
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
        return;
    }

    if method == ApiMethod::Get && url.starts_with("/api/chat/sessions/stream") {
        match app
            .event_service()
            .and_then(|service| build_chat_stream_sse_response(&url, &service))
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
        return;
    }

    let mut request_body = Vec::new();
    if method == ApiMethod::Post {
        let _ = req.as_reader().read_to_end(&mut request_body);
    }

    let response = match handle_api_request_with_app(
        app,
        method,
        &url,
        events_limit,
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
