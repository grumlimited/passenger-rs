//! Copilot models API — types for the `models.dev` JSON response.
//!
//! The endpoint (`github.copilot_models_url` in config) returns:
//! `{ "github-copilot": { "models": { "<id>": { ... }, ... } } }`

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct CopilotModelsResponse {
    #[serde(default)]
    pub models: Vec<CopilotModel>,
}

/// Custom deserializer: unwraps the `github-copilot.models` map into a flat `Vec`.
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
    use super::*;

    const FIXTURE: &str = r#"{
        "github-copilot": {
            "models": {
                "gpt-4.1": {
                    "id": "gpt-4.1",
                    "name": "GPT-4.1",
                    "family": "gpt",
                    "attachment": true,
                    "reasoning": false,
                    "tool_call": true,
                    "open_weights": false,
                    "modalities": { "input": ["text", "image"], "output": ["text"] },
                    "limit": { "context": 1048576, "output": 32768 }
                },
                "gpt-4o": {
                    "id": "gpt-4o",
                    "name": "GPT-4o",
                    "family": "gpt",
                    "attachment": true,
                    "reasoning": false,
                    "tool_call": true,
                    "open_weights": false,
                    "modalities": { "input": ["text", "image"], "output": ["text"] },
                    "limit": { "context": 128000, "output": 16384 }
                }
            }
        }
    }"#;

    #[test]
    fn test_parse_copilot_models_response() {
        let result: CopilotModelsResponse = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(result.models.len(), 2);
        let mut ids: Vec<&str> = result.models.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["gpt-4.1", "gpt-4o"]);
    }
}
