use crate::copilot::models::CopilotModelsResponse;
use crate::server::{AppError, AppState, Server};
use axum::{Json, extract::State};
use serde::Serialize;
use std::sync::Arc;
use tracing::log::{error, info};

#[derive(Serialize)]
pub struct OllamaTagsResponse {
    pub models: Vec<OllamaModel>,
}

#[derive(Serialize)]
pub struct OllamaModel {
    pub name: String,
    pub model: String,
    pub modified_at: String,
    pub size: u64,
    pub digest: String,
    pub details: OllamaModelDetails,
}

#[derive(Serialize)]
pub struct OllamaModelDetails {
    pub parent_model: String,
    pub format: String,
    pub family: String,
    pub families: Vec<String>,
    pub parameter_size: String,
    pub quantization_level: String,
}

#[allow(async_fn_in_trait)]
pub trait OllamaTags {
    async fn ollama_tags(state: State<Arc<AppState>>)
    -> Result<Json<OllamaTagsResponse>, AppError>;
}

impl OllamaTags for Server {
    async fn ollama_tags(
        State(state): State<Arc<AppState>>,
    ) -> Result<Json<OllamaTagsResponse>, AppError> {
        info!("Received ollama tags request");

        let token = Self::get_token(state.clone()).await?;

        let response = state
            .client
            .get(&state.config.github.copilot_models_url)
            .header("Authorization", format!("Bearer {}", token.token))
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

        let models = copilot_response
            .models
            .into_iter()
            .map(|m| OllamaModel {
                name: m.id.clone(),
                model: m.id,
                modified_at: "1970-01-01T00:00:00Z".to_string(),
                size: 0,
                digest: String::new(),
                details: OllamaModelDetails {
                    parent_model: String::new(),
                    format: "api".to_string(),
                    family: m.family.clone(),
                    families: vec![m.family],
                    parameter_size: String::new(),
                    quantization_level: String::new(),
                },
            })
            .collect();

        info!("Successfully processed ollama tags request");
        Ok(Json(OllamaTagsResponse { models }))
    }
}
