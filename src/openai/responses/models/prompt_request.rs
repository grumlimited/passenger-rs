use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequest {
    pub input: Vec<Message>,
    pub model: String,
    pub instructions: String,
    pub max_output_tokens: u32,
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub content: Vec<Content>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "input_text")]
    InputText { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub parameters: ToolParameters,
    pub strict: bool,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameters {
    pub properties: serde_json::Value,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(rename = "additionalProperties")]
    pub additional_properties: bool,
    pub required: Vec<String>,
}
