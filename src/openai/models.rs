//! OpenAI-compatible models list — `GET /v1/models` response types.

use serde::{Deserialize, Serialize};

use crate::copilot::models::{CopilotModel, CopilotModelsResponse};

/// Response body for `GET /v1/models`.
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIModelsResponse {
    pub object: String,
    pub data: Vec<OpenAIModel>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIModel {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

impl From<CopilotModelsResponse> for OpenAIModelsResponse {
    fn from(value: CopilotModelsResponse) -> Self {
        Self {
            object: "list".to_string(),
            data: value.data.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<CopilotModel> for OpenAIModel {
    fn from(value: CopilotModel) -> Self {
        Self {
            id: value.id,
            object: "model".to_string(),
            created: 1687882411,
            owned_by: value.capabilities.family,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copilot::models::{
        CopilotModel, CopilotModelCapabilitiesLimits, CopilotModelCapabilitiesSupports,
    };

    fn make_copilot_model(id: &str, family: &str) -> CopilotModel {
        CopilotModel {
            id: id.to_string(),
            name: id.to_string(),
            object: "model".to_string(),
            model_picker_enabled: true,
            model_picker_category: Some("versatile".to_string()),
            preview: false,
            supported_endpoints: vec!["/chat/completions".to_string()],
            vendor: Some("OpenAI".to_string()),
            version: Some(id.to_string()),
            policy: None,
            capabilities: crate::copilot::models::CopilotModelCapabilities {
                family: family.to_string(),
                limits: Some(CopilotModelCapabilitiesLimits::default()),
                object: "model_capabilities".to_string(),
                supports: CopilotModelCapabilitiesSupports::default(),
                tokenizer: Some("o200k_base".to_string()),
                model_type: "chat".to_string(),
            },
        }
    }

    #[test]
    fn test_from_copilot_models_response() {
        let copilot = CopilotModelsResponse {
            object: "list".to_string(),
            data: vec![
                make_copilot_model("gpt-4o", "gpt"),
                make_copilot_model("gpt-4.1", "gpt"),
            ],
        };
        let openai: OpenAIModelsResponse = copilot.into();
        assert_eq!(openai.object, "list");
        assert_eq!(openai.data.len(), 2);
        assert_eq!(openai.data[0].object, "model");
        assert_eq!(openai.data[0].owned_by, "gpt");
    }

    #[test]
    fn test_openai_model_fields() {
        let model: OpenAIModel = make_copilot_model("o4-mini", "openai").into();
        assert_eq!(model.id, "o4-mini");
        assert_eq!(model.object, "model");
        assert_eq!(model.owned_by, "openai");
    }
}
