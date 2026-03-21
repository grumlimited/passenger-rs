use crate::auth::CopilotTokenResponse;
use crate::config::Config;
use crate::token_manager;
use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use reqwest::Client;
use std::sync::Arc;
use tracing::log::error;

pub mod chat_completions;
pub mod models;
pub mod ollama_chat;
pub mod ollama_tags;
pub mod ollama_version;
pub mod responses;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub client: Client,
}

/// Custom error type for API responses
#[allow(dead_code)]
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

        let router = Self::create_router(state);
        let addr = format!("{}:{}", config.server.host, config.server.port);

        Self { addr, router }
    }

    fn create_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/v1/chat/completions", post(chat_completions::handler))
            .route("/v1/responses", post(responses::handler))
            .route("/v1/models", get(models::handler))
            .route("/api/chat", post(ollama_chat::handler))
            .route("/api/tags", get(ollama_tags::handler))
            .route("/api/version", get(ollama_version::handler))
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

async fn health_check() -> &'static str {
    "OK"
}
