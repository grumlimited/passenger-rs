use crate::server::Server;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct OllamaVersionResponse {
    pub version: String,
}

#[allow(async_fn_in_trait)]
pub trait OllamaVersion {
    async fn ollama_version() -> Json<OllamaVersionResponse>;
}

impl OllamaVersion for Server {
    async fn ollama_version() -> Json<OllamaVersionResponse> {
        Json(OllamaVersionResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
}
