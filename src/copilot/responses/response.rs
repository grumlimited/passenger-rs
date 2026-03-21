//! Copilot `/responses` API — non-streaming response types.
//!
//! Translated from the Zod response schema in
//! `openai-responses-language-model.ts` (`doGenerate` handler).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    pub input_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<InputTokensDetails>,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<OutputTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputTokensDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutputTokensDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// Annotations on output_text parts
// ---------------------------------------------------------------------------

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Annotation {
    UrlCitation {
        start_index: u32,
        end_index: u32,
        url: String,
        title: String,
    },
    FileCitation(FileCitationAnnotation),
    ContainerFileCitation {},
}

/// Shared file-citation payload used in both `Annotation` (non-streaming)
/// and `StreamAnnotation` (streaming).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCitationAnnotation {
    pub file_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

// ---------------------------------------------------------------------------
// Logprobs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogprobEntry {
    pub token: String,
    pub logprob: f64,
    pub top_logprobs: Vec<TopLogprob>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TopLogprob {
    pub token: String,
    pub logprob: f64,
}

// ---------------------------------------------------------------------------
// Output items
// ---------------------------------------------------------------------------

/// A single item in the `output` array of the response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputItem {
    Message(MessageOutputItem),
    FunctionCall(FunctionCallOutputItem),
    Reasoning(ReasoningOutputItem),
    WebSearchCall(WebSearchCallItem),
    FileSearchCall(FileSearchCallItem),
    CodeInterpreterCall(CodeInterpreterCallItem),
    ImageGenerationCall(ImageGenerationCallItem),
    LocalShellCall(LocalShellCallItem),
    ComputerCall(ComputerCallItem),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageOutputItem {
    pub id: String,
    pub role: String, // "assistant"
    pub content: Vec<OutputTextPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutputTextPart {
    #[serde(rename = "type")]
    pub kind: OutputTextKind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<LogprobEntry>>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputTextKind {
    OutputText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCallOutputItem {
    pub id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReasoningOutputItem {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    pub summary: Vec<SummaryTextPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SummaryTextPart {
    #[serde(rename = "type")]
    pub kind: SummaryTextKind,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SummaryTextKind {
    SummaryText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebSearchCallItem {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSearchCallItem {
    pub id: String,
    pub queries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<FileSearchResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSearchResult {
    pub file_id: String,
    pub filename: String,
    pub score: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeInterpreterCallItem {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub container_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<CodeInterpreterOutput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodeInterpreterOutput {
    Logs { logs: String },
    Image { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageGenerationCallItem {
    pub id: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalShellCallItem {
    pub id: String,
    pub call_id: String,
    pub action: LocalShellAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalShellAction {
    #[serde(rename = "type")]
    pub kind: String, // "exec"
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComputerCallItem {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// Inline error (Copilot-specific: may return 200 with error in body)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InlineError {
    pub code: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Top-level response
// ---------------------------------------------------------------------------

/// Response body from `POST {copilot_api_base_url}/responses` (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CopilotResponsesResponse {
    pub id: String,
    pub created_at: u64,
    pub model: String,
    pub output: Vec<OutputItem>,
    pub usage: Usage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<InlineError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<IncompleteDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncompleteDetails {
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_response_json() -> serde_json::Value {
        json!({
            "id": "resp-1",
            "created_at": 1700000000_u64,
            "model": "gpt-5.4-mini",
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
        let resp: CopilotResponsesResponse =
            serde_json::from_value(minimal_response_json()).unwrap();
        assert_eq!(resp.id, "resp-1");
        assert_eq!(resp.model, "gpt-5.4-mini");
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_parse_message_output_text() {
        let resp: CopilotResponsesResponse =
            serde_json::from_value(minimal_response_json()).unwrap();
        match &resp.output[0] {
            OutputItem::Message(m) => {
                assert_eq!(m.content[0].text, "Hello!");
            }
            other => panic!("expected Message, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_function_call_output() {
        let json = json!({
            "id": "resp-2",
            "created_at": 1700000000_u64,
            "model": "gpt-5.4-mini",
            "output": [
                {
                    "type": "function_call",
                    "id": "item-1",
                    "call_id": "call-abc",
                    "name": "get_weather",
                    "arguments": "{\"city\":\"Paris\"}"
                }
            ],
            "usage": { "input_tokens": 5, "output_tokens": 20 }
        });
        let resp: CopilotResponsesResponse = serde_json::from_value(json).unwrap();
        match &resp.output[0] {
            OutputItem::FunctionCall(fc) => {
                assert_eq!(fc.name, "get_weather");
                assert_eq!(fc.call_id, "call-abc");
            }
            other => panic!("expected FunctionCall, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_reasoning_output() {
        let json = json!({
            "id": "resp-3",
            "created_at": 1700000000_u64,
            "model": "gpt-5.4-mini",
            "output": [
                {
                    "type": "reasoning",
                    "id": "rsn-1",
                    "summary": [
                        { "type": "summary_text", "text": "thinking..." }
                    ]
                }
            ],
            "usage": { "input_tokens": 5, "output_tokens": 10 }
        });
        let resp: CopilotResponsesResponse = serde_json::from_value(json).unwrap();
        match &resp.output[0] {
            OutputItem::Reasoning(r) => {
                assert_eq!(r.summary[0].text, "thinking...");
            }
            other => panic!("expected Reasoning, got {:?}", other),
        }
    }

    #[test]
    fn test_inline_error_is_parsed() {
        let json = json!({
            "id": "resp-err",
            "created_at": 1700000000_u64,
            "model": "gpt-5.4-mini",
            "output": [],
            "usage": { "input_tokens": 0, "output_tokens": 0 },
            "error": { "code": "model_not_found", "message": "Model not found." }
        });
        let resp: CopilotResponsesResponse = serde_json::from_value(json).unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, "model_not_found");
        assert_eq!(err.message, "Model not found.");
    }

    #[test]
    fn test_usage_details_optional_fields() {
        let json = json!({
            "id": "resp-4",
            "created_at": 1700000000_u64,
            "model": "gpt-5.4-mini",
            "output": [],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20,
                "input_tokens_details": { "cached_tokens": 5 },
                "output_tokens_details": { "reasoning_tokens": 3 }
            }
        });
        let resp: CopilotResponsesResponse = serde_json::from_value(json).unwrap();
        assert_eq!(
            resp.usage.input_tokens_details.unwrap().cached_tokens,
            Some(5)
        );
        assert_eq!(
            resp.usage.output_tokens_details.unwrap().reasoning_tokens,
            Some(3)
        );
    }

    #[test]
    fn test_url_citation_annotation_parsed() {
        let json = json!({
            "id": "resp-5",
            "created_at": 1700000000_u64,
            "model": "gpt-5.4-mini",
            "output": [
                {
                    "type": "message",
                    "id": "msg-1",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "output_text",
                            "text": "See [1].",
                            "annotations": [
                                {
                                    "type": "url_citation",
                                    "start_index": 4,
                                    "end_index": 7,
                                    "url": "https://example.com",
                                    "title": "Example"
                                }
                            ]
                        }
                    ]
                }
            ],
            "usage": { "input_tokens": 5, "output_tokens": 5 }
        });
        let resp: CopilotResponsesResponse = serde_json::from_value(json).unwrap();
        match &resp.output[0] {
            OutputItem::Message(m) => match &m.content[0].annotations[0] {
                Annotation::UrlCitation { url, title, .. } => {
                    assert_eq!(url, "https://example.com");
                    assert_eq!(title, "Example");
                }
                other => panic!("expected UrlCitation, got {:?}", other),
            },
            other => panic!("expected Message, got {:?}", other),
        }
    }
}
