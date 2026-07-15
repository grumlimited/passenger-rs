//! Handler for `POST /v1/chat/completions`.
//!
//! Routes the request to either Copilot `/responses` or `/chat/completions`
//! based on the model ID (via `should_use_responses_api`).
//!
//! Only streaming responses are supported. Requests with `stream: false`
//! (or no `stream` field) are rejected with 400 Bad Request.
//!
//! **`/responses` path** (gpt-5+, non-mini):
//!   Converts to `CopilotResponsesRequest`, proxies to `/responses`,
//!   translates SSE events → `ChatCompletionChunk` SSE.
//!
//!   The stream state tracks `id`, `created`, and `model` from the first
//!   `ResponseCreated` event and attaches them to every subsequent chunk,
//!   as required by the OpenAI spec.
//!
//!   Tool call indices are tracked independently from `output_index` via a
//!   map, since Copilot's `output_index` counts all output items (messages,
//!   function calls, etc.) while OpenAI's `tool_calls[].index` counts only
//!   tool calls.
//!
//! **`/chat/completions` path** (everything else):
//!   Sends `ChatCompletionsRequest` directly to `/chat/completions`.
//!   Upstream SSE is already in the correct OpenAI format — passed through.

use axum::{
    Json,
    body::Body,
    body::Bytes,
    extract::State,
    http::{StatusCode, header},
    response::Response,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::error;

use crate::copilot::responses::request::CopilotResponsesRequest;
use crate::copilot::responses::stream::{
    OutputItemAdded, ParsedSseEvent, StreamEvent, parse_sse_line,
};
use crate::copilot::should_use_responses_api;
use crate::openai::chat_completions::request::ChatCompletionsRequest;
use crate::openai::chat_completions::response::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatDelta, ChatUsage, ToolCallDelta,
    ToolCallFunctionDelta,
};

use super::{AppError, AppState, Server};

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatCompletionsRequest>,
) -> Result<Response, AppError> {
    if request.stream != Some(true) {
        return Err(AppError::BadRequest(
            "Only streaming is supported. Set \"stream\": true in your request.".to_string(),
        ));
    }

    let token = Server::get_token(state.clone()).await?;
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

        let byte_stream = upstream.bytes_stream();

        // Stream state:
        //   buf        — incomplete SSE line buffer
        //   model      — model name (from request, overwritten by ResponseCreated)
        //   id         — response id, set from ResponseCreated, carried on all chunks
        //   created    — created timestamp, set from ResponseCreated, carried on all chunks
        //   tool_index_map — maps output_index → tool_call_index (sequential per-stream counter)
        //   next_tool_index — next available tool_call_index
        type StreamState = (
            futures_util::stream::BoxStream<'static, Result<Bytes, reqwest::Error>>,
            String,            // buf
            String,            // model
            Option<String>,    // id
            Option<u64>,       // created
            HashMap<u32, u32>, // tool_index_map
            u32,               // next_tool_index
        );

        let initial_state: StreamState = (
            Box::pin(byte_stream),
            String::new(),
            model,
            None,
            None,
            HashMap::new(),
            0,
        );

        let translated_stream = futures_util::stream::unfold(
            initial_state,
            |(
                mut stream,
                mut buf,
                model,
                mut id,
                mut created,
                mut tool_index_map,
                mut next_tool_index,
            )| async move {
                loop {
                    if let Some(newline_pos) = buf.find('\n') {
                        let line = buf[..newline_pos].trim_end_matches('\r').to_string();
                        buf = buf[newline_pos + 1..].to_string();

                        if let Some(sse_data) = translate_copilot_event_to_chat_chunk(
                            &line,
                            &model,
                            &mut id,
                            &mut created,
                            &mut tool_index_map,
                            &mut next_tool_index,
                        ) {
                            let bytes = Bytes::from(sse_data);
                            return Some((
                                Ok::<_, std::convert::Infallible>(bytes),
                                (
                                    stream,
                                    buf,
                                    model,
                                    id,
                                    created,
                                    tool_index_map,
                                    next_tool_index,
                                ),
                            ));
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
                            return Some((
                                Ok(done),
                                (
                                    stream,
                                    buf,
                                    model,
                                    id,
                                    created,
                                    tool_index_map,
                                    next_tool_index,
                                ),
                            ));
                        }
                        None => {
                            if !buf.is_empty() {
                                let line = buf.trim().to_string();
                                buf.clear();
                                if let Some(sse_data) = translate_copilot_event_to_chat_chunk(
                                    &line,
                                    &model,
                                    &mut id,
                                    &mut created,
                                    &mut tool_index_map,
                                    &mut next_tool_index,
                                ) {
                                    let bytes = Bytes::from(sse_data);
                                    return Some((
                                        Ok(bytes),
                                        (
                                            stream,
                                            buf,
                                            model,
                                            id,
                                            created,
                                            tool_index_map,
                                            next_tool_index,
                                        ),
                                    ));
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

        // Upstream SSE is already valid OpenAI SSE — pass bytes through directly.
        let safe_stream =
            futures_util::stream::unfold(upstream.bytes_stream(), |mut stream| async move {
                match stream.next().await {
                    Some(Ok(bytes)) => Some((Ok::<_, std::convert::Infallible>(bytes), stream)),
                    Some(Err(e)) => {
                        error!("Stream error: {}", e);
                        None
                    }
                    None => None,
                }
            });
        let body = Body::from_stream(safe_stream);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header("X-Accel-Buffering", "no")
            .body(body)
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        Ok(response)
    }
}

// ---------------------------------------------------------------------------
// Copilot Responses SSE → ChatCompletionChunk SSE translation
// ---------------------------------------------------------------------------

const OBJECT_CHUNK: &str = "chat.completion.chunk";

/// Build a minimal chunk wrapper with the correct `object` field and
/// the stable `id`/`created` values for this response stream.
fn make_chunk(
    id: &Option<String>,
    created: &Option<u64>,
    model: &str,
    choices: Vec<ChatCompletionChunkChoice>,
    usage: Option<ChatUsage>,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        object: OBJECT_CHUNK.to_string(),
        id: id.clone(),
        created: *created,
        model: Some(model.to_string()),
        choices,
        usage,
    }
}

/// Translate a single SSE line from the Copilot Responses stream into a
/// `ChatCompletionChunk` SSE line, or `None` if the line should be skipped.
///
/// Mutates `id`, `created`, `tool_index_map`, and `next_tool_index` as
/// new events establish or reference the stream's identity and tool calls.
fn translate_copilot_event_to_chat_chunk(
    line: &str,
    model: &str,
    id: &mut Option<String>,
    created: &mut Option<u64>,
    tool_index_map: &mut HashMap<u32, u32>,
    next_tool_index: &mut u32,
) -> Option<String> {
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
        StreamEvent::ResponseCreated { response } => {
            // Capture stable id and created for all subsequent chunks
            *id = Some(response.id.clone());
            *created = Some(response.created_at);
            make_chunk(
                id,
                created,
                &response.model,
                vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: Some(ChatDelta {
                        role: Some("assistant".to_string()),
                        content: None,
                        refusal: None,
                        tool_calls: None,
                        reasoning_text: None,
                        reasoning_opaque: None,
                    }),
                    finish_reason: None,
                }],
                None,
            )
        }

        StreamEvent::OutputTextDelta { delta, .. } => make_chunk(
            id,
            created,
            model,
            vec![ChatCompletionChunkChoice {
                index: 0,
                delta: Some(ChatDelta {
                    role: None,
                    content: Some(delta),
                    refusal: None,
                    tool_calls: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                }),
                finish_reason: None,
            }],
            None,
        ),

        // First event for a function call output item: emit the tool call header chunk.
        // Assign an independent tool_call_index (sequential, not output_index).
        StreamEvent::OutputItemAdded {
            output_index,
            item:
                OutputItemAdded::FunctionCall {
                    id: _,
                    call_id,
                    name,
                    ..
                },
        } => {
            let tool_call_index = *next_tool_index;
            tool_index_map.insert(output_index, tool_call_index);
            *next_tool_index += 1;

            make_chunk(
                id,
                created,
                model,
                vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: Some(ChatDelta {
                        role: None,
                        content: None,
                        refusal: None,
                        tool_calls: Some(vec![ToolCallDelta {
                            index: tool_call_index,
                            id: Some(call_id),
                            kind: Some("function".to_string()),
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
                None,
            )
        }

        // Subsequent argument deltas: look up the tool_call_index from the map.
        StreamEvent::FunctionCallArgumentsDelta {
            output_index,
            delta,
            ..
        } => {
            let tool_call_index = tool_index_map.get(&output_index).copied().unwrap_or(0);
            make_chunk(
                id,
                created,
                model,
                vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: Some(ChatDelta {
                        role: None,
                        content: None,
                        refusal: None,
                        tool_calls: Some(vec![ToolCallDelta {
                            index: tool_call_index,
                            id: None,
                            kind: None,
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
                None,
            )
        }

        StreamEvent::ReasoningSummaryTextDelta { delta, .. } => make_chunk(
            id,
            created,
            model,
            vec![ChatCompletionChunkChoice {
                index: 0,
                delta: Some(ChatDelta {
                    role: None,
                    content: None,
                    refusal: None,
                    tool_calls: None,
                    reasoning_text: Some(delta),
                    reasoning_opaque: None,
                }),
                finish_reason: None,
            }],
            None,
        ),

        StreamEvent::ResponseCompleted { response } => {
            let usage = ChatUsage {
                prompt_tokens: Some(response.usage.input_tokens),
                completion_tokens: Some(response.usage.output_tokens),
                total_tokens: Some(response.usage.input_tokens + response.usage.output_tokens),
                prompt_tokens_details: None,
                completion_tokens_details: None,
            };
            let finish_chunk = make_chunk(
                id,
                created,
                model,
                vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: Some(ChatDelta {
                        role: None,
                        content: None,
                        refusal: None,
                        tool_calls: None,
                        reasoning_text: None,
                        reasoning_opaque: None,
                    }),
                    finish_reason: Some("stop".to_string()),
                }],
                Some(usage),
            );
            let json = serde_json::to_string(&finish_chunk).ok()?;
            return Some(format!("data: {}\n\ndata: [DONE]\n\n", json));
        }

        StreamEvent::ResponseIncomplete { response } => {
            let usage = ChatUsage {
                prompt_tokens: Some(response.usage.input_tokens),
                completion_tokens: Some(response.usage.output_tokens),
                total_tokens: Some(response.usage.input_tokens + response.usage.output_tokens),
                prompt_tokens_details: None,
                completion_tokens_details: None,
            };
            let finish_chunk = make_chunk(
                id,
                created,
                model,
                vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: Some(ChatDelta {
                        role: None,
                        content: None,
                        refusal: None,
                        tool_calls: None,
                        reasoning_text: None,
                        reasoning_opaque: None,
                    }),
                    finish_reason: Some("length".to_string()),
                }],
                Some(usage),
            );
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
