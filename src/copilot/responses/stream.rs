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
    CodeInterpreterCallItem, FileSearchCallItem, FunctionCallOutputItem, ImageGenerationCallItem,
    LocalShellCallItem, LogprobEntry, MessageOutputItem, ReasoningOutputItem, Usage,
    WebSearchCallItem,
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
    UrlCitation {
        url: String,
        title: String,
    },
    FileCitation {
        file_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        start_index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        end_index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        quote: Option<String>,
    },
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

/// Parse a single `data: <json>` SSE line into a `StreamEvent`.
/// Returns `None` if the line is not a `data:` line, is `data: [DONE]`,
/// or cannot be parsed (unknown type is returned as `Err`).
pub fn parse_sse_line(line: &str) -> Option<Result<StreamEvent, serde_json::Error>> {
    let data = line.strip_prefix("data: ")?;
    if data.trim() == "[DONE]" {
        return None;
    }
    Some(serde_json::from_str(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_response_created() {
        let line = r#"data: {"type":"response.created","response":{"id":"resp-1","created_at":1700000000,"model":"gpt-5.4-mini"}}"#;
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
        let event = parse_sse_line(line).unwrap().unwrap();
        match event {
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
}
