//! Copilot models API — types for the GitHub Copilot models response.
//!
//! The endpoint returns `{ "object": "list", "data": [...] }`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotModelsResponse {
    pub data: Vec<CopilotModel>,
    pub object: String,
}

impl CopilotModelsResponse {
    pub fn visible_models(self) -> Vec<CopilotModel> {
        self.data
            .into_iter()
            .filter(|model| {
                model
                    .policy
                    .as_ref()
                    .map(|policy| policy.state.as_str())
                    .filter(|state| *state == "enabled")
                    .is_some()
            })
            .collect()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotModel {
    pub id: String,
    pub name: String,
    pub object: String,
    pub model_picker_enabled: bool,
    #[serde(default)]
    pub model_picker_category: Option<String>,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub supported_endpoints: Vec<String>,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub policy: Option<CopilotModelPolicy>,
    pub capabilities: CopilotModelCapabilities,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotModelCapabilities {
    pub family: String,
    pub limits: Option<CopilotModelCapabilitiesLimits>,
    pub object: String,
    pub supports: CopilotModelCapabilitiesSupports,
    pub tokenizer: Option<String>,
    #[serde(rename = "type")]
    pub model_type: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CopilotModelCapabilitiesLimits {
    #[serde(default)]
    pub max_inputs: Option<u64>,
    #[serde(default)]
    pub max_context_window_tokens: Option<u64>,
    #[serde(default)]
    pub max_non_streaming_output_tokens: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub max_prompt_tokens: Option<u64>,
    #[serde(default)]
    pub vision: Option<CopilotModelCapabilitiesVisionLimits>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CopilotModelCapabilitiesSupports {
    #[serde(default)]
    pub adaptive_thinking: bool,
    #[serde(default)]
    pub max_thinking_budget: Option<u64>,
    #[serde(default)]
    pub min_thinking_budget: Option<u64>,
    #[serde(default)]
    pub parallel_tool_calls: bool,
    #[serde(default)]
    pub reasoning_effort: Vec<String>,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub structured_outputs: bool,
    #[serde(default)]
    pub tool_calls: bool,
    #[serde(default)]
    pub vision: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CopilotModelCapabilitiesVisionLimits {
    #[serde(default)]
    pub max_prompt_image_size: u64,
    #[serde(default)]
    pub max_prompt_images: u64,
    #[serde(default)]
    pub supported_media_types: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotModelPolicy {
    pub state: String,
    pub terms: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "object": "list",
        "data": [
            {
                "capabilities": {
                    "family": "auto-model-3",
                    "limits": {
                        "max_context_window_tokens": 264000,
                        "max_non_streaming_output_tokens": 16000,
                        "max_output_tokens": 64000,
                        "max_prompt_tokens": 200000,
                        "vision": {
                            "max_prompt_image_size": 3145728,
                            "max_prompt_images": 1,
                            "supported_media_types": [
                                "image/jpeg",
                                "image/png",
                                "image/webp",
                                "image/gif",
                                "application/pdf"
                            ]
                        }
                    },
                    "object": "model_capabilities",
                    "supports": {
                        "adaptive_thinking": true,
                        "max_thinking_budget": 32000,
                        "min_thinking_budget": 1024,
                        "parallel_tool_calls": true,
                        "reasoning_effort": ["low", "medium", "high", "xhigh", "max"],
                        "streaming": true,
                        "structured_outputs": true,
                        "tool_calls": true,
                        "vision": true
                    },
                    "tokenizer": "o200k_base",
                    "type": "chat"
                },
                "id": "auto-model-3",
                "model_picker_category": "versatile",
                "model_picker_enabled": false,
                "name": "Auto model",
                "object": "model",
                "policy": {
                    "state": "disabled",
                    "terms": "Enable access to the latest Auto model 3. [Learn more](https://example.com)."
                },
                "preview": true,
                "supported_endpoints": ["/chat/completions"],
                "vendor": "Experimental",
                "version": "auto-model-3"
            },
            {
                "capabilities": {
                    "family": "gpt-4.1",
                    "limits": {
                        "max_context_window_tokens": 128000,
                        "max_non_streaming_output_tokens": 8000,
                        "max_output_tokens": 16384,
                        "max_prompt_tokens": 100000,
                        "vision": {
                            "max_prompt_image_size": 3145728,
                            "max_prompt_images": 1,
                            "supported_media_types": ["image/jpeg", "image/png"]
                        }
                    },
                    "object": "model_capabilities",
                    "supports": {
                        "adaptive_thinking": false,
                        "max_thinking_budget": 0,
                        "min_thinking_budget": 0,
                        "parallel_tool_calls": true,
                        "reasoning_effort": [],
                        "streaming": true,
                        "structured_outputs": true,
                        "tool_calls": true,
                        "vision": true
                    },
                    "tokenizer": "o200k_base",
                    "type": "chat"
                },
                "id": "gpt-4.1",
                "model_picker_category": "versatile",
                "model_picker_enabled": true,
                "name": "GPT-4.1",
                "object": "model",
                "policy": {
                    "state": "enabled",
                    "terms": "Model terms"
                },
                "preview": false,
                "supported_endpoints": ["/chat/completions"],
                "vendor": "OpenAI",
                "version": "gpt-4.1"
            }
        ]
    }"#;

    #[test]
    fn test_parse_copilot_models_response() {
        let result: CopilotModelsResponse = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(result.data.len(), 2);
        let mut ids: Vec<&str> = result.data.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["auto-model-3", "gpt-4.1"]);
        assert_eq!(result.object, "list");
        assert_eq!(result.data[0].object, "model");
        assert_eq!(result.data[0].capabilities.object, "model_capabilities");
        assert!(result.data[0].capabilities.supports.streaming);
    }

    #[test]
    fn test_visible_models_filters_picker_disabled_entries() {
        let result: CopilotModelsResponse = serde_json::from_str(FIXTURE).unwrap();
        let visible = result.visible_models();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "gpt-4.1");
    }
}
