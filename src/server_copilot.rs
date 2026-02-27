use crate::auth::CopilotTokenResponse;
use crate::server::{AppError, AppState, Server};
use reqwest::{IntoUrl, Response};
use serde::Serialize;
use std::sync::Arc;
use tracing::log::error;

pub(crate) trait CopilotIntegration {
    async fn forward_prompt<U, T>(
        state: Arc<AppState>,
        token: CopilotTokenResponse,
        url: U,
        json: &T,
    ) -> Result<Response, AppError>
    where
        U: IntoUrl,
        T: Serialize + Sized;

    async fn handle_errors(response: Response) -> Result<axum::response::Response, AppError>;
}

impl CopilotIntegration for Server {
    async fn forward_prompt<U, T>(
        state: Arc<AppState>,
        token: CopilotTokenResponse,
        url: U,
        json: &T,
    ) -> Result<Response, AppError>
    where
        U: IntoUrl,
        T: Serialize + Sized,
    {
        state
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", token.token))
            .header("Copilot-Integration-Id", "vscode-chat")
            .header("Content-Type", "application/json")
            .json(&json)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to send request to Copilot API: {}", e);
                AppError::InternalServerError(format!(
                    "Failed to communicate with Copilot API: {}",
                    e
                ))
            })
    }

    async fn handle_errors(response: Response) -> Result<axum::response::Response, AppError> {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        error!("Copilot API returned error: {} - {}", status, error_text);
        Err(AppError::InternalServerError(format!(
            "Copilot API error: {} - {}",
            status, error_text
        )))
    }
}
