//! Standard OpenAI `/chat/completions` API — request types.
//!
//! Translated from the Zod schemas in `openai-compatible-chat-language-model.ts`
//! and `openai-compatible-api-types.ts`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// A single message in the `messages` array.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatMessage {
    System(SystemMessage),
    User(UserMessage),
    Assistant(AssistantMessage),
    Tool(ToolMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemMessage {
    pub content: SystemContent,
}

/// System content: a plain string or an array of text parts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SystemContent {
    Text(String),
    Parts(Vec<SystemContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemContentPart {
    #[serde(rename = "type")]
    pub kind: SystemContentKind,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SystemContentKind {
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserMessage {
    pub content: UserContent,
}

/// User content: a plain string or an array of content parts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Parts(Vec<UserContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Copilot-specific: reasoning summary text (plain text, shown to user).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_text: Option<String>,
    /// Copilot-specific: encrypted reasoning token for multi-turn continuity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_opaque: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: ToolCallKind,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallKind {
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolMessage {
    pub content: String,
    pub tool_call_id: String,
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatTool {
    Function(ChatFunctionToolWrapper),
}

/// Wrapper matching the OpenAI wire format:
/// `{"type": "function", "function": {"name": ..., ...}}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatFunctionToolWrapper {
    pub function: ChatFunctionTool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatFunctionTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Tool choice: `"none"`, `"auto"`, `"required"`, or `{"type":"function","function":{"name":"..."}}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ChatToolChoice {
    Mode(ChatToolChoiceMode),
    Function(ChatToolChoiceFunction),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChatToolChoiceMode {
    None,
    Auto,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatToolChoiceFunction {
    #[serde(rename = "type")]
    pub kind: String, // "function"
    pub function: ChatToolChoiceFunctionName,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatToolChoiceFunctionName {
    pub name: String,
}

// ---------------------------------------------------------------------------
// Response format
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    Text,
    JsonObject,
    JsonSchema { json_schema: JsonSchemaConfig },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonSchemaConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub schema: Value,
}

// ---------------------------------------------------------------------------
// Top-level request
// ---------------------------------------------------------------------------

/// Request body received at `POST /v1/chat/completions` from a standard OpenAI client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionsRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatTool>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ChatToolChoice>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// Copilot-specific: controls reasoning verbosity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,

    /// Copilot-specific: text verbosity level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<String>,

    /// Copilot-specific: maximum reasoning token budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_minimal_request() {
        let json = json!({
            "model": "gpt-4o",
            "messages": [
                { "role": "user", "content": "Hello" }
            ]
        });
        let req: ChatCompletionsRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.model, "gpt-4o");
        assert_eq!(req.messages.len(), 1);
        assert!(req.stream.is_none());
    }

    #[test]
    fn test_deserialize_system_and_user_messages() {
        let json = json!({
            "model": "gpt-4o",
            "messages": [
                { "role": "system", "content": "You are helpful." },
                { "role": "user", "content": "Hi" }
            ]
        });
        let req: ChatCompletionsRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.messages.len(), 2);
        match &req.messages[0] {
            ChatMessage::System(s) => match &s.content {
                SystemContent::Text(t) => assert_eq!(t, "You are helpful."),
                _ => panic!("expected text"),
            },
            _ => panic!("expected system"),
        }
    }

    #[test]
    fn test_deserialize_assistant_with_tool_calls() {
        let json = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call-1",
                            "type": "function",
                            "function": { "name": "get_weather", "arguments": "{\"city\":\"Paris\"}" }
                        }
                    ]
                }
            ]
        });
        let req: ChatCompletionsRequest = serde_json::from_value(json).unwrap();
        match &req.messages[0] {
            ChatMessage::Assistant(a) => {
                let tc = &a.tool_calls.as_ref().unwrap()[0];
                assert_eq!(tc.function.name, "get_weather");
                assert_eq!(tc.id, "call-1");
            }
            _ => panic!("expected assistant"),
        }
    }

    #[test]
    fn test_deserialize_tool_message() {
        let json = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "tool",
                    "content": "{\"temperature\":22}",
                    "tool_call_id": "call-1"
                }
            ]
        });
        let req: ChatCompletionsRequest = serde_json::from_value(json).unwrap();
        match &req.messages[0] {
            ChatMessage::Tool(t) => {
                assert_eq!(t.tool_call_id, "call-1");
                assert!(t.content.contains("temperature"));
            }
            _ => panic!("expected tool"),
        }
    }

    #[test]
    fn test_deserialize_user_content_parts() {
        let json = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": "What is in this image?" },
                        { "type": "image_url", "image_url": { "url": "https://example.com/img.png" } }
                    ]
                }
            ]
        });
        let req: ChatCompletionsRequest = serde_json::from_value(json).unwrap();
        match &req.messages[0] {
            ChatMessage::User(u) => match &u.content {
                UserContent::Parts(parts) => {
                    assert_eq!(parts.len(), 2);
                    match &parts[1] {
                        UserContentPart::ImageUrl { image_url } => {
                            assert!(image_url.url.contains("img.png"))
                        }
                        _ => panic!("expected image_url"),
                    }
                }
                _ => panic!("expected parts"),
            },
            _ => panic!("expected user"),
        }
    }

    #[test]
    fn test_deserialize_with_function_tool() {
        let json = json!({
            "model": "gpt-4o",
            "messages": [{ "role": "user", "content": "weather?" }],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "parameters": { "type": "object" }
                    }
                }
            ],
            "tool_choice": "auto"
        });
        let req: ChatCompletionsRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.tools.as_ref().unwrap().len(), 1);
        match &req.tools.as_ref().unwrap()[0] {
            ChatTool::Function(w) => assert_eq!(w.function.name, "get_weather"),
        }
        assert_eq!(
            req.tool_choice,
            Some(ChatToolChoice::Mode(ChatToolChoiceMode::Auto))
        );
    }

    #[test]
    fn test_roundtrip_minimal() {
        let req = ChatCompletionsRequest {
            model: "gpt-4o".to_string(),
            messages: vec![ChatMessage::User(UserMessage {
                content: UserContent::Text("Hello".to_string()),
            })],
            stream: Some(false),
            stream_options: None,
            temperature: Some(0.7),
            top_p: None,
            max_tokens: Some(512),
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            seed: None,
            response_format: None,
            tools: None,
            tool_choice: None,
            user: None,
            reasoning_effort: None,
            verbosity: None,
            thinking_budget: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        let back: ChatCompletionsRequest = serde_json::from_value(json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn test_copilot_specific_fields() {
        let json = json!({
            "model": "gpt-4.5",
            "messages": [{ "role": "user", "content": "Think hard." }],
            "reasoning_effort": "high",
            "verbosity": "detailed",
            "thinking_budget": 10000
        });
        let req: ChatCompletionsRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(req.verbosity.as_deref(), Some("detailed"));
        assert_eq!(req.thinking_budget, Some(10000));
    }
}
