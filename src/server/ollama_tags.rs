//! Handler for `GET /api/tags`.
//!
//! Fetches the model list from `github.copilot_models_url`, converts it to
//! an Ollama-compatible `{ "models": [...] }` response.

use axum::{Json, extract::State};
use std::sync::Arc;
use tracing::{error, info};

use crate::copilot::models::CopilotModelsResponse;
use crate::ollama::tags::OllamaTagsResponse;

use super::{AppError, AppState, Server};

pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<OllamaTagsResponse>, AppError> {
    info!("Received Ollama list tags request");

    let token = Server::get_token(state.clone()).await?;

    let response = state
        .client
        .get(&state.config.github.copilot_models_url)
        .bearer_auth(&token.token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
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

    let copilot_response: CopilotModelsResponse = response.json().await.map_err(|e| {
        error!("Failed to parse Copilot models response: {}", e);
        AppError::InternalServerError(format!("Failed to parse Copilot models response: {}", e))
    })?;

    info!("Successfully fetched {} models", copilot_response.models.len());
    Ok(Json(copilot_response.into()))
}
