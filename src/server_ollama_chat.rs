use crate::server::{AppError, AppState, Server};
use crate::server_chat_completion::{
    CopilotChatRequest, CopilotChatResponse, CopilotMessage, OpenAIChatRequest,
};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::log::{error, info};

/// Ollama-compatible chat response
#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: OllamaMessage,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_duration: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaToolCall {
    pub function: OllamaFunction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub arguments: serde_json::Value,
}

pub(crate) trait OllamaChatEndpoint {
    async fn ollama_chat(
        state: State<Arc<AppState>>,
        request: Json<OpenAIChatRequest>,
    ) -> Result<Json<OllamaChatResponse>, AppError>;
}

impl OllamaChatEndpoint for Server {
    async fn ollama_chat(
        State(state): State<Arc<AppState>>,
        Json(request): Json<OpenAIChatRequest>,
    ) -> Result<Json<OllamaChatResponse>, AppError> {
        info!("Received Ollama chat request for model: {}", request.model);

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
                    padding: None,
                })
                .collect(),
            model: request.model.clone(),
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

        // Transform Copilot response to Ollama format
        let ollama_response = transform_to_ollama_response(copilot_response, request.model)?;

        info!("Successfully processed Ollama chat request");
        Ok(Json(ollama_response))
    }
}

/// Transform CopilotChatResponse to OllamaChatResponse
fn transform_to_ollama_response(
    copilot: CopilotChatResponse,
    model: String,
) -> Result<OllamaChatResponse, AppError> {
    // Get the first choice (Ollama format returns a single message)
    let choice = copilot.choices.first().ok_or_else(|| {
        AppError::InternalServerError("No choices in Copilot response".to_string())
    })?;

    // Map finish_reason to done_reason
    let done_reason = match choice.finish_reason.as_str() {
        "stop" => Some("stop".to_string()),
        "length" => Some("length".to_string()),
        _ => Some(choice.finish_reason.clone()),
    };

    // Create timestamp in RFC3339 format
    let created_at = if let Some(created) = copilot.created {
        // Convert Unix timestamp to RFC3339
        chrono::DateTime::from_timestamp(created as i64, 0)
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339()
    } else {
        chrono::Utc::now().to_rfc3339()
    };

    // Calculate durations and counts from usage if available
    let (prompt_eval_count, eval_count) = if let Some(ref usage) = copilot.usage {
        (Some(usage.prompt_tokens), Some(usage.completion_tokens))
    } else {
        (None, None)
    };

    Ok(OllamaChatResponse {
        model,
        created_at,
        message: OllamaMessage {
            role: choice.message.role.clone(),
            content: choice.message.content.clone(),
            thinking: None,
            tool_calls: None,
            images: None,
        },
        done: true,
        done_reason,
        total_duration: None,
        load_duration: None,
        prompt_eval_count,
        prompt_eval_duration: None,
        eval_count,
        eval_duration: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_chat_completion::{CopilotChoice, CopilotUsage};

    #[test]
    fn test_transform_to_ollama_response() {
        let copilot_response = CopilotChatResponse {
            id: "test-id".to_string(),
            created: Some(1699334516),
            model: "gpt-4".to_string(),
            system_fingerprint: Some("fp_test".to_string()),
            choices: vec![CopilotChoice {
                index: Some(0),
                message: CopilotMessage {
                    role: "assistant".to_string(),
                    content: "Hello, World!".to_string(),
                    padding: None,
                },
                finish_reason: "stop".to_string(),
            }],
            usage: Some(CopilotUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        };

        let result = transform_to_ollama_response(copilot_response, "gpt-4".to_string());
        assert!(result.is_ok(), "Failed to transform: {:?}", result.err());

        let ollama = result.unwrap();
        assert_eq!(ollama.model, "gpt-4");
        assert_eq!(ollama.message.role, "assistant");
        assert_eq!(ollama.message.content, "Hello, World!");
        assert!(ollama.done);
        assert_eq!(ollama.done_reason, Some("stop".to_string()));
        assert_eq!(ollama.prompt_eval_count, Some(10));
        assert_eq!(ollama.eval_count, Some(5));
    }

    #[test]
    fn test_transform_without_usage() {
        let copilot_response = CopilotChatResponse {
            id: "test-id".to_string(),
            created: None,
            model: "gpt-4".to_string(),
            system_fingerprint: Some("fp_test".to_string()),
            choices: vec![CopilotChoice {
                index: Some(0),
                message: CopilotMessage {
                    role: "assistant".to_string(),
                    content: "Test".to_string(),
                    padding: None,
                },
                finish_reason: "length".to_string(),
            }],
            usage: None,
        };

        let result = transform_to_ollama_response(copilot_response, "gpt-4".to_string());
        assert!(result.is_ok());

        let ollama = result.unwrap();
        assert_eq!(ollama.done_reason, Some("length".to_string()));
        assert_eq!(ollama.prompt_eval_count, None);
        assert_eq!(ollama.eval_count, None);
    }

    #[test]
    fn test_parse_ollama_response() {
        // Test parsing the expected JSON structure
        let json = include_str!("resources/ollama_chat_response.json");
        let result = serde_json::from_str::<OllamaChatResponse>(json);

        assert!(
            result.is_ok(),
            "Failed to parse Ollama response: {:?}",
            result.err()
        );
    }
}
