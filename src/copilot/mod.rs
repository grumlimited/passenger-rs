pub mod models;
pub mod utils;

use crate::openai::completion::models::{Tool, ToolCall, ToolChoice};
use crate::server::chat_completion::{CopilotChoice, CopilotUsage};
use serde::{Deserialize, Serialize};

/// Copilot chat completion request
#[derive(Debug, Serialize)]
pub struct CopilotChatRequest {
    pub messages: Vec<CopilotMessage>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default)]
    pub padding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Copilot chat completion response
#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotChatResponse {
    pub id: String,
    #[serde(default)]
    pub created: Option<u64>,
    pub model: String,
    /// Optional system fingerprint (GitHub Copilot may omit this field)
    #[allow(dead_code)]
    // pub system_fingerprint: Option<String>,
    pub choices: Vec<CopilotChoice>,
    #[serde(default)]
    pub usage: Option<CopilotUsage>,
}
