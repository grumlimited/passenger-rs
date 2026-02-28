// use passenger_rs::auth::CopilotTokenResponse;
use crate::auth::CopilotTokenResponse;
use crate::config::Config;
use crate::token_manager;

pub mod chat_completion;
pub mod copilot;
pub mod list_models;
pub mod ollama_chat;
pub mod ollama_tags;
pub mod ollama_version;
pub mod openai_responses_chat;

use self::chat_completion::*;
use self::list_models::*;
use self::ollama_chat::*;
use self::ollama_tags::*;
use self::ollama_version::*;
use self::openai_responses_chat::*;
use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use reqwest::Client;
use std::sync::Arc;
use tracing::log::error;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub client: Client,
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// Custom error type for API responses
#[derive(Debug)]
pub enum AppError {
    Unauthorized(String),
    InternalServerError(String),
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
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

pub struct Server {
    pub addr: String,
    pub router: Router,
}

impl Server {
    pub fn new(config: &Config) -> Self {
        let client = Client::new();
        let state = AppState {
            config: config.clone(),
            client,
        };
        let state = Arc::new(state);

        let app = Self::create_router(state.clone());
        let addr = format!("{}:{}", config.server.host, config.server.port);

        Self { addr, router: app }
    }

    /// Create the Axum router
    fn create_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/v1/chat/completions", post(Self::chat_completions))
            // Ollama-compatible routes: standard /api/... paths
            .route("/api/chat", post(Self::ollama_chat))
            .route("/api/tags", get(Self::ollama_tags))
            .route("/api/version", get(Self::ollama_version))
            // Ollama-compatible routes: legacy /v1/api/... paths
            .route("/v1/api/chat", post(Self::ollama_chat))
            .route("/v1/api/tags", get(Self::ollama_tags))
            .route("/v1/api/version", get(Self::ollama_version))
            .route("/v1/models", get(Self::list_models))
            .route("/v1/responses", post(Self::openai_responses_chat))
            .route("/health", get(health_check))
            .with_state(state)
    }

    pub(crate) async fn get_token(state: Arc<AppState>) -> Result<CopilotTokenResponse, AppError> {
        token_manager::get_valid_token(&state.config, &state.client)
            .await
            .map_err(|e| {
                error!("Failed to get valid token: {}", e);
                AppError::Unauthorized(
                    "No valid authentication. Please run with --login".to_string(),
                )
            })
    }
}
