use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize)]
pub struct CopilotModelsResponse {
    #[serde(default)]
    pub models: Vec<CopilotModel>,
}

impl<'de> Deserialize<'de> for CopilotModelsResponse {
    fn deserialize<D>(deserializer: D) -> Result<CopilotModelsResponse, D::Error>
    where
        D: Deserializer<'de>,
    {
        let models = Vec::<CopilotModel>::deserialize(deserializer)?;

        Ok(CopilotModelsResponse { models })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotModel {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub registry: String,
    pub summary: String,
    pub html_url: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub limits: CopilotModelLimits,
    pub rate_limit_tier: String,
    pub supported_input_modalities: Vec<String>,
    pub supported_output_modalities: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotModelLimits {
    max_input_tokens: u64,
    max_output_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use crate::copilot::models::CopilotModelsResponse;

    #[test]
    fn test_parse_json_models_response() {
        let json = include_str!("../../resources/models_response.json");

        let json = serde_json::from_str::<CopilotModelsResponse>(json).unwrap();

        assert_eq!(2, json.models.len())
    }
}
