//! Standard OpenAI `/responses` API — request types.
//!
//! The OpenAI Responses API request format is nearly identical to the Copilot
//! format. We define our own top-level request struct here (omitting the two
//! Copilot-only fields), and re-export the shared input/tool types from
//! `copilot::responses::request` directly.

pub use crate::copilot::responses::request::{
    AssistantContentPart, AssistantMessage, CodeInterpreterTool, ComputerUseTool, DeveloperMessage,
    FileSearchTool, FunctionCallItem, FunctionCallItemKind, FunctionTool, ImageGenerationTool,
    InputFile, InputImage, InputItem, ItemReference, ItemReferenceKind, LocalShellTool,
    ReasoningConfig, SystemMessage, TextConfig, TextFormat, Tool, ToolChoice, ToolChoiceFunction,
    ToolChoiceMode, ToolResult, ToolResultKind, UserContent, UserContentPart, UserMessage,
    WebSearchTool,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Request body received at `POST /v1/responses` from a standard OpenAI client.
///
/// This is the public-facing schema — identical to the Copilot request minus
/// the two Copilot-specific extension fields (`prompt_cache_key`,
/// `safety_identifier`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAIResponsesRequest {
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
    pub truncation: Option<String>,

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_minimal_request() {
        let json = json!({
            "model": "gpt-4o",
            "input": [
                { "role": "user", "content": "Hello" }
            ]
        });
        let req: OpenAIResponsesRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.model, "gpt-4o");
        assert_eq!(req.input.len(), 1);
        assert!(req.stream.is_none());
    }

    #[test]
    fn test_deserialize_with_tools() {
        let json = json!({
            "model": "gpt-4o",
            "input": [{ "role": "user", "content": "What's the weather?" }],
            "tools": [
                {
                    "type": "function",
                    "name": "get_weather",
                    "parameters": { "type": "object" }
                }
            ],
            "tool_choice": "auto"
        });
        let req: OpenAIResponsesRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.tools.as_ref().unwrap().len(), 1);
        assert_eq!(
            req.tool_choice,
            Some(ToolChoice::Mode(ToolChoiceMode::Auto))
        );
    }

    #[test]
    fn test_deserialize_with_instructions() {
        let json = json!({
            "model": "gpt-4o",
            "input": [],
            "instructions": "You are a helpful assistant.",
            "max_output_tokens": 512
        });
        let req: OpenAIResponsesRequest = serde_json::from_value(json).unwrap();
        assert_eq!(
            req.instructions.as_deref(),
            Some("You are a helpful assistant.")
        );
        assert_eq!(req.max_output_tokens, Some(512));
    }

    #[test]
    fn test_roundtrip_preserves_all_optional_fields() {
        let req = OpenAIResponsesRequest {
            model: "gpt-4o".to_string(),
            input: vec![],
            stream: Some(true),
            temperature: Some(0.7),
            top_p: Some(0.9),
            max_output_tokens: Some(1024),
            tools: None,
            tool_choice: None,
            instructions: Some("Be concise.".to_string()),
            store: Some(true),
            previous_response_id: Some("resp-prev".to_string()),
            reasoning: None,
            truncation: None,
            text: None,
            metadata: None,
            user: Some("user-123".to_string()),
            service_tier: None,
            parallel_tool_calls: None,
            max_tool_calls: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        let back: OpenAIResponsesRequest = serde_json::from_value(json).unwrap();
        assert_eq!(back, req);
    }
}
