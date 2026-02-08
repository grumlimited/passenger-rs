use crate::copilot::models::CopilotModelsResponse;
use crate::openai::completion::models::OpenAIModelsResponse;
use crate::server::{AppError, AppState, Server};
use axum::{extract::State, Json};
use std::sync::Arc;
use tracing::log::{error, info};

#[allow(async_fn_in_trait)]
pub trait CoPilotListModels {
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

        info!("Successfully processed model request");
        Ok(Json(copilot_response.into()))
    }
}
