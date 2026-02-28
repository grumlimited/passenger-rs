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
use axum::{Json, extract::State};
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

    async fn openai_responses_chat_sse(response: reqwest::Response) -> Result<Response, AppError>;

    async fn openai_responses_chat_no_sse(
        response: reqwest::Response,
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
            Self::openai_responses_chat_sse(response).await
        } else {
            Self::openai_responses_chat_no_sse(response).await
        }
    }

    async fn openai_responses_chat_sse(response: reqwest::Response) -> Result<Response, AppError> {
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
    }

    async fn openai_responses_chat_no_sse(
        response: reqwest::Response,
    ) -> Result<Response, AppError> {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::responses::models::prompt_response::{
        AssistantContent, Output, ResponseStatus,
    };

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Build a fake `reqwest::Response` whose body is the given string.
    fn make_reqwest_response(body: impl Into<bytes::Bytes>) -> reqwest::Response {
        let http_resp = http::Response::builder()
            .status(200)
            .body(body.into())
            .unwrap();
        reqwest::Response::from(http_resp)
    }

    /// Parse one SSE block (event + data lines separated by blank lines) from
    /// the raw body text produced by `openai_responses_chat_sse`.
    ///
    /// Returns a `Vec` of `(event_name, parsed_json_value)` pairs in order.
    fn parse_sse_blocks(raw: &str) -> Vec<(String, serde_json::Value)> {
        raw.split("\n\n")
            .filter(|block| !block.trim().is_empty())
            .map(|block| {
                let mut event_name = String::new();
                let mut data_line = String::new();
                for line in block.lines() {
                    if let Some(e) = line.strip_prefix("event:") {
                        event_name = e.trim().to_string();
                    } else if let Some(d) = line.strip_prefix("data:") {
                        data_line = d.trim().to_string();
                    }
                }
                let value: serde_json::Value =
                    serde_json::from_str(&data_line).unwrap_or(serde_json::Value::Null);
                (event_name, value)
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // translate_sse_line — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_translate_empty_line_returns_no_events() {
        let mut id = String::new();
        let mut model = String::new();
        let mut text = String::new();
        let result = translate_sse_line("", 0, &mut id, &mut model, &mut text);
        assert!(result.is_empty(), "empty line should produce no events");
    }

    #[test]
    fn test_translate_whitespace_only_line_returns_no_events() {
        let mut id = String::new();
        let mut model = String::new();
        let mut text = String::new();
        let result = translate_sse_line("   ", 0, &mut id, &mut model, &mut text);
        assert!(result.is_empty());
    }

    #[test]
    fn test_translate_non_data_line_returns_no_events() {
        let mut id = String::new();
        let mut model = String::new();
        let mut text = String::new();
        // Lines that don't start with "data: " are silently skipped (warned but no events).
        let result = translate_sse_line("event: ping", 0, &mut id, &mut model, &mut text);
        assert!(result.is_empty());
    }

    #[test]
    fn test_translate_malformed_json_returns_no_events() {
        let mut id = String::new();
        let mut model = String::new();
        let mut text = String::new();
        let result = translate_sse_line("data: {bad json}", 0, &mut id, &mut model, &mut text);
        assert!(result.is_empty());
    }

    #[test]
    fn test_translate_first_chunk_emits_lifecycle_and_delta() {
        let payload = r#"{"id":"resp-1","model":"gpt-4o","choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let line = format!("data: {payload}");

        let mut id = String::new();
        let mut model = String::new();
        let mut text = String::new();

        let events = translate_sse_line(&line, 100, &mut id, &mut model, &mut text);

        // First chunk: response.created, output_item.added, content_part.added, output_text.delta
        assert_eq!(events.len(), 4, "first chunk must emit 4 events");
        assert_eq!(id, "resp-1");
        assert_eq!(model, "gpt-4o");
        assert_eq!(text, "Hello");

        let expected_names: &[&str] = &[
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
        ];

        for (i, expected) in expected_names.iter().enumerate() {
            let event = events[i].as_ref().expect("event must be Ok");
            // axum SSE Event's Debug repr contains the event name
            assert!(
                format!("{event:?}").contains(expected),
                "event[{i}] should be {expected}, got {event:?}"
            );
        }
    }

    #[test]
    fn test_translate_subsequent_chunk_emits_only_delta() {
        let payload = r#"{"id":"resp-1","model":"gpt-4o","choices":[{"delta":{"content":" world"},"finish_reason":null}]}"#;
        let line = format!("data: {payload}");

        // Pre-seed state as if the first chunk already ran.
        let mut id = "resp-1".to_string();
        let mut model = "gpt-4o".to_string();
        let mut text = "Hello".to_string();

        let events = translate_sse_line(&line, 100, &mut id, &mut model, &mut text);

        assert_eq!(
            events.len(),
            1,
            "subsequent chunk must emit only a delta event"
        );
        assert!(
            format!("{:?}", events[0].as_ref().unwrap()).contains("response.output_text.delta"),
            "must be a delta event"
        );
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_translate_chunk_with_empty_content_emits_no_delta() {
        let payload = r#"{"id":"resp-1","model":"gpt-4o","choices":[{"delta":{"content":""},"finish_reason":null}]}"#;
        let line = format!("data: {payload}");

        // Pre-seed as if the first chunk ran.
        let mut id = "resp-1".to_string();
        let mut model = "gpt-4o".to_string();
        let mut text = String::new();

        let events = translate_sse_line(&line, 100, &mut id, &mut model, &mut text);
        assert!(events.is_empty(), "empty delta must not emit any event");
    }

    #[test]
    fn test_translate_done_emits_four_terminal_events() {
        let mut id = "resp-1".to_string();
        let mut model = "gpt-4o".to_string();
        let mut text = "Hello world".to_string();

        let events = translate_sse_line("data: [DONE]", 100, &mut id, &mut model, &mut text);

        assert_eq!(events.len(), 4, "[DONE] must emit 4 terminal events");

        let expected_names = [
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ];
        for (i, name) in expected_names.iter().enumerate() {
            let event = events[i].as_ref().expect("event must be Ok");
            assert!(
                format!("{event:?}").contains(name),
                "event[{i}] should be {name}, got {event:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // openai_responses_chat_no_sse
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_no_sse_returns_completion_response() {
        let copilot_body = serde_json::json!({
            "id": "copilot-id-1",
            "created": 1700000000u64,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hi there!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 3,
                "total_tokens": 8
            }
        });

        let response = make_reqwest_response(copilot_body.to_string());
        let result = <Server as OpenAiResponsesEndpoint>::openai_responses_chat_no_sse(response)
            .await
            .expect("should not error");

        assert_eq!(result.status(), 200);

        let body_bytes = axum::body::to_bytes(result.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: CompletionResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(parsed.status, ResponseStatus::Completed);
        assert_eq!(parsed.model, "gpt-4o");
        assert!(!parsed.output.is_empty(), "output must contain a message");

        if let Output::Message(msg) = &parsed.output[0] {
            assert_eq!(msg.status, ResponseStatus::Completed);
            assert_eq!(msg.role, OutputRole::Assistant);
            assert!(!msg.content.is_empty(), "message content must not be empty");
            if let AssistantContent::OutputText(text) = &msg.content[0] {
                assert_eq!(text.text, "Hi there!");
            } else {
                panic!("expected OutputText content");
            }
        } else {
            panic!("expected Output::Message");
        }
    }

    // -----------------------------------------------------------------------
    // openai_responses_chat_sse
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sse_response_has_correct_content_type() {
        let chunk_payload = r#"{"id":"r1","model":"gpt-4o","choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}"#;
        let body = format!("data: {chunk_payload}\ndata: [DONE]\n");

        let response = make_reqwest_response(body);
        let result = <Server as OpenAiResponsesEndpoint>::openai_responses_chat_sse(response)
            .await
            .expect("should not error");

        assert_eq!(result.status(), 200);
        let ct = result
            .headers()
            .get("content-type")
            .expect("must have content-type")
            .to_str()
            .unwrap();
        assert!(ct.contains("text/event-stream"), "content-type must be SSE");
    }

    #[tokio::test]
    async fn test_sse_response_emits_expected_event_sequence() {
        let chunk_payload = r#"{"id":"r1","model":"gpt-4o","choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}"#;
        let body = format!("data: {chunk_payload}\ndata: [DONE]\n");

        let response = make_reqwest_response(body);
        let result = <Server as OpenAiResponsesEndpoint>::openai_responses_chat_sse(response)
            .await
            .unwrap();

        let body_bytes = axum::body::to_bytes(result.into_body(), usize::MAX)
            .await
            .unwrap();
        let raw = std::str::from_utf8(&body_bytes).unwrap();
        let blocks = parse_sse_blocks(raw);

        // Expected sequence: created, output_item.added, content_part.added,
        //                    output_text.delta,
        //                    output_text.done, content_part.done, output_item.done, completed
        let expected_events = [
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ];

        assert_eq!(
            blocks.len(),
            expected_events.len(),
            "wrong number of SSE events; got: {:?}",
            blocks.iter().map(|(e, _)| e.as_str()).collect::<Vec<_>>()
        );

        for (i, expected) in expected_events.iter().enumerate() {
            assert_eq!(blocks[i].0, *expected, "event[{i}] should be {expected}");
        }
    }

    #[tokio::test]
    async fn test_sse_response_delta_carries_correct_text() {
        let chunk_payload = r#"{"id":"r2","model":"gpt-4o","choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let body = format!("data: {chunk_payload}\ndata: [DONE]\n");

        let response = make_reqwest_response(body);
        let result = <Server as OpenAiResponsesEndpoint>::openai_responses_chat_sse(response)
            .await
            .unwrap();

        let body_bytes = axum::body::to_bytes(result.into_body(), usize::MAX)
            .await
            .unwrap();
        let raw = std::str::from_utf8(&body_bytes).unwrap();
        let blocks = parse_sse_blocks(raw);

        // The delta event (index 3) should carry "Hello"
        let delta_block = blocks
            .iter()
            .find(|(e, _)| e == "response.output_text.delta")
            .expect("must have a delta event");
        assert_eq!(delta_block.1["delta"], "Hello");
        assert_eq!(delta_block.1["item_id"], "r2");

        // The done event should carry the full accumulated text
        let done_block = blocks
            .iter()
            .find(|(e, _)| e == "response.output_text.done")
            .expect("must have a done event");
        assert_eq!(done_block.1["text"], "Hello");
    }

    #[tokio::test]
    async fn test_sse_response_completed_event_has_correct_model() {
        let chunk_payload = r#"{"id":"r3","model":"gpt-4o-mini","choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}"#;
        let body = format!("data: {chunk_payload}\ndata: [DONE]\n");

        let response = make_reqwest_response(body);
        let result = <Server as OpenAiResponsesEndpoint>::openai_responses_chat_sse(response)
            .await
            .unwrap();

        let body_bytes = axum::body::to_bytes(result.into_body(), usize::MAX)
            .await
            .unwrap();
        let raw = std::str::from_utf8(&body_bytes).unwrap();
        let blocks = parse_sse_blocks(raw);

        let completed = blocks
            .iter()
            .find(|(e, _)| e == "response.completed")
            .expect("must have a completed event");
        assert_eq!(completed.1["response"]["model"], "gpt-4o-mini");
        assert_eq!(completed.1["response"]["status"], "completed");
    }

    #[tokio::test]
    async fn test_sse_response_multi_chunk_accumulates_text() {
        let chunk1 = r#"{"id":"r4","model":"gpt-4o","choices":[{"delta":{"content":"Foo"},"finish_reason":null}]}"#;
        let chunk2 = r#"{"id":"r4","model":"gpt-4o","choices":[{"delta":{"content":"Bar"},"finish_reason":null}]}"#;
        let body = format!("data: {chunk1}\ndata: {chunk2}\ndata: [DONE]\n");

        let response = make_reqwest_response(body);
        let result = <Server as OpenAiResponsesEndpoint>::openai_responses_chat_sse(response)
            .await
            .unwrap();

        let body_bytes = axum::body::to_bytes(result.into_body(), usize::MAX)
            .await
            .unwrap();
        let raw = std::str::from_utf8(&body_bytes).unwrap();
        let blocks = parse_sse_blocks(raw);

        // 3 lifecycle open events + 2 deltas + 4 terminal = 9
        assert_eq!(
            blocks.len(),
            9,
            "two-chunk stream should emit 9 events; got: {:?}",
            blocks.iter().map(|(e, _)| e.as_str()).collect::<Vec<_>>()
        );

        let done_block = blocks
            .iter()
            .find(|(e, _)| e == "response.output_text.done")
            .unwrap();
        assert_eq!(
            done_block.1["text"], "FooBar",
            "accumulated text must be FooBar"
        );
    }
}
