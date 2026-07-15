//! Handler for `POST /v1/responses`.
//!
//! Accepts an OpenAI Responses API request, converts it to a Copilot
//! Responses request, proxies it to `{copilot_api_base_url}/responses`,
//! and streams the response back.
//!
//! Only streaming responses are supported. Requests with `stream: false`
//! (or no `stream` field) are rejected with 400 Bad Request.
//!
//! The Copilot Responses SSE format is identical to the OpenAI Responses SSE
//! format, so the stream is passed through byte-for-byte.

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::Response,
};
use std::sync::Arc;
use tracing::error;

use crate::copilot::responses::request::CopilotResponsesRequest;
use crate::openai::responses::request::OpenAIResponsesRequest;

use super::{AppError, AppState, Server};

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<OpenAIResponsesRequest>,
) -> Result<Response, AppError> {
    if request.stream != Some(true) {
        return Err(AppError::BadRequest(
            "Only streaming is supported. Set \"stream\": true in your request.".to_string(),
        ));
    }

    let token = Server::get_token(state.clone()).await?;

    let copilot_request: CopilotResponsesRequest = request.into();
    let url = format!("{}/responses", state.config.copilot.api_base_url);

    let upstream = state
        .client
        .post(&url)
        .bearer_auth(&token.token)
        .header("Copilot-Integration-Id", "vscode-chat")
        .json(&copilot_request)
        .send()
        .await
        .map_err(|e| {
            error!("Upstream request failed: {}", e);
            AppError::InternalServerError(format!("Upstream request failed: {}", e))
        })?;

    if !upstream.status().is_success() {
        let status = upstream.status();
        let body = upstream.text().await.unwrap_or_default();
        error!("Upstream returned {}: {}", status, body);
        return Err(AppError::InternalServerError(format!(
            "Upstream error {}: {}",
            status, body
        )));
    }

    // Pass-through: Copilot Responses SSE == OpenAI Responses SSE
    let stream = upstream.bytes_stream();
    let body = Body::from_stream(stream);
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(body)
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(response)
}
