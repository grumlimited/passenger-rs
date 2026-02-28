use crate::copilot::CopilotChatRequest;
use crate::copilot::CopilotChatResponse;
use crate::openai::responses::models::prompt_request::PromptRequest;
use crate::openai::responses::models::prompt_response::{
    AdditionalParameters, AssistantContent, CompletionResponse, ContentPartText, Output,
    OutputMessage, OutputRole, ResponseObject, ResponseStatus, ResponseStreamEvent, Text,
};
use crate::server::copilot::CopilotIntegration;
use crate::server::{AppError, AppState, Server};
use axum::response::{IntoResponse, Response};
use axum::{extract::State, Json};
use futures_util::{StreamExt as _, TryStreamExt as _};
use serde_json::Value;
use std::io::Error;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;
use tracing::log::{error, info, warn};

pub(crate) trait OpenAiResponsesEndpoint: CopilotIntegration {
    async fn openai_responses_chat(
        state: State<Arc<AppState>>,
        request_as_text: String,
    ) -> Result<Response, AppError>;
}

impl OpenAiResponsesEndpoint for Server {
    async fn openai_responses_chat(
        State(state): State<Arc<AppState>>,
        request_as_text: String,
    ) -> Result<Response, AppError> {
        /*
         * We are not destructuring directly into a Json<PromptRequest> because the openai request
         * coming from Rig contains 2 "role" keys within the input["role" == "user"].
         * It is causing serde to fail on doing serde_json::from_str::<PromptRequest>(&request_as_text), yet
         * it is somewhat more laxist when parsing it into a json_serde::Value instead.
         */
        let request_as_value: Value = serde_json::from_str(&request_as_text).map_err(|e| {
            error!("Failed to parse request body as JSON: {}", e);
            AppError::BadRequest(format!("Invalid JSON: {}", e))
        })?;
        debug!(
            "request_as_value:\n{}",
            serde_json::to_string_pretty(&request_as_value).unwrap()
        );

        let request: PromptRequest = serde_json::from_value(request_as_value).map_err(|e| {
            error!("Failed to deserialize request into PromptRequest: {}", e);
            AppError::BadRequest(format!("Invalid request structure: {}", e))
        })?;

        debug!(
            "original_openai_request:\n{}",
            serde_json::to_string_pretty(&request).unwrap()
        );

        let is_stream = request.stream;

        // Get a valid Copilot token
        let token = Self::get_token(state.clone()).await?;

        // Transform OpenAI request to Copilot format
        let copilot_request: CopilotChatRequest = request.into();

        debug!(
            "copilot_request:\n{}",
            serde_json::to_string_pretty(&copilot_request).unwrap()
        );

        // Forward request to Copilot API
        let copilot_url = format!("{}/chat/completions", state.config.copilot.api_base_url);

        let response = Self::forward_prompt(state, token, copilot_url, &copilot_request).await?;

        let status = response.status();
        if !status.is_success() {
            return Self::handle_errors(response).await;
        }

        if is_stream {
            use axum::response::sse::{Event, Sse};

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should go forward")
                .as_secs();

            let byte_stream = response.bytes_stream();

            // State accumulated across chunks, captured by move into the closure.
            let mut accumulated_text = String::new();
            let mut response_id = String::new();
            let mut response_model = String::new();

            let sse_stream = byte_stream
                .map_err(|e: reqwest::Error| {
                    error!("Error reading streaming response from Copilot: {}", e);
                    Error::other(e.to_string())
                })
                .flat_map(move |result| {
                    let events: Vec<Result<Event, Error>> = match result {
                        Err(e) => vec![Err(e)],
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes).into_owned();
                            text.lines()
                                .flat_map(|line| {
                                    translate_sse_line(
                                        line,
                                        now,
                                        &mut response_id,
                                        &mut response_model,
                                        &mut accumulated_text,
                                    )
                                })
                                .collect()
                        }
                    };
                    futures_util::stream::iter(events)
                });

            info!("Streaming OpenAI Responses chat response");
            Ok(Sse::new(sse_stream).into_response())
        } else {
            // Non-streaming path: buffer the full response and return JSON.
            let copilot_response: CopilotChatResponse = response.json().await.map_err(|e| {
                error!("Failed to parse Copilot response: {}", e);
                AppError::InternalServerError(format!("Failed to parse Copilot response: {}", e))
            })?;

            debug!(
                "copilot_response:\n{}",
                serde_json::to_string_pretty(&copilot_response).unwrap()
            );

            let openai_response: CompletionResponse = copilot_response.into();

            debug!(
                "openai_response:\n{}",
                serde_json::to_string_pretty(&openai_response).unwrap()
            );

            info!("Successfully processed OpenAI Responses chat request");

            Ok(Json(openai_response).into_response())
        }
    }
}

// ---------------------------------------------------------------------------
// SSE translation helpers
// ---------------------------------------------------------------------------

/// Parsed content of a `chat.completion.chunk` SSE payload from Copilot.
#[derive(Debug, serde::Deserialize)]
struct CopilotChunk {
    id: String,
    model: String,
    choices: Vec<CopilotChunkChoice>,
    #[allow(dead_code)]
    usage: Option<CopilotChunkUsage>,
}

#[derive(Debug, serde::Deserialize)]
struct CopilotChunkChoice {
    delta: CopilotChunkDelta,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct CopilotChunkDelta {
    content: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct CopilotChunkUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

/// Translate one raw line from the Copilot SSE stream into zero or more
/// Responses API SSE events.
///
/// State that accumulates across calls (response_id, response_model,
/// accumulated_text) is passed as mutable references.
pub(crate) fn translate_sse_line(
    line: &str,
    created_at: u64,
    response_id: &mut String,
    response_model: &mut String,
    accumulated_text: &mut String,
) -> Vec<Result<axum::response::sse::Event, Error>> {
    // Strip the "data: " prefix produced by Copilot's SSE format.
    let payload = match line.strip_prefix("data: ") {
        Some(p) => p,
        None => {
            if !line.trim().is_empty() {
                warn!("Unexpected SSE line from Copilot: {}", line);
            }
            return vec![];
        }
    };

    // "[DONE]" signals the end of the Copilot stream.
    if payload == "[DONE]" {
        return emit_completed_events(created_at, response_id, response_model, accumulated_text);
    }

    // Parse the chunk JSON.
    let chunk: CopilotChunk = match serde_json::from_str(payload) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "Could not parse Copilot SSE chunk as JSON: {}: {}",
                e, payload
            );
            return vec![];
        }
    };

    // On the first chunk, capture id/model and emit the lifecycle open events.
    if response_id.is_empty() && !chunk.id.is_empty() {
        *response_id = chunk.id.clone();
        *response_model = chunk.model.clone();

        let created_event = make_event(ResponseStreamEvent::ResponseCreated {
            response: make_in_progress_response(
                response_id.clone(),
                response_model.clone(),
                created_at,
            ),
        });

        let item_added = make_event(ResponseStreamEvent::ResponseOutputItemAdded {
            output_index: 0,
            item: make_empty_output_message(response_id.clone()),
        });

        let part_added = make_event(ResponseStreamEvent::ResponseContentPartAdded {
            item_id: response_id.clone(),
            output_index: 0,
            content_index: 0,
            part: ContentPartText {
                kind: "output_text".to_string(),
                text: String::new(),
            },
        });

        let mut events = vec![created_event, item_added, part_added];
        events.extend(emit_delta_events(&chunk, response_id, accumulated_text));
        return events;
    }

    emit_delta_events(&chunk, response_id, accumulated_text)
}

/// Emit `response.output_text.delta` for each non-empty content delta in a chunk.
fn emit_delta_events(
    chunk: &CopilotChunk,
    response_id: &str,
    accumulated_text: &mut String,
) -> Vec<Result<axum::response::sse::Event, Error>> {
    chunk
        .choices
        .iter()
        .filter_map(|choice| {
            let delta = choice.delta.content.as_deref().unwrap_or("");
            if delta.is_empty() {
                return None;
            }
            accumulated_text.push_str(delta);
            Some(make_event(ResponseStreamEvent::ResponseOutputTextDelta {
                item_id: response_id.to_string(),
                output_index: 0,
                content_index: 0,
                delta: delta.to_string(),
            }))
        })
        .collect()
}

/// Emit the four terminal lifecycle events once `[DONE]` is received.
fn emit_completed_events(
    created_at: u64,
    response_id: &str,
    response_model: &str,
    accumulated_text: &str,
) -> Vec<Result<axum::response::sse::Event, Error>> {
    let full_text = accumulated_text.to_string();

    let text_done = make_event(ResponseStreamEvent::ResponseOutputTextDone {
        item_id: response_id.to_string(),
        output_index: 0,
        content_index: 0,
        text: full_text.clone(),
    });

    let part_done = make_event(ResponseStreamEvent::ResponseContentPartDone {
        item_id: response_id.to_string(),
        output_index: 0,
        content_index: 0,
        part: ContentPartText {
            kind: "output_text".to_string(),
            text: full_text.clone(),
        },
    });

    let finished_message = OutputMessage {
        id: response_id.to_string(),
        role: OutputRole::Assistant,
        status: ResponseStatus::Completed,
        content: vec![AssistantContent::OutputText(Text {
            text: full_text.clone(),
        })],
    };

    let item_done = make_event(ResponseStreamEvent::ResponseOutputItemDone {
        output_index: 0,
        item: finished_message.clone(),
    });

    let completed_response = CompletionResponse {
        id: response_id.to_string(),
        object: ResponseObject::Response,
        created_at,
        status: ResponseStatus::Completed,
        error: None,
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        model: response_model.to_string(),
        usage: None,
        output: vec![Output::Message(finished_message)],
        tools: vec![],
        additional_parameters: AdditionalParameters::default(),
    };

    let completed = make_event(ResponseStreamEvent::ResponseCompleted {
        response: completed_response,
    });

    vec![text_done, part_done, item_done, completed]
}

// ---------------------------------------------------------------------------
// Small constructors
// ---------------------------------------------------------------------------

fn make_in_progress_response(id: String, model: String, created_at: u64) -> CompletionResponse {
    CompletionResponse {
        id,
        object: ResponseObject::Response,
        created_at,
        status: ResponseStatus::InProgress,
        error: None,
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        model,
        usage: None,
        output: vec![],
        tools: vec![],
        additional_parameters: AdditionalParameters::default(),
    }
}

fn make_empty_output_message(id: String) -> OutputMessage {
    OutputMessage {
        id,
        role: OutputRole::Assistant,
        status: ResponseStatus::InProgress,
        content: vec![],
    }
}

fn make_event(event: ResponseStreamEvent) -> Result<axum::response::sse::Event, Error> {
    let event_type = match &event {
        ResponseStreamEvent::ResponseCreated { .. } => "response.created",
        ResponseStreamEvent::ResponseOutputItemAdded { .. } => "response.output_item.added",
        ResponseStreamEvent::ResponseContentPartAdded { .. } => "response.content_part.added",
        ResponseStreamEvent::ResponseOutputTextDelta { .. } => "response.output_text.delta",
        ResponseStreamEvent::ResponseOutputTextDone { .. } => "response.output_text.done",
        ResponseStreamEvent::ResponseContentPartDone { .. } => "response.content_part.done",
        ResponseStreamEvent::ResponseOutputItemDone { .. } => "response.output_item.done",
        ResponseStreamEvent::ResponseCompleted { .. } => "response.completed",
    };

    let data = serde_json::to_string(&event)
        .map_err(|e| Error::other(format!("Failed to serialize stream event: {}", e)))?;

    Ok(axum::response::sse::Event::default()
        .event(event_type)
        .data(data))
}
