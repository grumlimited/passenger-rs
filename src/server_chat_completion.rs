use crate::server::{AppError, AppState, Server};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::log::{error, info};

/// OpenAI-compatible chat completion request
#[derive(Debug, Deserialize)]
pub struct OpenAIChatRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

/// OpenAI-compatible chat completion response
#[derive(Debug, Serialize)]
pub struct OpenAIChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: OpenAIUsage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CopilotMessage {
    pub role: String,
    pub content: String,
}

/// Copilot chat completion request
#[derive(Debug, Serialize)]
pub struct CopilotChatRequest {
    pub messages: Vec<CopilotMessage>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

/// Copilot chat completion response
#[derive(Debug, Deserialize)]
pub struct CopilotChatResponse {
    pub id: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<CopilotChoice>,
    #[serde(default)]
    pub usage: Option<CopilotUsage>,
}

#[derive(Debug, Deserialize)]
pub struct CopilotChoice {
    pub index: u32,
    pub message: CopilotMessage,
    pub finish_reason: String,
}

#[derive(Debug, Deserialize)]
pub struct CopilotUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct OpenAIChoice {
    pub index: u32,
    pub message: OpenAIMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub(crate) trait CoPilotChatCompletions {
    async fn chat_completions(
        state: State<Arc<AppState>>,
        request: Json<OpenAIChatRequest>,
    ) -> Result<Json<OpenAIChatResponse>, AppError>;
}

impl CoPilotChatCompletions for Server {
    async fn chat_completions(
        State(state): State<Arc<AppState>>,
        Json(request): Json<OpenAIChatRequest>,
    ) -> Result<Json<OpenAIChatResponse>, AppError> {
        info!(
            "Received chat completion request for model: {}",
            request.model
        );

        // Get a valid Copilot token
        let token = Self::get_token(state.clone()).await?;

        // Transform OpenAI request to Copilot format
        let copilot_request = CopilotChatRequest {
            messages: request
                .messages
                .iter()
                .map(|m| CopilotMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            model: request.model,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(request.stream),
        };

        // Forward request to Copilot API
        let copilot_url = format!("{}/chat/completions", state.config.copilot.api_base_url);

        let response = state
            .client
            .post(&copilot_url)
            .header("Authorization", format!("Bearer {}", token.token))
            .header("Copilot-Integration-Id", "vscode-chat")
            .header("Content-Type", "application/json")
            .json(&copilot_request)
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

        let copilot_response: CopilotChatResponse = response.json().await.map_err(|e| {
            error!("Failed to parse Copilot response: {}", e);
            AppError::InternalServerError(format!("Failed to parse Copilot response: {}", e))
        })?;

        // Transform Copilot response to OpenAI format
        let openai_response = OpenAIChatResponse {
            id: copilot_response.id,
            object: "chat.completion".to_string(),
            created: copilot_response.created,
            model: copilot_response.model,
            choices: copilot_response
                .choices
                .into_iter()
                .map(|c| OpenAIChoice {
                    index: c.index,
                    message: OpenAIMessage {
                        role: c.message.role,
                        content: c.message.content,
                    },
                    finish_reason: c.finish_reason,
                })
                .collect(),
            usage: copilot_response
                .usage
                .map(|u| OpenAIUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                })
                .unwrap_or(OpenAIUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                }),
        };

        info!("Successfully processed chat completion request");
        Ok(Json(openai_response))
    }
}
