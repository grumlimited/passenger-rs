//! Copilot `/responses` API — SSE streaming event types.
//!
//! Translated from the `openaiResponsesChunkSchema` union in
//! `openai-responses-language-model.ts`.
//!
//! Each SSE event arrives as `data: <json>` where the JSON has a `type` field.
//! This module provides the `StreamEvent` enum that covers all known event types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::response::{
    CodeInterpreterCallItem, FileCitationAnnotation, FileSearchCallItem, FunctionCallOutputItem,
    ImageGenerationCallItem, LocalShellCallItem, LogprobEntry, MessageOutputItem,
    ReasoningOutputItem, Usage, WebSearchCallItem,
};

// ---------------------------------------------------------------------------
// Partial output items used in output_item.added / output_item.done
// ---------------------------------------------------------------------------

/// Item variants for `response.output_item.added` (subset of fields available at creation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputItemAdded {
    Message {
        id: String,
    },
    Reasoning {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    WebSearchCall {
        id: String,
        status: String,
    },
    ComputerCall {
        id: String,
        status: String,
    },
    FileSearchCall {
        id: String,
    },
    ImageGenerationCall {
        id: String,
    },
    CodeInterpreterCall {
        id: String,
        container_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        outputs: Option<Value>,
        status: String,
    },
    LocalShellCall {
        id: String,
        call_id: String,
        action: Value,
    },
}

/// Item variants for `response.output_item.done` (full fields available at completion).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputItemDone {
    Message(MessageOutputItem),
    Reasoning(ReasoningOutputItem),
    FunctionCall(FunctionCallOutputItem),
    WebSearchCall(WebSearchCallItem),
    FileSearchCall(FileSearchCallItem),
    CodeInterpreterCall(CodeInterpreterCallItem),
    ImageGenerationCall(ImageGenerationCallItem),
    LocalShellCall(LocalShellCallItem),
    ComputerCall {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Annotation types in streaming
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamAnnotation {
    UrlCitation { url: String, title: String },
    FileCitation(FileCitationAnnotation),
}

// ---------------------------------------------------------------------------
// Partial response in created / completed / incomplete events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseCreatedPayload {
    pub id: String,
    pub created_at: u64,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseFinishedPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<IncompleteDetailsStream>,
    pub usage: Usage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncompleteDetailsStream {
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Top-level streaming event enum
// ---------------------------------------------------------------------------

/// A single SSE event from the Copilot Responses streaming API.
/// Unknown event types are captured by the `Unknown` variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    // Text delta
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        item_id: String,
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<LogprobEntry>>,
    },

    // Fired when a text part is fully assembled (no new content beyond deltas)
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
    },

    // Stream lifecycle
    #[serde(rename = "response.created")]
    ResponseCreated { response: ResponseCreatedPayload },

    #[serde(rename = "response.completed")]
    ResponseCompleted { response: ResponseFinishedPayload },

    #[serde(rename = "response.incomplete")]
    ResponseIncomplete { response: ResponseFinishedPayload },

    // Output item lifecycle
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        output_index: u32,
        item: OutputItemAdded,
    },

    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        output_index: u32,
        item: OutputItemDone,
    },

    // Function call argument streaming
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        item_id: String,
        output_index: u32,
        delta: String,
    },

    // Image generation partial
    #[serde(rename = "response.image_generation_call.partial_image")]
    ImageGenerationPartialImage {
        item_id: String,
        output_index: u32,
        partial_image_b64: String,
    },

    // Code interpreter
    #[serde(rename = "response.code_interpreter_call_code.delta")]
    CodeInterpreterCallCodeDelta {
        item_id: String,
        output_index: u32,
        delta: String,
    },

    #[serde(rename = "response.code_interpreter_call_code.done")]
    CodeInterpreterCallCodeDone {
        item_id: String,
        output_index: u32,
        code: String,
    },

    // Annotations
    #[serde(rename = "response.output_text.annotation.added")]
    OutputTextAnnotationAdded { annotation: StreamAnnotation },

    // Reasoning summary
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded { item_id: String, summary_index: u32 },

    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta {
        item_id: String,
        summary_index: u32,
        delta: String,
    },

    // Content part lifecycle (fired when a content part within a message item
    // is added or fully assembled; carries no new delta information).
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        output_index: u32,
        content_index: u32,
        item_id: String,
    },

    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        output_index: u32,
        content_index: u32,
        item_id: String,
    },

    // Response in-progress (sent after created, before any output)
    #[serde(rename = "response.in_progress")]
    ResponseInProgress {
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u32>,
    },

    // Error event
    #[serde(rename = "error")]
    Error {
        code: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        param: Option<String>,
        sequence_number: u32,
    },
}

/// Result of parsing a single SSE line.
#[derive(Debug)]
pub enum ParsedSseEvent {
    /// A successfully parsed, known event.
    Known(StreamEvent),
    /// The `type` field was present but not a known variant.
    /// The caller should log at `warn` level and skip.
    Unknown(String),
}

/// Parse a single `data: <json>` SSE line.
///
/// Returns:
/// - `None` if the line is not a `data:` line or is `data: [DONE]`
/// - `Some(Ok(ParsedSseEvent::Known(_)))` for known events
/// - `Some(Ok(ParsedSseEvent::Unknown(type_str)))` for unrecognised event types
/// - `Some(Err(_))` for malformed JSON or missing required fields on a known type
pub fn parse_sse_line(line: &str) -> Option<Result<ParsedSseEvent, serde_json::Error>> {
    let data = line.strip_prefix("data: ")?;
    if data.trim() == "[DONE]" {
        return None;
    }

    // Attempt full deserialisation first (fast path for known types).
    match serde_json::from_str::<StreamEvent>(data) {
        Ok(event) => Some(Ok(ParsedSseEvent::Known(event))),
        Err(e) => {
            // If it's an "unknown variant" error, extract the type string and
            // return Unknown so callers can warn-and-skip rather than error.
            let raw: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => return Some(Err(e)), // not even valid JSON
            };
            if let Some(type_str) = raw.get("type").and_then(|t| t.as_str()) {
                Some(Ok(ParsedSseEvent::Unknown(type_str.to_string())))
            } else {
                Some(Err(e)) // known type but missing required fields — real error
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unwrap a parsed SSE line, asserting it is a `Known` event.
    fn unwrap_known(line: &str) -> StreamEvent {
        match parse_sse_line(line).unwrap().unwrap() {
            ParsedSseEvent::Known(e) => e,
            ParsedSseEvent::Unknown(t) => panic!("expected Known event, got Unknown({})", t),
        }
    }

    #[test]
    fn test_parse_response_created() {
        let line = r#"data: {"type":"response.created","response":{"id":"resp-1","created_at":1700000000,"model":"gpt-5.4-mini"}}"#;
        match unwrap_known(line) {
            StreamEvent::ResponseCreated { response } => {
                assert_eq!(response.id, "resp-1");
                assert_eq!(response.model, "gpt-5.4-mini");
            }
            other => panic!("expected ResponseCreated, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_output_text_delta() {
        let line =
            r#"data: {"type":"response.output_text.delta","item_id":"msg-1","delta":"Hello"}"#;
        match unwrap_known(line) {
            StreamEvent::OutputTextDelta { item_id, delta, .. } => {
                assert_eq!(item_id, "msg-1");
                assert_eq!(delta, "Hello");
            }
            other => panic!("expected OutputTextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_response_completed_with_usage() {
        let line = r#"data: {"type":"response.completed","response":{"usage":{"input_tokens":10,"output_tokens":5},"incomplete_details":null}}"#;
        match unwrap_known(line) {
            StreamEvent::ResponseCompleted { response } => {
                assert_eq!(response.usage.input_tokens, 10);
                assert_eq!(response.usage.output_tokens, 5);
            }
            other => panic!("expected ResponseCompleted, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_function_call_arguments_delta() {
        let line = r#"data: {"type":"response.function_call_arguments.delta","item_id":"fc-1","output_index":0,"delta":"{\"city\":"}"#;
        match unwrap_known(line) {
            StreamEvent::FunctionCallArgumentsDelta {
                item_id,
                output_index,
                delta,
            } => {
                assert_eq!(item_id, "fc-1");
                assert_eq!(output_index, 0);
                assert!(!delta.is_empty());
            }
            other => panic!("expected FunctionCallArgumentsDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_output_item_added_message() {
        let line = r#"data: {"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg-1"}}"#;
        match unwrap_known(line) {
            StreamEvent::OutputItemAdded { output_index, item } => {
                assert_eq!(output_index, 0);
                match item {
                    OutputItemAdded::Message { id } => assert_eq!(id, "msg-1"),
                    other => panic!("expected Message, got {:?}", other),
                }
            }
            other => panic!("expected OutputItemAdded, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_output_item_added_function_call() {
        let line = r#"data: {"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"item-1","call_id":"call-abc","name":"get_weather","arguments":""}}"#;
        match unwrap_known(line) {
            StreamEvent::OutputItemAdded { item, .. } => match item {
                OutputItemAdded::FunctionCall { name, call_id, .. } => {
                    assert_eq!(name, "get_weather");
                    assert_eq!(call_id, "call-abc");
                }
                other => panic!("expected FunctionCall, got {:?}", other),
            },
            other => panic!("expected OutputItemAdded, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_error_event() {
        let line = r#"data: {"type":"error","code":"rate_limit","message":"Too many requests","sequence_number":3}"#;
        match unwrap_known(line) {
            StreamEvent::Error { code, message, .. } => {
                assert_eq!(code, "rate_limit");
                assert_eq!(message, "Too many requests");
            }
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_done_line_returns_none() {
        assert!(parse_sse_line("data: [DONE]").is_none());
    }

    #[test]
    fn test_non_data_line_returns_none() {
        assert!(parse_sse_line("event: response.created").is_none());
        assert!(parse_sse_line("").is_none());
        assert!(parse_sse_line(": keep-alive").is_none());
    }

    #[test]
    fn test_reasoning_summary_text_delta() {
        let line = r#"data: {"type":"response.reasoning_summary_text.delta","item_id":"rsn-1","summary_index":0,"delta":"thinking..."}"#;
        match unwrap_known(line) {
            StreamEvent::ReasoningSummaryTextDelta { item_id, delta, .. } => {
                assert_eq!(item_id, "rsn-1");
                assert_eq!(delta, "thinking...");
            }
            other => panic!("expected ReasoningSummaryTextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_url_citation_annotation() {
        let line = r#"data: {"type":"response.output_text.annotation.added","annotation":{"type":"url_citation","url":"https://example.com","title":"Example"}}"#;
        match unwrap_known(line) {
            StreamEvent::OutputTextAnnotationAdded { annotation } => match annotation {
                StreamAnnotation::UrlCitation { url, title } => {
                    assert_eq!(url, "https://example.com");
                    assert_eq!(title, "Example");
                }
                other => panic!("expected UrlCitation, got {:?}", other),
            },
            other => panic!("expected OutputTextAnnotationAdded, got {:?}", other),
        }
    }

    #[test]
    fn test_unknown_event_type_returns_unknown_variant() {
        let line = r#"data: {"type":"response.some_future_event","foo":"bar"}"#;
        match parse_sse_line(line).unwrap().unwrap() {
            ParsedSseEvent::Unknown(t) => assert_eq!(t, "response.some_future_event"),
            ParsedSseEvent::Known(e) => panic!("expected Unknown, got Known({:?})", e),
        }
    }

    #[test]
    fn test_content_part_done_is_known() {
        let line = r#"data: {"type":"response.content_part.done","output_index":0,"content_index":0,"item_id":"msg-1","part":{},"sequence_number":10}"#;
        match unwrap_known(line) {
            StreamEvent::ContentPartDone { .. } => {}
            other => panic!("expected ContentPartDone, got {:?}", other),
        }
    }

    #[test]
    fn test_response_in_progress_is_known() {
        let line = r#"data: {"type":"response.in_progress","sequence_number":1,"response":{}}"#;
        match unwrap_known(line) {
            StreamEvent::ResponseInProgress { .. } => {}
            other => panic!("expected ResponseInProgress, got {:?}", other),
        }
    }
}
