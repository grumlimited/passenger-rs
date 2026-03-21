//! Standard OpenAI `/responses` API — SSE streaming event types.
//!
//! The OpenAI Responses streaming format is identical to the Copilot streaming
//! format (same SSE event type names and JSON shapes). We simply re-export all
//! types from `copilot::responses::stream` so the rest of the codebase can
//! refer to either namespace interchangeably.

#[allow(unused_imports)]
pub use crate::copilot::responses::stream::{
    IncompleteDetailsStream, OutputItemAdded, OutputItemDone, ParsedSseEvent,
    ResponseCreatedPayload, ResponseFinishedPayload, StreamAnnotation, StreamEvent, parse_sse_line,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_text_delta() {
        let line =
            r#"data: {"type":"response.output_text.delta","item_id":"msg-1","delta":"Hello"}"#;
        match parse_sse_line(line).unwrap().unwrap() {
            ParsedSseEvent::Known(StreamEvent::OutputTextDelta { item_id, delta, .. }) => {
                assert_eq!(item_id, "msg-1");
                assert_eq!(delta, "Hello");
            }
            other => panic!("expected OutputTextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_done_line_returns_none() {
        assert!(parse_sse_line("data: [DONE]").is_none());
    }

    #[test]
    fn test_parse_response_completed() {
        let line = r#"data: {"type":"response.completed","response":{"usage":{"input_tokens":10,"output_tokens":5},"incomplete_details":null}}"#;
        match parse_sse_line(line).unwrap().unwrap() {
            ParsedSseEvent::Known(StreamEvent::ResponseCompleted { response }) => {
                assert_eq!(response.usage.input_tokens, 10);
            }
            other => panic!("expected ResponseCompleted, got {:?}", other),
        }
    }
}
