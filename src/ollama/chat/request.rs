//! Ollama `/api/chat` API — request types.
//!
//! Modelled from the official Ollama API docs:
//! https://github.com/ollama/ollama/blob/main/docs/api.md#generate-a-chat-completion

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// A single message in the `messages` array.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaMessage {
    pub role: OllamaRole,

    pub content: String,

    /// Optional base64-encoded images (multimodal models only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,

    /// Thinking content from reasoning models.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,

    /// Tool calls made by the assistant (assistant messages only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,

    /// Name of the tool that was called (tool messages only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OllamaRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool call in an assistant message.
///
/// Note: unlike OpenAI, Ollama tool calls have no `id` field and
/// `arguments` is a JSON object (not a serialised string).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaToolCall {
    pub function: OllamaToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaToolCallFunction {
    pub name: String,
    /// Arguments as a JSON object (not a string).
    pub arguments: Value,
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// A tool definition (same structure as OpenAI function tools).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OllamaTool {
    Function(OllamaFunctionTool),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaFunctionTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

// ---------------------------------------------------------------------------
// Format
// ---------------------------------------------------------------------------

/// The `format` field: either a named string (e.g. `"json"`) or a JSON schema object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum OllamaFormat {
    /// A named format string, e.g. `"json"`.
    Named(String),
    /// An inline JSON schema object.
    Schema(Value),
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// Model parameter options (a subset of the full Ollama options).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,

    /// Maximum number of tokens to predict (`-1` = infinite, `-2` = fill context).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    /// Reduces the probability of generating nonsense. Higher = more conservative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_p: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_last_n: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u32>,

    /// Stop sequences — generation stops when any of these are produced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Top-level request
// ---------------------------------------------------------------------------

/// Request body received at `POST /api/chat` from an Ollama client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<OllamaMessage>,

    /// Tools the model can call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OllamaTool>>,

    /// Enable thinking mode (for models that support it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think: Option<bool>,

    /// Response format: a named string or a JSON schema object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OllamaFormat>,

    /// Model parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OllamaOptions>,

    /// Whether to stream the response. Defaults to `true` in Ollama.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    /// How long to keep the model loaded. Duration string (e.g. `"5m"`) or seconds as integer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<Value>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_minimal_request() {
        let json = json!({
            "model": "llama3.2",
            "messages": [
                { "role": "user", "content": "Hello" }
            ]
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.model, "llama3.2");
        assert_eq!(req.messages.len(), 1);
        assert!(req.stream.is_none());
    }

    #[test]
    fn test_deserialize_system_and_user_messages() {
        let json = json!({
            "model": "llama3.2",
            "messages": [
                { "role": "system", "content": "You are helpful." },
                { "role": "user", "content": "Hi" }
            ]
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, OllamaRole::System);
        assert_eq!(req.messages[0].content, "You are helpful.");
    }

    #[test]
    fn test_deserialize_assistant_with_tool_calls() {
        let json = json!({
            "model": "llama3.2",
            "messages": [
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "function": {
                                "name": "get_weather",
                                "arguments": { "city": "Tokyo" }
                            }
                        }
                    ]
                }
            ]
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        let msg = &req.messages[0];
        assert_eq!(msg.role, OllamaRole::Assistant);
        let tc = &msg.tool_calls.as_ref().unwrap()[0];
        assert_eq!(tc.function.name, "get_weather");
        // arguments is a JSON object, not a string
        assert_eq!(tc.function.arguments["city"], "Tokyo");
    }

    #[test]
    fn test_deserialize_tool_message() {
        let json = json!({
            "model": "llama3.2",
            "messages": [
                {
                    "role": "tool",
                    "content": "22",
                    "tool_name": "get_weather"
                }
            ]
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        let msg = &req.messages[0];
        assert_eq!(msg.role, OllamaRole::Tool);
        assert_eq!(msg.tool_name.as_deref(), Some("get_weather"));
        assert_eq!(msg.content, "22");
    }

    #[test]
    fn test_deserialize_message_with_images() {
        let json = json!({
            "model": "llava",
            "messages": [
                {
                    "role": "user",
                    "content": "What is in this image?",
                    "images": ["iVBORw0KGgoAAAANSUhEUgAAAAUA"]
                }
            ]
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        let images = req.messages[0].images.as_ref().unwrap();
        assert_eq!(images.len(), 1);
        assert!(images[0].starts_with("iVBOR"));
    }

    #[test]
    fn test_deserialize_message_with_thinking() {
        let json = json!({
            "model": "deepseek-r1",
            "messages": [
                {
                    "role": "assistant",
                    "content": "The answer is 42.",
                    "thinking": "Let me reason through this step by step..."
                }
            ]
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(
            req.messages[0].thinking.as_deref(),
            Some("Let me reason through this step by step...")
        );
    }

    #[test]
    fn test_deserialize_with_function_tool() {
        let json = json!({
            "model": "llama3.2",
            "messages": [{ "role": "user", "content": "weather?" }],
            "tools": [
                {
                    "type": "function",
                    "name": "get_weather",
                    "description": "Get the weather for a city",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "city": { "type": "string" }
                        }
                    }
                }
            ]
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        let tools = req.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 1);
        match &tools[0] {
            OllamaTool::Function(f) => {
                assert_eq!(f.name, "get_weather");
                assert_eq!(f.description.as_deref(), Some("Get the weather for a city"));
            }
        }
    }

    #[test]
    fn test_deserialize_format_string() {
        let json = json!({
            "model": "llama3.2",
            "messages": [{ "role": "user", "content": "Give me JSON" }],
            "format": "json"
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        match req.format.unwrap() {
            OllamaFormat::Named(s) => assert_eq!(s, "json"),
            OllamaFormat::Schema(_) => panic!("expected Named"),
        }
    }

    #[test]
    fn test_deserialize_format_schema() {
        let json = json!({
            "model": "llama3.2",
            "messages": [{ "role": "user", "content": "Structured output" }],
            "format": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                }
            }
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        match req.format.unwrap() {
            OllamaFormat::Schema(v) => assert_eq!(v["type"], "object"),
            OllamaFormat::Named(_) => panic!("expected Schema"),
        }
    }

    #[test]
    fn test_deserialize_options() {
        let json = json!({
            "model": "llama3.2",
            "messages": [{ "role": "user", "content": "hi" }],
            "options": {
                "temperature": 0.8,
                "seed": 42,
                "num_predict": 100,
                "top_k": 40,
                "top_p": 0.9
            }
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        let opts = req.options.as_ref().unwrap();
        assert_eq!(opts.temperature, Some(0.8));
        assert_eq!(opts.seed, Some(42));
        assert_eq!(opts.num_predict, Some(100));
        assert_eq!(opts.top_k, Some(40));
        assert_eq!(opts.top_p, Some(0.9));
    }

    #[test]
    fn test_deserialize_keep_alive_string() {
        let json = json!({
            "model": "llama3.2",
            "messages": [{ "role": "user", "content": "hi" }],
            "keep_alive": "5m"
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.keep_alive.as_ref().unwrap(), &json!("5m"));
    }

    #[test]
    fn test_deserialize_keep_alive_integer() {
        let json = json!({
            "model": "llama3.2",
            "messages": [{ "role": "user", "content": "hi" }],
            "keep_alive": 300
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.keep_alive.as_ref().unwrap(), &json!(300));
    }

    #[test]
    fn test_deserialize_think_flag() {
        let json = json!({
            "model": "deepseek-r1",
            "messages": [{ "role": "user", "content": "solve it" }],
            "think": true
        });
        let req: OllamaChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.think, Some(true));
    }

    #[test]
    fn test_roundtrip() {
        let req = OllamaChatRequest {
            model: "llama3.2".to_string(),
            messages: vec![OllamaMessage {
                role: OllamaRole::User,
                content: "Hello".to_string(),
                images: None,
                thinking: None,
                tool_calls: None,
                tool_name: None,
            }],
            tools: None,
            think: None,
            format: None,
            options: Some(OllamaOptions {
                temperature: Some(0.7),
                ..Default::default()
            }),
            stream: Some(false),
            keep_alive: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        let back: OllamaChatRequest = serde_json::from_value(json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn test_stream_false_is_serialised() {
        let req = OllamaChatRequest {
            model: "llama3.2".to_string(),
            messages: vec![],
            tools: None,
            think: None,
            format: None,
            options: None,
            stream: Some(false),
            keep_alive: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["stream"], false);
        // options absent
        assert!(json.get("options").is_none());
    }
}
