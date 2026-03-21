//! Standard OpenAI `/responses` API — non-streaming response types.
//!
//! The OpenAI Responses API response format is nearly identical to Copilot's.
//! We re-export all shared output/annotation/usage types from
//! `copilot::responses::response` and define our own top-level struct that
//! omits the Copilot-specific `error` inline field.

#[allow(unused_imports)]
pub use crate::copilot::responses::response::{
    Annotation, CodeInterpreterCallItem, CodeInterpreterOutput, ComputerCallItem,
    FileSearchCallItem, FileSearchResult, FunctionCallOutputItem, ImageGenerationCallItem,
    IncompleteDetails, InputTokensDetails, LocalShellAction, LocalShellCallItem, LogprobEntry,
    MessageOutputItem, OutputItem, OutputTextKind, OutputTextPart, OutputTokensDetails,
    ReasoningOutputItem, SummaryTextKind, SummaryTextPart, TopLogprob, Usage, WebSearchCallItem,
};

use serde::{Deserialize, Serialize};

/// Response body returned at `POST /v1/responses` to a standard OpenAI client.
///
/// Identical to `CopilotResponsesResponse` minus the inline `error` field.
/// Any Copilot inline error should be detected upstream and converted to a
/// proper HTTP error response before forwarding to the client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAIResponsesResponse {
    pub id: String,
    pub created_at: u64,
    pub model: String,
    pub output: Vec<OutputItem>,
    pub usage: Usage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<IncompleteDetails>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_response_json() -> serde_json::Value {
        json!({
            "id": "resp-1",
            "created_at": 1700000000_u64,
            "model": "gpt-4o",
            "output": [
                {
                    "type": "message",
                    "id": "msg-1",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "output_text",
                            "text": "Hello!",
                            "annotations": []
                        }
                    ]
                }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5
            }
        })
    }

    #[test]
    fn test_parse_minimal_response() {
        let resp: OpenAIResponsesResponse =
            serde_json::from_value(minimal_response_json()).unwrap();
        assert_eq!(resp.id, "resp-1");
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
    }

    #[test]
    fn test_no_error_field_in_schema() {
        // OpenAI response must not expose the `error` field — verify it
        // round-trips cleanly without it (not present in struct).
        let resp: OpenAIResponsesResponse =
            serde_json::from_value(minimal_response_json()).unwrap();
        let serialized = serde_json::to_value(&resp).unwrap();
        assert!(serialized.get("error").is_none());
    }

    #[test]
    fn test_parse_message_output_text() {
        let resp: OpenAIResponsesResponse =
            serde_json::from_value(minimal_response_json()).unwrap();
        match &resp.output[0] {
            OutputItem::Message(m) => {
                assert_eq!(m.content[0].text, "Hello!");
            }
            other => panic!("expected Message, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip() {
        let resp = OpenAIResponsesResponse {
            id: "resp-rt".to_string(),
            created_at: 1700000000,
            model: "gpt-4o".to_string(),
            output: vec![],
            usage: Usage {
                input_tokens: 3,
                input_tokens_details: None,
                output_tokens: 7,
                output_tokens_details: None,
            },
            service_tier: Some("default".to_string()),
            incomplete_details: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        let back: OpenAIResponsesResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back, resp);
    }
}
