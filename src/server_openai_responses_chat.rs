use crate::copilot::CopilotChatRequest;
use crate::copilot::CopilotChatResponse;
use crate::openai::responses::models::prompt_request::PromptRequest;
use crate::openai::responses::models::prompt_response::CompletionResponse;
use crate::server::{AppError, AppState, Server};
use crate::server_copilot::CopilotIntegration;
use axum::{extract::State, Json};
use std::sync::Arc;
use tracing::debug;
use tracing::log::{error, info};

pub(crate) trait OpenAiResponsesEndpoint: CopilotIntegration {
    async fn openai_responses_chat(
        state: State<Arc<AppState>>,
        request: Json<PromptRequest>,
    ) -> Result<Json<CompletionResponse>, AppError>;
}

impl OpenAiResponsesEndpoint for Server {
    async fn openai_responses_chat(
        State(state): State<Arc<AppState>>,
        request: Json<PromptRequest>,
    ) -> Result<Json<CompletionResponse>, AppError> {
        let mut request = request.0;

        debug!(
            "original_openai_request:\n{}",
            serde_json::to_string_pretty(&request).unwrap()
        );

        // Get a valid Copilot token
        let token = Self::get_token(state.clone()).await?;

        // Transform OpenAI request to Copilot format
        let copilot_request: CopilotChatRequest = request.into();

        debug!(
            "copilot_request:\n{}",
            serde_json::to_string_pretty(&copilot_request).unwrap()
        );

        // Forward request to Copilot API
        let copilot_url = format!("{}/chat/completions", state.config.copilot.api_base_url);

        let response = Self::forward_prompt(state, token, copilot_url, &copilot_request).await?;

        let status = response.status();
        if !status.is_success() {
            return Self::handle_errors(response).await;
        }

        let copilot_response: CopilotChatResponse = response.json().await.map_err(|e| {
            error!("Failed to parse Copilot response: {}", e);
            AppError::InternalServerError(format!("Failed to parse Copilot response: {}", e))
        })?;

        debug!(
            "copilot_response:\n{}",
            serde_json::to_string_pretty(&copilot_response).unwrap()
        );

        // Transform Copilot response to Ollama format
        let openai_response: CompletionResponse = copilot_response.into();

        debug!(
            "openai_response:\n{}",
            serde_json::to_string_pretty(&openai_response).unwrap()
        );

        info!("Successfully processed Ollama chat request");

        Ok(Json(openai_response))
    }
}
