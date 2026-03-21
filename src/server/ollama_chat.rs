//! Handler for `POST /api/chat`.
//!
//! Accepts an Ollama `/api/chat` request, converts it to a Copilot Responses
//! request, proxies to `{copilot_api_base_url}/responses`, then converts the
//! response back to Ollama `/api/chat` format.
//!
//! Streaming: Copilot Responses SSE events are translated on-the-fly into
//! Ollama NDJSON chunks (one JSON object per line, `\n`-terminated).

use axum::{
    Json,
    body::Body,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use futures_util::StreamExt;
use std::sync::Arc;
use tracing::error;

use crate::copilot::responses::request::CopilotResponsesRequest;
use crate::copilot::responses::response::CopilotResponsesResponse;
use crate::copilot::responses::stream::{OutputItemAdded, StreamEvent, parse_sse_line};
use crate::ollama::chat::request::{OllamaMessage, OllamaRole, OllamaChatRequest};
use crate::ollama::chat::response::OllamaChatResponse;

use super::{AppError, AppState, Server};

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<OllamaChatRequest>,
) -> Result<Response, AppError> {
    let token = Server::get_token(state.clone()).await?;

    // Ollama defaults stream to true
    let is_streaming = request.stream.unwrap_or(true);
    let model = request.model.clone();
    let mut copilot_request: CopilotResponsesRequest = request.into();
    // Ensure the upstream request reflects the resolved streaming decision,
    // since the Ollama default (true) differs from Copilot's default (false).
    copilot_request.stream = Some(is_streaming);
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

    if is_streaming {
        // Translate Copilot Responses SSE events → Ollama NDJSON chunks
        let byte_stream = upstream.bytes_stream();

        let translated_stream = futures_util::stream::unfold(
            (byte_stream, String::new(), model),
            |(mut stream, mut buf, model): (_, String, String)| async move {
                loop {
                    // Try to process any complete lines in the buffer first
                    if let Some(newline_pos) = buf.find('\n') {
                        let line = buf[..newline_pos].trim_end_matches('\r').to_string();
                        buf = buf[newline_pos + 1..].to_string();

                        if let Some(ndjson) =
                            translate_copilot_event_to_ollama_chunk(&line, &model)
                        {
                            let bytes = Bytes::from(ndjson);
                            return Some((Ok::<_, std::convert::Infallible>(bytes), (stream, buf, model)));
                        }
                        // Line produced no output — continue to next line
                        continue;
                    }

                    // No complete line in buffer; fetch more bytes
                    match stream.next().await {
                        Some(Ok(chunk)) => {
                            buf.push_str(&String::from_utf8_lossy(&chunk));
                        }
                        Some(Err(e)) => {
                            error!("Stream error: {}", e);
                            return None;
                        }
                        None => {
                            // Stream ended — flush remaining buffer
                            if !buf.is_empty() {
                                let line = buf.trim().to_string();
                                buf.clear();
                                if let Some(ndjson) =
                                    translate_copilot_event_to_ollama_chunk(&line, &model)
                                {
                                    let bytes = Bytes::from(ndjson);
                                    return Some((Ok(bytes), (stream, buf, model)));
                                }
                            }
                            return None;
                        }
                    }
                }
            },
        );

        let body = Body::from_stream(translated_stream);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/x-ndjson")
            .header(header::CACHE_CONTROL, "no-cache")
            .header("X-Accel-Buffering", "no")
            .body(body)
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        Ok(response)
    } else {
        let bytes = upstream.bytes().await.map_err(|e| {
            error!("Failed to read upstream body: {}", e);
            AppError::InternalServerError(format!("Failed to read upstream body: {}", e))
        })?;

        let copilot_response: CopilotResponsesResponse =
            serde_json::from_slice(&bytes).map_err(|e| {
                error!("Failed to parse upstream response: {}", e);
                AppError::InternalServerError(format!("Failed to parse upstream response: {}", e))
            })?;

        let ollama_response: OllamaChatResponse = copilot_response.into();

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

        Ok((StatusCode::OK, headers, Json(ollama_response)).into_response())
    }
}

// ---------------------------------------------------------------------------
// Copilot Responses SSE → Ollama NDJSON translation
// ---------------------------------------------------------------------------

/// Translate a single SSE line from the Copilot Responses stream into an
/// Ollama NDJSON line (`<json>\n`), or `None` if the line should be skipped.
fn translate_copilot_event_to_ollama_chunk(line: &str, model: &str) -> Option<String> {
    let event = match parse_sse_line(line) {
        Some(Ok(e)) => e,
        Some(Err(e)) => {
            error!("Failed to parse Copilot SSE event: {} | line: {}", e, line);
            return None;
        }
        None => return None,
    };

    let now = Utc::now().to_rfc3339();

    let chunk = match event {
        StreamEvent::ResponseCreated { .. } => {
            // Emit an initial chunk with role=assistant, empty content
            OllamaChatResponse {
                model: model.to_string(),
                created_at: now,
                message: OllamaMessage {
                    role: OllamaRole::Assistant,
                    content: String::new(),
                    images: None,
                    thinking: None,
                    tool_calls: None,
                    tool_name: None,
                },
                done: false,
                done_reason: None,
                total_duration: None,
                load_duration: None,
                prompt_eval_count: None,
                prompt_eval_duration: None,
                eval_count: None,
                eval_duration: None,
            }
        }

        StreamEvent::OutputTextDelta { delta, .. } => OllamaChatResponse {
            model: model.to_string(),
            created_at: now,
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: delta,
                images: None,
                thinking: None,
                tool_calls: None,
                tool_name: None,
            },
            done: false,
            done_reason: None,
            total_duration: None,
            load_duration: None,
            prompt_eval_count: None,
            prompt_eval_duration: None,
            eval_count: None,
            eval_duration: None,
        },

        StreamEvent::ReasoningSummaryTextDelta { delta, .. } => OllamaChatResponse {
            model: model.to_string(),
            created_at: now,
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: String::new(),
                images: None,
                thinking: Some(delta),
                tool_calls: None,
                tool_name: None,
            },
            done: false,
            done_reason: None,
            total_duration: None,
            load_duration: None,
            prompt_eval_count: None,
            prompt_eval_duration: None,
            eval_count: None,
            eval_duration: None,
        },

        StreamEvent::ResponseCompleted { response } => OllamaChatResponse {
            model: model.to_string(),
            created_at: now,
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: String::new(),
                images: None,
                thinking: None,
                tool_calls: None,
                tool_name: None,
            },
            done: true,
            done_reason: Some("stop".to_string()),
            total_duration: None,
            load_duration: None,
            prompt_eval_count: Some(response.usage.input_tokens),
            prompt_eval_duration: None,
            eval_count: Some(response.usage.output_tokens),
            eval_duration: None,
        },

        StreamEvent::ResponseIncomplete { response } => OllamaChatResponse {
            model: model.to_string(),
            created_at: now,
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: String::new(),
                images: None,
                thinking: None,
                tool_calls: None,
                tool_name: None,
            },
            done: true,
            done_reason: Some("length".to_string()),
            total_duration: None,
            load_duration: None,
            prompt_eval_count: Some(response.usage.input_tokens),
            prompt_eval_duration: None,
            eval_count: Some(response.usage.output_tokens),
            eval_duration: None,
        },

        // Skip these events — not representable in Ollama streaming wire format
        StreamEvent::FunctionCallArgumentsDelta { .. }
        | StreamEvent::OutputItemAdded {
            item: OutputItemAdded::FunctionCall { .. },
            ..
        }
        | StreamEvent::OutputItemAdded { .. }
        | StreamEvent::OutputItemDone { .. }
        | StreamEvent::OutputTextAnnotationAdded { .. }
        | StreamEvent::ReasoningSummaryPartAdded { .. }
        | StreamEvent::ImageGenerationPartialImage { .. }
        | StreamEvent::CodeInterpreterCallCodeDelta { .. }
        | StreamEvent::CodeInterpreterCallCodeDone { .. }
        | StreamEvent::Error { .. } => return None,
    };

    let json = serde_json::to_string(&chunk).ok()?;
    Some(format!("{}\n", json))
}
