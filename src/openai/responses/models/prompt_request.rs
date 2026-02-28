use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequest {
    pub input: Vec<Message>,
    pub model: String,
    pub instructions: Option<String>,
    pub max_output_tokens: Option<u32>,
    #[serde(default = "default_tools")]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Option<String>,
    #[serde(rename = "type")]
    pub message_type: String,
    pub content: Option<Vec<Content>>,
    pub name: Option<String>,
    pub arguments: Option<String>,
    pub output: Option<String>,
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

fn default_tools() -> Vec<Tool> {
    vec![]
}
