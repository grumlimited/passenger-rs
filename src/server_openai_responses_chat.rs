use crate::copilot::CopilotChatRequest;
use crate::copilot::CopilotChatResponse;
use crate::openai::responses::models::prompt_request::PromptRequest;
use crate::openai::responses::models::prompt_response::CompletionResponse;
use crate::server::{AppError, AppState, Server};
use crate::server_copilot::CopilotIntegration;
use axum::{extract::State, Json};
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;
use tracing::log::{error, info};

pub(crate) trait OpenAiResponsesEndpoint: CopilotIntegration {
    async fn openai_responses_chat(
        state: State<Arc<AppState>>,
        request_as_text: String,
    ) -> Result<Json<CompletionResponse>, AppError>;
}

impl OpenAiResponsesEndpoint for Server {
    async fn openai_responses_chat(
        State(state): State<Arc<AppState>>,
        request_as_text: String,
    ) -> Result<Json<CompletionResponse>, AppError> {
        /*
         * We are not destructuring directly into a Json<PromptRequest> because the openai request
         * coming from Rig contains 2 "role" key within the input["role" == "user"].
         * It is causing serde to fail on doing serde_json::from_str::<PromptRequest>(&request_as_text), yet
         * it is somewhat more laxist when parsing it into a json_serde::Value.
         */
        let request_as_value: Value = serde_json::from_str(&request_as_text).unwrap();
        debug!(
            "request_as_value:\n{}",
            serde_json::to_string_pretty(&request_as_value).unwrap()
        );

        let request: PromptRequest = serde_json::from_value(request_as_value).unwrap();

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
