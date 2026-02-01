use crate::server::{AppError, AppState, Server};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::log::{error, info};

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenAIModelsResponse {
    #[serde(default)]
    pub data: Vec<OpenAIModel>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenAIModel {
    pub id: String,
    pub object: String,
    pub created: u32,
    pub owned_by: String,
}

impl From<CopilotModelsResponse> for OpenAIModelsResponse {
    fn from(value: CopilotModelsResponse) -> Self {
        Self {
            data: value.models.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<CopilotModel> for OpenAIModel {
    fn from(value: CopilotModel) -> Self {
        Self {
            id: value.id,
            object: "model".to_string(),
            created: 1687882411,
            owned_by: value.publisher,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
// #[serde(transparent)]
pub struct CopilotModelsResponse {
    #[serde(default)]
    pub models: Vec<CopilotModel>,
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
    pub limits: Vec<CopilotModelLimits>,
    pub rate_limit_tier: String,
    pub supported_input_modalities: Vec<String>,
    pub supported_output_modalities: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotModelLimits {
    max_input_tokens: u64,
    max_output_tokens: u64,
}

pub(crate) trait CoPilotListModels {
    // List available models (OpenAI-compatible)
    async fn list_models(
        state: State<Arc<AppState>>,
    ) -> Result<Json<OpenAIModelsResponse>, AppError>;
}

impl CoPilotListModels for Server {
    /// List available models (OpenAI-compatible)
    async fn list_models(
        State(state): State<Arc<AppState>>,
    ) -> Result<Json<OpenAIModelsResponse>, AppError> {
        info!("Received list models request");

        // Get a valid Copilot token
        let token = Self::get_token(state.clone()).await?;

        let copilot_url = format!("{}/catalog/models", state.config.copilot.api_base_url);

        let response = state
            .client
            .get(&copilot_url)
            .header("Authorization", format!("Bearer {}", token.token))
            .header("Copilot-Integration-Id", "vscode-chat")
            .header("Content-Type", "application/json")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| {
                error!("Failed to send request to Copilot API: {}", e);
                AppError::InternalServerError(format!(
                    "Failed to communicate with Copilot API: {}",
                    e
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Copilot API returned error: {} - {}", status, error_text);
            return Err(AppError::InternalServerError(format!(
                "Copilot API error: {} - {}",
                status, error_text
            )));
        }

        let copilot_response: CopilotModelsResponse = response.json().await.map_err(|e| {
            error!("Failed to parse Copilot response: {}", e);
            AppError::InternalServerError(format!("Failed to parse Copilot response: {}", e))
        })?;

        info!("Successfully processed model request");
        Ok(Json(copilot_response.into()))
    }
}
