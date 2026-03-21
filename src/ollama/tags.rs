//! Ollama `/api/tags` API — response types and conversion from Copilot models.
//!
//! Modelled from the official Ollama API docs:
//! https://github.com/ollama/ollama/blob/main/docs/api.md#list-local-models

use serde::Serialize;

use crate::copilot::models::{CopilotModel, CopilotModelsResponse};

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Response body from `GET /api/tags`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OllamaTagsResponse {
    pub models: Vec<OllamaModelInfo>,
}

/// A single model entry in the `/api/tags` response.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OllamaModelInfo {
    /// Display name used as the model identifier (e.g. `"gpt-4o"`).
    pub name: String,
    /// Same as `name` — Ollama uses this as the pull reference.
    pub model: String,
    /// ISO 8601 timestamp — hardcoded to epoch since Copilot has no pull date.
    pub modified_at: String,
    /// Model size in bytes — not available from Copilot; always `0`.
    pub size: u64,
    /// Opaque digest — derived from the model id.
    pub digest: String,
    pub details: OllamaModelDetails,
}

/// Additional metadata about a model.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OllamaModelDetails {
    pub parent_model: String,
    /// Always `"api"` — Copilot models are remote API models, not local GGUF files.
    pub format: String,
    /// The model family (e.g. `"gpt"`, `"claude"`).
    pub family: String,
    pub families: Vec<String>,
    pub parameter_size: String,
    pub quantization_level: String,
}

// ---------------------------------------------------------------------------
// CopilotModel → OllamaModelInfo
// ---------------------------------------------------------------------------

impl From<CopilotModel> for OllamaModelInfo {
    fn from(m: CopilotModel) -> Self {
        OllamaModelInfo {
            name: m.id.clone(),
            model: m.id.clone(),
            modified_at: "1970-01-01T00:00:00Z".to_string(),
            size: 0,
            digest: format!("sha256:{}", m.id),
            details: OllamaModelDetails {
                parent_model: String::new(),
                format: "api".to_string(),
                family: m.family.clone(),
                families: vec![m.family],
                parameter_size: String::new(),
                quantization_level: String::new(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// CopilotModelsResponse → OllamaTagsResponse
// ---------------------------------------------------------------------------

impl From<CopilotModelsResponse> for OllamaTagsResponse {
    fn from(resp: CopilotModelsResponse) -> Self {
        let mut models: Vec<OllamaModelInfo> =
            resp.models.into_iter().map(OllamaModelInfo::from).collect();
        // Sort by name for a stable, predictable response order.
        models.sort_by(|a, b| a.name.cmp(&b.name));
        OllamaTagsResponse { models }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copilot::models::{CopilotModel, CopilotModelLimit, CopilotModelModalities};

    fn make_model(id: &str, family: &str) -> CopilotModel {
        CopilotModel {
            id: id.to_string(),
            name: id.to_string(),
            family: family.to_string(),
            tool_call: false,
            reasoning: false,
            attachment: false,
            open_weights: false,
            modalities: CopilotModelModalities::default(),
            limit: CopilotModelLimit::default(),
        }
    }

    #[test]
    fn test_copilot_model_converts_to_ollama_model_info() {
        let m = make_model("gpt-4o", "gpt");
        let info: OllamaModelInfo = m.into();
        assert_eq!(info.name, "gpt-4o");
        assert_eq!(info.model, "gpt-4o");
        assert_eq!(info.modified_at, "1970-01-01T00:00:00Z");
        assert_eq!(info.size, 0);
        assert_eq!(info.digest, "sha256:gpt-4o");
        assert_eq!(info.details.format, "api");
        assert_eq!(info.details.family, "gpt");
        assert_eq!(info.details.families, vec!["gpt"]);
    }

    #[test]
    fn test_copilot_models_response_converts_to_ollama_tags_response() {
        let resp = CopilotModelsResponse {
            models: vec![
                make_model("gpt-4o", "gpt"),
                make_model("claude-3", "claude"),
            ],
        };
        let tags: OllamaTagsResponse = resp.into();
        assert_eq!(tags.models.len(), 2);
        // Should be sorted by name
        assert_eq!(tags.models[0].name, "claude-3");
        assert_eq!(tags.models[1].name, "gpt-4o");
    }

    #[test]
    fn test_empty_models_list() {
        let resp = CopilotModelsResponse { models: vec![] };
        let tags: OllamaTagsResponse = resp.into();
        assert!(tags.models.is_empty());
    }

    #[test]
    fn test_serializes_to_json() {
        let resp = CopilotModelsResponse {
            models: vec![make_model("llama3.2", "llama")],
        };
        let tags: OllamaTagsResponse = resp.into();
        let json = serde_json::to_value(&tags).unwrap();
        let model = &json["models"][0];
        assert_eq!(model["name"], "llama3.2");
        assert_eq!(model["size"], 0);
        assert_eq!(model["details"]["format"], "api");
        assert_eq!(model["details"]["family"], "llama");
    }
}
