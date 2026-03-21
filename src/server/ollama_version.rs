//! Handler for `GET /api/version`.
//!
//! Returns the current passenger-rs version in Ollama's version response format.

use axum::Json;

use crate::ollama::version::OllamaVersionResponse;

pub async fn handler() -> Json<OllamaVersionResponse> {
    Json(OllamaVersionResponse::current())
}
