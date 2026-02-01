// use passenger_rs::auth::CopilotTokenResponse;
use crate::auth::CopilotTokenResponse;
use crate::config::Config;
use crate::server_chat_completion::*;
use crate::server_list_models::*;
use crate::{server, token_manager};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
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

pub struct Server {
    pub addr: String,
    pub router: Router,
}

impl Server {
    pub fn new(config: &Config) -> Self {
        let client = Client::new();
        let state = server::AppState {
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
            // .route("/v1/models", get(Self::list_models))
            .route("/v1/models", get(Self::list_models))
            .route("/health", get(health_check))
            .with_state(state)
    }

    pub(crate) async fn get_token(state: Arc<AppState>) -> Result<CopilotTokenResponse, AppError> {
        token_manager::get_valid_token(&state.config, &state.client, None)
            .await
            .map_err(|e| {
                error!("Failed to get valid token: {}", e);
                AppError::Unauthorized(
                    "No valid authentication. Please run with --login".to_string(),
                )
            })
    }
}
