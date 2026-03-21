//! Copilot `/responses` API — request types.
//!
//! Translated from the Zod schemas in
//! `openai-responses-language-model.ts` and
//! `convert-to-openai-responses-input.ts`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Input items (the `input` array)
// ---------------------------------------------------------------------------

/// A single item in the `input` array sent to the Copilot Responses API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum InputItem {
    /// A message from the user, developer, or system.
    #[serde(rename = "user")]
    UserMessage(UserMessage),
    #[serde(rename = "assistant")]
    AssistantMessage(AssistantMessage),
    #[serde(rename = "system")]
    SystemMessage(SystemMessage),
    #[serde(rename = "developer")]
    DeveloperMessage(DeveloperMessage),
    /// A tool (function call) result.
    #[serde(untagged)]
    ToolResult(ToolResult),
    /// An assistant function call item (multi-turn: tool call emitted by the model).
    #[serde(untagged)]
    FunctionCall(FunctionCallItem),
    /// A reference to a previously stored item (when `store: true`).
    #[serde(untagged)]
    ItemReference(ItemReference),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserMessage {
    pub content: UserContent,
}

/// User message content: either a plain string or an array of content parts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Parts(Vec<UserContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserContentPart {
    InputText { text: String },
    InputImage(InputImage),
    InputFile(InputFile),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputImage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantMessage {
    pub content: Vec<AssistantContentPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantContentPart {
    OutputText { text: String },
    Refusal { refusal: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemMessage {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeveloperMessage {
    pub content: String,
}

/// A function call result submitted back to the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    #[serde(rename = "type")]
    pub kind: ToolResultKind,
    pub call_id: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolResultKind {
    FunctionCallOutput,
    LocalShellCallOutput,
}

/// Reference to a previously stored item (multi-turn with `store: true`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemReference {
    #[serde(rename = "type")]
    pub kind: ItemReferenceKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ItemReferenceKind {
    ItemReference,
}

// ---------------------------------------------------------------------------
// Function call item (used in input for multi-turn)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCallItem {
    #[serde(rename = "type")]
    pub kind: FunctionCallItemKind,
    pub id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FunctionCallItemKind {
    FunctionCall,
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool {
    Function(FunctionTool),
    WebSearch(WebSearchTool),
    FileSearch(FileSearchTool),
    CodeInterpreter(CodeInterpreterTool),
    ImageGeneration(ImageGenerationTool),
    LocalShell(LocalShellTool),
    ComputerUse(ComputerUseTool),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebSearchTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSearchTool {
    pub vector_store_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeInterpreterTool {
    pub container: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageGenerationTool {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalShellTool {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComputerUseTool {
    pub display_width: u32,
    pub display_height: u32,
    pub environment: String,
}

// ---------------------------------------------------------------------------
// Tool choice
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ToolChoice {
    Mode(ToolChoiceMode),
    Function(ToolChoiceFunction),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoiceMode {
    Auto,
    None,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolChoiceFunction {
    #[serde(rename = "type")]
    pub kind: String, // "function"
    pub name: String,
}

// ---------------------------------------------------------------------------
// Top-level request body
// ---------------------------------------------------------------------------

/// Request body sent to `POST {copilot_api_base_url}/responses`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CopilotResponsesRequest {
    pub model: String,
    pub input: Vec<InputItem>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>, // "auto"

    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,

    /// Copilot-specific extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,

    /// Copilot-specific extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextFormat {
    JsonObject,
    JsonSchema {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        schema: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_user_message_text_roundtrip() {
        let msg = InputItem::UserMessage(UserMessage {
            content: UserContent::Text("hello".to_string()),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "hello");
        let back: InputItem = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn test_system_message_roundtrip() {
        let msg = InputItem::SystemMessage(SystemMessage {
            content: "You are helpful.".to_string(),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "system");
        assert_eq!(json["content"], "You are helpful.");
    }

    #[test]
    fn test_developer_message_roundtrip() {
        let msg = InputItem::DeveloperMessage(DeveloperMessage {
            content: "Be concise.".to_string(),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "developer");
    }

    #[test]
    fn test_assistant_message_roundtrip() {
        let msg = InputItem::AssistantMessage(AssistantMessage {
            content: vec![AssistantContentPart::OutputText {
                text: "Sure!".to_string(),
            }],
            id: Some("msg-1".to_string()),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"][0]["type"], "output_text");
        assert_eq!(json["content"][0]["text"], "Sure!");
    }

    #[test]
    fn test_tool_result_roundtrip() {
        let item = InputItem::ToolResult(ToolResult {
            kind: ToolResultKind::FunctionCallOutput,
            call_id: "call-1".to_string(),
            output: "{\"result\":42}".to_string(),
        });
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["type"], "function_call_output");
        assert_eq!(json["call_id"], "call-1");
    }

    #[test]
    fn test_function_tool_roundtrip() {
        let tool = Tool::Function(FunctionTool {
            name: "get_weather".to_string(),
            description: Some("Get weather".to_string()),
            parameters: json!({"type": "object", "properties": {}}),
            strict: Some(false),
        });
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["type"], "function");
        assert_eq!(json["name"], "get_weather");
    }

    #[test]
    fn test_request_minimal_roundtrip() {
        let req = CopilotResponsesRequest {
            model: "gpt-5.4-mini".to_string(),
            input: vec![InputItem::UserMessage(UserMessage {
                content: UserContent::Text("hi".to_string()),
            })],
            stream: Some(false),
            temperature: None,
            top_p: None,
            max_output_tokens: Some(100),
            tools: None,
            tool_choice: None,
            instructions: None,
            store: None,
            previous_response_id: None,
            reasoning: None,
            truncation: None,
            text: None,
            metadata: None,
            user: None,
            service_tier: None,
            parallel_tool_calls: None,
            max_tool_calls: None,
            prompt_cache_key: None,
            safety_identifier: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "gpt-5.4-mini");
        assert_eq!(json["input"][0]["role"], "user");
        assert_eq!(json["input"][0]["content"], "hi");
        assert_eq!(json["max_output_tokens"], 100);
        // None fields must not appear
        assert!(json.get("temperature").is_none());
        assert!(json.get("tools").is_none());
    }

    #[test]
    fn test_tool_choice_mode_serializes_as_string() {
        let tc = ToolChoice::Mode(ToolChoiceMode::Auto);
        let json = serde_json::to_value(&tc).unwrap();
        assert_eq!(json, json!("auto"));
    }

    #[test]
    fn test_tool_choice_function_roundtrip() {
        let tc = ToolChoice::Function(ToolChoiceFunction {
            kind: "function".to_string(),
            name: "get_weather".to_string(),
        });
        let json = serde_json::to_value(&tc).unwrap();
        assert_eq!(json["type"], "function");
        assert_eq!(json["name"], "get_weather");
    }
}
