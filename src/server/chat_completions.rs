//! Handler for `POST /v1/chat/completions`.
//!
//! Accepts an OpenAI Chat Completions request, converts it to a Copilot
//! Responses request, proxies to `{copilot_api_base_url}/responses`, then
//! converts the response back to the Chat Completions format.
//!
//! Streaming: Copilot Responses SSE events are translated on-the-fly into
//! `ChatCompletionChunk` SSE events.

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

use crate::copilot::responses::request::CopilotResponsesRequest;
use crate::copilot::responses::response::CopilotResponsesResponse;
use crate::copilot::responses::stream::{OutputItemAdded, StreamEvent, parse_sse_line};
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
        // Translate Copilot Responses SSE events → ChatCompletionChunk SSE events
        let byte_stream = upstream.bytes_stream();

        // We need to buffer partial lines. Use a string buffer carried across chunks.
        let translated_stream = futures_util::stream::unfold(
            (byte_stream, String::new(), model),
            |(mut stream, mut buf, model): (_, String, String)| async move {
                loop {
                    // Try to process any complete lines in the buffer first
                    if let Some(newline_pos) = buf.find('\n') {
                        let line = buf[..newline_pos].trim_end_matches('\r').to_string();
                        buf = buf[newline_pos + 1..].to_string();

                        if let Some(sse_data) =
                            translate_copilot_event_to_chat_chunk(&line, &model)
                        {
                            let bytes = Bytes::from(sse_data);
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
                            // Emit [DONE] and terminate
                            let done = Bytes::from("data: [DONE]\n\n");
                            return Some((Ok(done), (stream, buf, model)));
                        }
                        None => {
                            // Stream ended — flush remaining buffer as one last attempt
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
                AppError::InternalServerError(format!("Failed to parse upstream response: {}", e))
            })?;

        let chat_response: ChatCompletionsResponse = copilot_response.into();

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

        Ok((StatusCode::OK, headers, Json(chat_response)).into_response())
    }
}

// ---------------------------------------------------------------------------
// Copilot Responses SSE → ChatCompletionChunk SSE translation
// ---------------------------------------------------------------------------

/// Translate a single SSE line from the Copilot Responses stream into a
/// `ChatCompletionChunk` SSE line, or `None` if the line should be skipped.
///
/// Returns a formatted `data: <json>\n\n` string, or `data: [DONE]\n\n`
/// for `response.completed` / `response.incomplete`, or `None` to skip.
fn translate_copilot_event_to_chat_chunk(line: &str, model: &str) -> Option<String> {
    // Pass through [DONE] lines unchanged
    if line.trim() == "data: [DONE]" {
        return Some("data: [DONE]\n\n".to_string());
    }

    let event = match parse_sse_line(line) {
        Some(Ok(e)) => e,
        Some(Err(e)) => {
            // Unknown event type — log and skip
            error!("Failed to parse Copilot SSE event: {} | line: {}", e, line);
            return None;
        }
        None => return None, // non-data line or [DONE] already handled
    };

    let chunk = match event {
        StreamEvent::ResponseCreated { response } => {
            // Emit an initial chunk with role=assistant, no content
            ChatCompletionChunk {
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
            }
        }

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

        // When a function call output item is first added, emit the tool call
        // header (id + name) as a delta
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
            // Emit a final chunk with finish_reason=stop and usage, then [DONE]
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
        | StreamEvent::OutputTextAnnotationAdded { .. }
        | StreamEvent::ReasoningSummaryPartAdded { .. }
        | StreamEvent::ImageGenerationPartialImage { .. }
        | StreamEvent::CodeInterpreterCallCodeDelta { .. }
        | StreamEvent::CodeInterpreterCallCodeDone { .. }
        | StreamEvent::Error { .. } => return None,
    };

    let json = serde_json::to_string(&chunk).ok()?;
    Some(format!("data: {}\n\n", json))
}
