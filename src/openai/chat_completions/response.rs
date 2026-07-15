//! Standard OpenAI `/chat/completions` API — streaming response types.
//!
//! Translated from `OpenAICompatibleChatResponseSchema` in
//! `openai-compatible-chat-language-model.ts`.
//!
//! Copilot-specific additions (`reasoning_text`, `reasoning_opaque`) are
//! included on `ChatDelta`; standard clients ignore them.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptTokensDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletionTokensDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_prediction_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_prediction_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// Streaming chunk
// ---------------------------------------------------------------------------

/// A single SSE chunk for `/chat/completions` streaming.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub choices: Vec<ChatCompletionChunkChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ChatUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionChunkChoice {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<ChatDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
    /// Copilot-specific: reasoning text delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_text: Option<String>,
    /// Copilot-specific: reasoning opaque token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_opaque: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallDelta {
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ToolCallFunctionDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFunctionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_streaming_chunk_text_delta() {
        let json = json!({
            "id": "chatcmpl-abc",
            "created": 1700000000_u64,
            "model": "gpt-4o",
            "choices": [
                {
                    "index": 0,
                    "delta": { "role": "assistant", "content": "Hi" },
                    "finish_reason": null
                }
            ]
        });
        let chunk: ChatCompletionChunk = serde_json::from_value(json).unwrap();
        let delta = chunk.choices[0].delta.as_ref().unwrap();
        assert_eq!(delta.content.as_deref(), Some("Hi"));
    }

    #[test]
    fn test_parse_streaming_chunk_tool_call_delta() {
        let json = json!({
            "choices": [
                {
                    "index": 0,
                    "delta": {
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": "call-1",
                                "function": { "name": "get_weather", "arguments": "" }
                            }
                        ]
                    },
                    "finish_reason": null
                }
            ]
        });
        let chunk: ChatCompletionChunk = serde_json::from_value(json).unwrap();
        let tc = &chunk.choices[0]
            .delta
            .as_ref()
            .unwrap()
            .tool_calls
            .as_ref()
            .unwrap()[0];
        assert_eq!(
            tc.function.as_ref().unwrap().name.as_deref(),
            Some("get_weather")
        );
    }

    #[test]
    fn test_parse_streaming_chunk_usage() {
        let json = json!({
            "choices": [],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 10,
                "total_tokens": 15,
                "completion_tokens_details": {
                    "reasoning_tokens": 3
                }
            }
        });
        let chunk: ChatCompletionChunk = serde_json::from_value(json).unwrap();
        let usage = chunk.usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(5));
        assert_eq!(
            usage.completion_tokens_details.unwrap().reasoning_tokens,
            Some(3)
        );
    }
}
