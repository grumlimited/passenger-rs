//! Ollama `/api/version` API — response type.
//!
//! Modelled from the official Ollama API docs:
//! https://github.com/ollama/ollama/blob/main/docs/api.md#show-ollama-version

use serde::Serialize;

/// Response body from `GET /api/version`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OllamaVersionResponse {
    pub version: String,
}

impl OllamaVersionResponse {
    /// Returns the current crate version as reported by Cargo.
    pub fn current() -> Self {
        OllamaVersionResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_non_empty() {
        let resp = OllamaVersionResponse::current();
        assert!(!resp.version.is_empty());
    }

    #[test]
    fn test_serializes_to_json() {
        let resp = OllamaVersionResponse {
            version: "1.2.3".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["version"], "1.2.3");
    }
}
