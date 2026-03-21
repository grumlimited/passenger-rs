//! Handler for `POST /v1/chat/completions`.
//!
//! Routes the request to either Copilot `/responses` or `/chat/completions`
//! based on the model ID (via `should_use_responses_api`).
//!
//! **`/responses` path** (gpt-5+, non-mini):
//!   Converts to `CopilotResponsesRequest`, proxies to `/responses`,
//!   translates SSE events → `ChatCompletionChunk` SSE (streaming) or
//!   `CopilotResponsesResponse` → `ChatCompletionsResponse` (non-streaming).
//!
//! **`/chat/completions` path** (everything else):
//!   Sends `ChatCompletionsRequest` directly to `/chat/completions`.
//!   - Streaming: upstream SSE is already in the correct format — pass through.
//!   - Non-streaming: parse `ChatCompletionsResponse` and return as-is.

use axum::{
    Json,
    body::Body,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use std::sync::Arc;
use tracing::error;

use crate::copilot::should_use_responses_api;
use crate::copilot::responses::request::CopilotResponsesRequest;
use crate::copilot::responses::response::CopilotResponsesResponse;
use crate::copilot::responses::stream::{OutputItemAdded, ParsedSseEvent, StreamEvent, parse_sse_line};
use crate::openai::chat_completions::request::ChatCompletionsRequest;
use crate::openai::chat_completions::response::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatDelta, ChatUsage, ToolCallDelta,
    ToolCallFunctionDelta,
};
use crate::openai::chat_completions::response::ChatCompletionsResponse;

use super::{AppError, AppState, Server};

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatCompletionsRequest>,
) -> Result<Response, AppError> {
    let token = Server::get_token(state.clone()).await?;

    let is_streaming = request.stream.unwrap_or(false);
    let model = request.model.clone();

    if should_use_responses_api(&model) {
        // --- /responses path ---
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

        if is_streaming {
            let byte_stream = upstream.bytes_stream();

            let translated_stream = futures_util::stream::unfold(
                (byte_stream, String::new(), model),
                |(mut stream, mut buf, model): (_, String, String)| async move {
                    loop {
                        if let Some(newline_pos) = buf.find('\n') {
                            let line = buf[..newline_pos].trim_end_matches('\r').to_string();
                            buf = buf[newline_pos + 1..].to_string();

                            if let Some(sse_data) =
                                translate_copilot_event_to_chat_chunk(&line, &model)
                            {
                                let bytes = Bytes::from(sse_data);
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
                                let done = Bytes::from("data: [DONE]\n\n");
                                return Some((Ok(done), (stream, buf, model)));
                            }
                            None => {
                                if !buf.is_empty() {
                                    let line = buf.trim().to_string();
                                    buf.clear();
                                    if let Some(sse_data) =
                                        translate_copilot_event_to_chat_chunk(&line, &model)
                                    {
                                        let bytes = Bytes::from(sse_data);
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
                .header(header::CONTENT_TYPE, "text/event-stream")
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

            let chat_response: ChatCompletionsResponse = copilot_response.into();

            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

            Ok((StatusCode::OK, headers, Json(chat_response)).into_response())
        }
    } else {
        // --- /chat/completions path ---
        let url = format!("{}/chat/completions", state.config.copilot.api_base_url);

        let upstream = state
            .client
            .post(&url)
            .bearer_auth(&token.token)
            .header("Copilot-Integration-Id", "vscode-chat")
            .json(&request)
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
            // Upstream SSE is already valid OpenAI SSE — pass bytes through directly.
            let safe_stream = futures_util::stream::unfold(
                upstream.bytes_stream(),
                |mut stream| async move {
                    match stream.next().await {
                        Some(Ok(bytes)) => {
                            Some((Ok::<_, std::convert::Infallible>(bytes), stream))
                        }
                        Some(Err(e)) => {
                            error!("Stream error: {}", e);
                            None
                        }
                        None => None,
                    }
                },
            );
            let body = Body::from_stream(safe_stream);
            let response = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
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

            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

            Ok((StatusCode::OK, headers, Json(chat_response)).into_response())
        }
    }
}

// ---------------------------------------------------------------------------
// Copilot Responses SSE → ChatCompletionChunk SSE translation
// ---------------------------------------------------------------------------

/// Translate a single SSE line from the Copilot Responses stream into a
/// `ChatCompletionChunk` SSE line, or `None` if the line should be skipped.
fn translate_copilot_event_to_chat_chunk(line: &str, model: &str) -> Option<String> {
    if line.trim() == "data: [DONE]" {
        return Some("data: [DONE]\n\n".to_string());
    }

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

    let chunk = match event {
        StreamEvent::ResponseCreated { response } => ChatCompletionChunk {
            id: Some(response.id),
            created: Some(response.created_at),
            model: Some(response.model),
            choices: vec![ChatCompletionChunkChoice {
                index: 0,
                delta: Some(ChatDelta {
                    role: Some("assistant".to_string()),
                    content: None,
                    tool_calls: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        },

        StreamEvent::OutputTextDelta { delta, .. } => ChatCompletionChunk {
            id: None,
            created: None,
            model: Some(model.to_string()),
            choices: vec![ChatCompletionChunkChoice {
                index: 0,
                delta: Some(ChatDelta {
                    role: None,
                    content: Some(delta),
                    tool_calls: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        },

        StreamEvent::FunctionCallArgumentsDelta {
            output_index,
            delta,
            ..
        } => ChatCompletionChunk {
            id: None,
            created: None,
            model: Some(model.to_string()),
            choices: vec![ChatCompletionChunkChoice {
                index: 0,
                delta: Some(ChatDelta {
                    role: None,
                    content: None,
                    tool_calls: Some(vec![ToolCallDelta {
                        index: output_index,
                        id: None,
                        function: Some(ToolCallFunctionDelta {
                            name: None,
                            arguments: Some(delta),
                        }),
                    }]),
                    reasoning_text: None,
                    reasoning_opaque: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        },

        StreamEvent::OutputItemAdded {
            output_index,
            item: OutputItemAdded::FunctionCall { id: _, call_id, name, .. },
        } => ChatCompletionChunk {
            id: None,
            created: None,
            model: Some(model.to_string()),
            choices: vec![ChatCompletionChunkChoice {
                index: 0,
                delta: Some(ChatDelta {
                    role: None,
                    content: None,
                    tool_calls: Some(vec![ToolCallDelta {
                        index: output_index,
                        id: Some(call_id),
                        function: Some(ToolCallFunctionDelta {
                            name: Some(name),
                            arguments: Some(String::new()),
                        }),
                    }]),
                    reasoning_text: None,
                    reasoning_opaque: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        },

        StreamEvent::ReasoningSummaryTextDelta { delta, .. } => ChatCompletionChunk {
            id: None,
            created: None,
            model: Some(model.to_string()),
            choices: vec![ChatCompletionChunkChoice {
                index: 0,
                delta: Some(ChatDelta {
                    role: None,
                    content: None,
                    tool_calls: None,
                    reasoning_text: Some(delta),
                    reasoning_opaque: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        },

        StreamEvent::ResponseCompleted { response } => {
            let usage = ChatUsage {
                prompt_tokens: Some(response.usage.input_tokens),
                completion_tokens: Some(response.usage.output_tokens),
                total_tokens: Some(
                    response.usage.input_tokens + response.usage.output_tokens,
                ),
                prompt_tokens_details: None,
                completion_tokens_details: None,
            };
            let finish_chunk = ChatCompletionChunk {
                id: None,
                created: None,
                model: Some(model.to_string()),
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: Some(ChatDelta {
                        role: None,
                        content: None,
                        tool_calls: None,
                        reasoning_text: None,
                        reasoning_opaque: None,
                    }),
                    finish_reason: Some("stop".to_string()),
                }],
                usage: Some(usage),
            };
            let json = serde_json::to_string(&finish_chunk).ok()?;
            return Some(format!("data: {}\n\ndata: [DONE]\n\n", json));
        }

        StreamEvent::ResponseIncomplete { response } => {
            let usage = ChatUsage {
                prompt_tokens: Some(response.usage.input_tokens),
                completion_tokens: Some(response.usage.output_tokens),
                total_tokens: Some(
                    response.usage.input_tokens + response.usage.output_tokens,
                ),
                prompt_tokens_details: None,
                completion_tokens_details: None,
            };
            let finish_chunk = ChatCompletionChunk {
                id: None,
                created: None,
                model: Some(model.to_string()),
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: Some(ChatDelta {
                        role: None,
                        content: None,
                        tool_calls: None,
                        reasoning_text: None,
                        reasoning_opaque: None,
                    }),
                    finish_reason: Some("length".to_string()),
                }],
                usage: Some(usage),
            };
            let json = serde_json::to_string(&finish_chunk).ok()?;
            return Some(format!("data: {}\n\ndata: [DONE]\n\n", json));
        }

        StreamEvent::OutputItemDone { .. }
        | StreamEvent::OutputItemAdded { .. }
        | StreamEvent::OutputTextDone { .. }
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
    Some(format!("data: {}\n\n", json))
}
