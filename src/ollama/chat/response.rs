//! Ollama `/api/chat` API — response types.
//!
//! Modelled from the official Ollama API docs:
//! https://github.com/ollama/ollama/blob/main/docs/api.md#generate-a-chat-completion
//!
//! Both streaming and non-streaming responses share the same top-level struct.
//! Intermediate streaming chunks have `done: false` and omit timing fields;
//! the final chunk (or the sole non-streaming response) has `done: true` and
//! includes timing statistics.

use serde::{Deserialize, Serialize};

use super::request::OllamaMessage;

// ---------------------------------------------------------------------------
// Top-level response
// ---------------------------------------------------------------------------

/// Response body from `POST /api/chat` (both streaming and non-streaming).
///
/// For streaming responses each line is a complete JSON object (NDJSON).
/// When `done` is `false` the timing fields are absent; when `done` is `true`
/// all timing fields are present.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaChatResponse {
    pub model: String,

    /// ISO 8601 timestamp of when the response was created.
    pub created_at: String,

    /// The assistant message for this chunk/response.
    pub message: OllamaMessage,

    /// Whether this is the final response chunk.
    pub done: bool,

    /// Reason generation stopped (`"stop"`, `"load"`, `"unload"`, `"length"`).
    /// Present on the final chunk only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_reason: Option<String>,

    // ------------------------------------------------------------------
    // Timing fields — present only when `done == true`.
    // ------------------------------------------------------------------
    /// Total time spent processing the request (nanoseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<u64>,

    /// Time spent loading the model (nanoseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_duration: Option<u64>,

    /// Number of tokens in the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_count: Option<u32>,

    /// Time spent evaluating the prompt (nanoseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_duration: Option<u64>,

    /// Number of tokens generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_count: Option<u32>,

    /// Time spent generating the response (nanoseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_duration: Option<u64>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::chat::request::{OllamaRole, OllamaToolCall, OllamaToolCallFunction};
    use serde_json::json;

    #[test]
    fn test_deserialize_streaming_chunk_in_progress() {
        let json = json!({
            "model": "llama3.2",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Hello"
            },
            "done": false
        });
        let resp: OllamaChatResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.model, "llama3.2");
        assert_eq!(resp.message.content, "Hello");
        assert!(!resp.done);
        assert!(resp.done_reason.is_none());
        assert!(resp.total_duration.is_none());
    }

    #[test]
    fn test_deserialize_final_streaming_chunk() {
        let json = json!({
            "model": "llama3.2",
            "created_at": "2024-01-01T00:00:01Z",
            "message": {
                "role": "assistant",
                "content": ""
            },
            "done": true,
            "done_reason": "stop",
            "total_duration": 5000000000_u64,
            "load_duration": 100000000_u64,
            "prompt_eval_count": 25,
            "prompt_eval_duration": 200000000_u64,
            "eval_count": 50,
            "eval_duration": 4000000000_u64
        });
        let resp: OllamaChatResponse = serde_json::from_value(json).unwrap();
        assert!(resp.done);
        assert_eq!(resp.done_reason.as_deref(), Some("stop"));
        assert_eq!(resp.total_duration, Some(5_000_000_000));
        assert_eq!(resp.load_duration, Some(100_000_000));
        assert_eq!(resp.prompt_eval_count, Some(25));
        assert_eq!(resp.prompt_eval_duration, Some(200_000_000));
        assert_eq!(resp.eval_count, Some(50));
        assert_eq!(resp.eval_duration, Some(4_000_000_000));
    }

    #[test]
    fn test_deserialize_non_streaming_response() {
        // Non-streaming: single JSON object with done=true
        let json = json!({
            "model": "llama3.2",
            "created_at": "2024-01-01T00:00:01Z",
            "message": {
                "role": "assistant",
                "content": "The sky is blue because of Rayleigh scattering."
            },
            "done": true,
            "done_reason": "stop",
            "total_duration": 8_000_000_000_u64,
            "load_duration": 50_000_000_u64,
            "prompt_eval_count": 15,
            "prompt_eval_duration": 100_000_000_u64,
            "eval_count": 30,
            "eval_duration": 7_000_000_000_u64
        });
        let resp: OllamaChatResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.message.role, OllamaRole::Assistant);
        assert!(resp.message.content.contains("Rayleigh"));
        assert!(resp.done);
        assert_eq!(resp.eval_count, Some(30));
    }

    #[test]
    fn test_deserialize_response_with_tool_calls() {
        let json = json!({
            "model": "llama3.2",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "function": {
                            "name": "get_weather",
                            "arguments": { "city": "Paris" }
                        }
                    }
                ]
            },
            "done": true,
            "done_reason": "stop"
        });
        let resp: OllamaChatResponse = serde_json::from_value(json).unwrap();
        let tc = &resp.message.tool_calls.as_ref().unwrap()[0];
        assert_eq!(tc.function.name, "get_weather");
        assert_eq!(tc.function.arguments["city"], "Paris");
    }

    #[test]
    fn test_serialise_skips_absent_timing_fields() {
        let resp = OllamaChatResponse {
            model: "llama3.2".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: "Hi".to_string(),
                images: None,
                thinking: None,
                tool_calls: None,
                tool_name: None,
            },
            done: false,
            done_reason: None,
            total_duration: None,
            load_duration: None,
            prompt_eval_count: None,
            prompt_eval_duration: None,
            eval_count: None,
            eval_duration: None,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v.get("total_duration").is_none());
        assert!(v.get("done_reason").is_none());
        assert_eq!(v["done"], false);
    }

    #[test]
    fn test_roundtrip_streaming_chunk() {
        let resp = OllamaChatResponse {
            model: "llama3.2".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: "Hello, world!".to_string(),
                images: None,
                thinking: None,
                tool_calls: None,
                tool_name: None,
            },
            done: false,
            done_reason: None,
            total_duration: None,
            load_duration: None,
            prompt_eval_count: None,
            prompt_eval_duration: None,
            eval_count: None,
            eval_duration: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        let back: OllamaChatResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn test_roundtrip_final_chunk() {
        let resp = OllamaChatResponse {
            model: "llama3.2".to_string(),
            created_at: "2024-01-01T00:00:01Z".to_string(),
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: String::new(),
                images: None,
                thinking: None,
                tool_calls: None,
                tool_name: None,
            },
            done: true,
            done_reason: Some("stop".to_string()),
            total_duration: Some(5_000_000_000),
            load_duration: Some(100_000_000),
            prompt_eval_count: Some(25),
            prompt_eval_duration: Some(200_000_000),
            eval_count: Some(50),
            eval_duration: Some(4_000_000_000),
        };
        let json = serde_json::to_value(&resp).unwrap();
        let back: OllamaChatResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn test_roundtrip_response_with_tool_calls() {
        let resp = OllamaChatResponse {
            model: "llama3.2".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: String::new(),
                images: None,
                thinking: None,
                tool_calls: Some(vec![OllamaToolCall {
                    function: OllamaToolCallFunction {
                        name: "get_weather".to_string(),
                        arguments: json!({ "city": "Tokyo" }),
                    },
                }]),
                tool_name: None,
            },
            done: true,
            done_reason: Some("stop".to_string()),
            total_duration: Some(1_000_000_000),
            load_duration: None,
            prompt_eval_count: Some(10),
            prompt_eval_duration: None,
            eval_count: Some(5),
            eval_duration: None,
        };
        let serialised = serde_json::to_value(&resp).unwrap();
        let back: OllamaChatResponse = serde_json::from_value(serialised).unwrap();
        assert_eq!(back, resp);
    }
}
