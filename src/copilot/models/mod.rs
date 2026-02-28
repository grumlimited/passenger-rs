use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct CopilotModelsResponse {
    #[serde(default)]
    pub models: Vec<CopilotModel>,
}

/// Deserialize from the models.dev API shape:
/// { "github-copilot": { "models": { "<id>": { ... }, ... } } }
impl<'de> Deserialize<'de> for CopilotModelsResponse {
    fn deserialize<D>(deserializer: D) -> Result<CopilotModelsResponse, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Root {
            #[serde(rename = "github-copilot")]
            github_copilot: GithubCopilot,
        }

        #[derive(Deserialize)]
        struct GithubCopilot {
            models: HashMap<String, CopilotModel>,
        }

        let root = Root::deserialize(deserializer)?;
        let models = root.github_copilot.models.into_values().collect();

        Ok(CopilotModelsResponse { models })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotModel {
    pub id: String,
    pub name: String,
    pub family: String,
    #[serde(default)]
    pub tool_call: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub attachment: bool,
    #[serde(default)]
    pub open_weights: bool,
    #[serde(default)]
    pub modalities: CopilotModelModalities,
    #[serde(default)]
    pub limit: CopilotModelLimit,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CopilotModelModalities {
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub output: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CopilotModelLimit {
    #[serde(default)]
    pub context: u64,
    #[serde(default)]
    pub output: u64,
}

#[cfg(test)]
mod tests {
    use crate::copilot::models::CopilotModelsResponse;

    #[test]
    fn test_parse_json_models_response() {
        let json = include_str!("../../resources/models_response.json");

        let result = serde_json::from_str::<CopilotModelsResponse>(json).unwrap();

        assert_eq!(2, result.models.len())
    }
}
