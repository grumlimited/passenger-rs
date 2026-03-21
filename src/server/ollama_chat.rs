//! Handler for `POST /api/chat`.
//!
//! Routes the request to either Copilot `/responses` or `/chat/completions`
//! based on the model ID (via `should_use_responses_api`).
//!
//! **`/responses` path** (gpt-5+, non-mini):
//!   Converts to `CopilotResponsesRequest`, proxies to `/responses`,
//!   translates SSE events → Ollama NDJSON (streaming) or
//!   `CopilotResponsesResponse` → `OllamaChatResponse` (non-streaming).
//!
//! **`/chat/completions` path** (everything else):
//!   Converts to `ChatCompletionsRequest`, proxies to `/chat/completions`.
//!   - Streaming: parse `ChatCompletionChunk` SSE → Ollama NDJSON on-the-fly.
//!   - Non-streaming: parse `ChatCompletionsResponse` → `OllamaChatResponse`.

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

use crate::copilot::should_use_responses_api;
use crate::copilot::responses::request::CopilotResponsesRequest;
use crate::copilot::responses::response::CopilotResponsesResponse;
use crate::copilot::responses::stream::{OutputItemAdded, ParsedSseEvent, StreamEvent, parse_sse_line};
use crate::openai::chat_completions::request::ChatCompletionsRequest;
use crate::openai::chat_completions::response::{ChatCompletionChunk, ChatCompletionsResponse};
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

    if should_use_responses_api(&model) {
        // --- /responses path ---
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
            let byte_stream = upstream.bytes_stream();

            let translated_stream = futures_util::stream::unfold(
                (byte_stream, String::new(), model),
                |(mut stream, mut buf, model): (_, String, String)| async move {
                    loop {
                        if let Some(newline_pos) = buf.find('\n') {
                            let line = buf[..newline_pos].trim_end_matches('\r').to_string();
                            buf = buf[newline_pos + 1..].to_string();

                            if let Some(ndjson) =
                                translate_copilot_event_to_ollama_chunk(&line, &model)
                            {
                                let bytes = Bytes::from(ndjson);
                                return Some((Ok::<_, std::convert::Infallible>(bytes), (stream, buf, model)));
                            }
                            continue;
                        }

                        match stream.next().await {
                            Some(Ok(chunk)) => {
                                buf.push_str(&String::from_utf8_lossy(&chunk));
                            }
                            Some(Err(e)) => {
                                error!("Stream error: {}", e);
                                return None;
                            }
                            None => {
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
                    AppError::InternalServerError(format!(
                        "Failed to parse upstream response: {}",
                        e
                    ))
                })?;

            let ollama_response: OllamaChatResponse = copilot_response.into();

            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

            Ok((StatusCode::OK, headers, Json(ollama_response)).into_response())
        }
    } else {
        // --- /chat/completions path ---
        let mut chat_request: ChatCompletionsRequest = request.into();
        // Ensure the upstream request reflects the resolved streaming decision
        chat_request.stream = Some(is_streaming);
        let url = format!("{}/chat/completions", state.config.copilot.api_base_url);

        let upstream = state
            .client
            .post(&url)
            .bearer_auth(&token.token)
            .header("Copilot-Integration-Id", "vscode-chat")
            .json(&chat_request)
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
            // Translate OpenAI SSE chunks → Ollama NDJSON
            let byte_stream = upstream.bytes_stream();

            let translated_stream = futures_util::stream::unfold(
                (byte_stream, String::new(), model),
                |(mut stream, mut buf, model): (_, String, String)| async move {
                    loop {
                        if let Some(newline_pos) = buf.find('\n') {
                            let line = buf[..newline_pos].trim_end_matches('\r').to_string();
                            buf = buf[newline_pos + 1..].to_string();

                            if let Some(ndjson) =
                                translate_chat_chunk_to_ollama_ndjson(&line, &model)
                            {
                                let bytes = Bytes::from(ndjson);
                                return Some((Ok::<_, std::convert::Infallible>(bytes), (stream, buf, model)));
                            }
                            continue;
                        }

                        match stream.next().await {
                            Some(Ok(chunk)) => {
                                buf.push_str(&String::from_utf8_lossy(&chunk));
                            }
                            Some(Err(e)) => {
                                error!("Stream error: {}", e);
                                return None;
                            }
                            None => {
                                if !buf.is_empty() {
                                    let line = buf.trim().to_string();
                                    buf.clear();
                                    if let Some(ndjson) =
                                        translate_chat_chunk_to_ollama_ndjson(&line, &model)
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

            let chat_response: ChatCompletionsResponse =
                serde_json::from_slice(&bytes).map_err(|e| {
                    error!("Failed to parse upstream response: {}", e);
                    AppError::InternalServerError(format!(
                        "Failed to parse upstream response: {}",
                        e
                    ))
                })?;

            let ollama_response: OllamaChatResponse = chat_response.into();

            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

            Ok((StatusCode::OK, headers, Json(ollama_response)).into_response())
        }
    }
}

// ---------------------------------------------------------------------------
// Copilot Responses SSE → Ollama NDJSON translation
// ---------------------------------------------------------------------------

/// Translate a single SSE line from the Copilot Responses stream into an
/// Ollama NDJSON line (`<json>\n`), or `None` if the line should be skipped.
fn translate_copilot_event_to_ollama_chunk(line: &str, model: &str) -> Option<String> {
    let event = match parse_sse_line(line) {
        Some(Ok(ParsedSseEvent::Known(e))) => e,
        Some(Ok(ParsedSseEvent::Unknown(t))) => {
            tracing::warn!("Skipping unknown Copilot SSE event type: {}", t);
            return None;
        }
        Some(Err(e)) => {
            error!("Failed to parse Copilot SSE event: {} | line: {}", e, line);
            return None;
        }
        None => return None,
    };

    let now = Utc::now().to_rfc3339();

    let chunk = match event {
        StreamEvent::ResponseCreated { .. } => OllamaChatResponse {
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
        },

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
        | StreamEvent::ContentPartAdded { .. }
        | StreamEvent::ContentPartDone { .. }
        | StreamEvent::ResponseInProgress { .. }
        | StreamEvent::Error { .. } => return None,
    };

    let json = serde_json::to_string(&chunk).ok()?;
    Some(format!("{}\n", json))
}

// ---------------------------------------------------------------------------
// OpenAI ChatCompletionChunk SSE → Ollama NDJSON translation
// ---------------------------------------------------------------------------

/// Translate a single SSE line from an OpenAI `/chat/completions` stream into
/// an Ollama NDJSON line, or `None` if the line should be skipped.
///
/// - Text delta → `{ done: false, message: { content: delta } }`
/// - Finish chunk (finish_reason set) → `{ done: true, done_reason, usage }`
/// - `data: [DONE]` → `None` (stream terminator, nothing to emit)
/// - Tool call deltas → skipped (not streamable in Ollama format)
fn translate_chat_chunk_to_ollama_ndjson(line: &str, model: &str) -> Option<String> {
    // Strip the "data: " prefix
    let data = line.strip_prefix("data: ")?;

    // Skip the stream terminator
    if data.trim() == "[DONE]" {
        return None;
    }

    let chunk: ChatCompletionChunk = serde_json::from_str(data).ok()?;

    let now = Utc::now().to_rfc3339();

    // Use the model from the chunk if present, else fall back to the request model.
    let chunk_model = chunk.model.as_deref().unwrap_or(model).to_string();

    // Inspect the first choice
    let choice = chunk.choices.into_iter().next();

    let ollama_chunk = if let Some(choice) = choice {
        let finish_reason = choice.finish_reason.clone();
        let delta = choice.delta;

        if let Some(reason) = finish_reason {
            // Final chunk
            let (prompt_eval_count, eval_count) = chunk
                .usage
                .map(|u| (u.prompt_tokens, u.completion_tokens))
                .unwrap_or((None, None));

            OllamaChatResponse {
                model: chunk_model,
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
                done_reason: Some(reason),
                total_duration: None,
                load_duration: None,
                prompt_eval_count,
                prompt_eval_duration: None,
                eval_count,
                eval_duration: None,
            }
        } else if let Some(d) = delta {
            if let Some(content) = d.content {
                OllamaChatResponse {
                    model: chunk_model,
                    created_at: now,
                    message: OllamaMessage {
                        role: OllamaRole::Assistant,
                        content,
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
            } else {
                // Role-only or tool-call delta — skip
                return None;
            }
        } else {
            return None;
        }
    } else {
        // Usage-only chunk (no choices) — skip
        return None;
    };

    let json = serde_json::to_string(&ollama_chunk).ok()?;
    Some(format!("{}\n", json))
}
