use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

use crate::config::Config;
use crate::token_manager;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub client: Client,
}

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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIMessage {
    pub role: String,
    pub content: String,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CopilotMessage {
    pub role: String,
    pub content: String,
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

/// Create the Axum router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/health", get(health_check))
        .with_state(Arc::new(state))
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// List available models (OpenAI-compatible)
async fn list_models() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "object": "list",
        "data": [
            {
                "id": "gpt-4",
                "object": "model",
                "created": 1687882411,
                "owned_by": "github-copilot"
            },
            {
                "id": "gpt-3.5-turbo",
                "object": "model",
                "created": 1677610602,
                "owned_by": "github-copilot"
            }
        ]
    }))
}

/// Chat completions endpoint (OpenAI-compatible)
async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(request): Json<OpenAIChatRequest>,
) -> Result<Json<OpenAIChatResponse>, AppError> {
    info!("Received chat completion request for model: {}", request.model);

    // Get a valid Copilot token
    let token = token_manager::get_valid_token(&state.config, &state.client, None)
        .await
        .map_err(|e| {
            error!("Failed to get valid token: {}", e);
            AppError::Unauthorized("No valid authentication. Please run with --login".to_string())
        })?;

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
            AppError::InternalServerError(format!("Failed to communicate with Copilot API: {}", e))
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
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
        usage: copilot_response.usage.map(|u| OpenAIUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }).unwrap_or(OpenAIUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        }),
    };

    info!("Successfully processed chat completion request");
    Ok(Json(openai_response))
}

/// Custom error type for API responses
#[derive(Debug)]
pub enum AppError {
    Unauthorized(String),
    InternalServerError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(serde_json::json!({
            "error": {
                "message": error_message,
                "type": "server_error",
            }
        }));

        (status, body).into_response()
    }
}
