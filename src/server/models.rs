//! Handler for `GET /v1/models`.
//!
//! Fetches the model list from `github.copilot_models_url` (models.dev API),
//! deserializes the Copilot-shaped JSON, and returns an OpenAI-compatible
//! `{ "object": "list", "data": [...] }` response.

use axum::{Json, extract::State};
use std::sync::Arc;
use tracing::{error, info};

use crate::copilot::models::CopilotModelsResponse;
use crate::openai::models::OpenAIModelsResponse;

use super::{AppError, AppState, Server};

pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<OpenAIModelsResponse>, AppError> {
    info!("Received list models request");

    let token = Server::get_token(state.clone()).await?;

    let response = state
        .client
        .get(&state.config.github.copilot_models_url)
        .bearer_auth(&token.token)
        .header("Copilot-Integration-Id", "vscode-chat")
        .send()
        .await
        .map_err(|e| {
            error!("Failed to fetch models from Copilot API: {}", e);
            AppError::InternalServerError(format!("Failed to communicate with Copilot API: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!("Copilot models API returned {}: {}", status, body);
        return Err(AppError::InternalServerError(format!(
            "Copilot API error {}: {}",
            status, body
        )));
    }

    let body = response.text().await.map_err(|e| {
        error!("Failed to read Copilot models response body: {}", e);
        AppError::InternalServerError(format!(
            "Failed to read Copilot models response body: {}",
            e
        ))
    })?;

    let copilot_response: CopilotModelsResponse = serde_json::from_str(&body).map_err(|e| {
        error!("Failed to parse Copilot models response: {}", e);
        AppError::InternalServerError(format!("Failed to parse Copilot models response: {}", e))
    })?;

    let visible_models = copilot_response.visible_models();

    info!("Successfully fetched {} models", visible_models.len());
    Ok(Json(
        CopilotModelsResponse {
            data: visible_models,
            object: "list".to_string(),
        }
        .into(),
    ))
}
