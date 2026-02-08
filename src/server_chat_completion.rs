use crate::server::{AppError, AppState, Server};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::log::{error, info};

/// Tool definition for function calling
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

/// Function definition with JSON schema for parameters
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

/// Tool choice specification
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum ToolChoice {
    String(String), // "auto", "none", "required"
    Specific {
        #[serde(rename = "type")]
        tool_type: String,
        function: ToolChoiceFunction,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolChoiceFunction {
    pub name: String,
}

/// Tool call made by the assistant
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// OpenAI-compatible chat completion request
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIChatRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Option<Vec<Tool>>,
    #[serde(default)]
    pub tool_choice: Option<ToolChoice>,
}

impl OpenAIChatRequest {
    pub fn ids_present(&self) -> bool {
        let all_tool_messages_have_ids = self
            .messages
            .iter()
            .filter(|t| t.role == "tool")
            .map(|m|
                {
                    m.tool_call_id.is_some() && m.tool_call_id.clone().unwrap().len() > 0 })
            .fold(true, |a, b| a && b);

        let all_tool_calls_have_ids = self
            .messages
            .iter()
            .filter(|t| t.role == "assistant")
            .filter(|t| t.tool_calls.is_some())
            .map(|t| t.tool_calls.clone().unwrap())
            .flatten()
            .collect::<Vec<ToolCall>>()
            .iter()
            .map(|tc| tc.id.is_some() && tc.id.clone().unwrap().len() > 0)
            .fold(true, |a, b| a && b);

        all_tool_messages_have_ids && all_tool_calls_have_ids
    }

    pub fn normalize_tools(&mut self) {
        if self.ids_present() {
            ()
        } else {
            self.messages
                .iter_mut()
                .filter(|t| t.role == "tool")
                .enumerate()
                .for_each(|(i, t)| t.tool_call_id = Some(format!("{}", i)));

            self.messages
                .iter_mut()
                .filter(|t| t.role == "assistant")
                .filter(|t| t.tool_calls.is_some())
                .for_each(|t| match t.tool_calls {
                    Some(ref mut tc) => tc.iter_mut().enumerate().for_each(|(i, v)| {
                        v.id = Some(format!("{}", i));
                    }),
                    _ => {}
                });
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default)]
    pub padding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}

/// Copilot chat completion response
#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotChatResponse {
    pub id: String,
    #[serde(default)]
    pub created: Option<u64>,
    pub model: String,
    /// Optional system fingerprint (GitHub Copilot may omit this field)
    #[allow(dead_code)]
    // pub system_fingerprint: Option<String>,
    pub choices: Vec<CopilotChoice>,
    #[serde(default)]
    pub usage: Option<CopilotUsage>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotChoice {
    /// Optional index (defaults to position in array if not provided)
    pub index: Option<u32>,
    pub message: CopilotMessage,
    pub finish_reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CopilotUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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
        println!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF");
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
                    padding: None,
                    tool_calls: m.tool_calls.clone(),
                    tool_call_id: m.tool_call_id.clone(),
                    name: m.name.clone(),
                })
                .collect(),
            model: request.model,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(request.stream),
            tools: request.tools,
            tool_choice: request.tool_choice,
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

        let since_the_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should go forward");

        // Transform Copilot response to OpenAI format
        let openai_response = OpenAIChatResponse {
            id: copilot_response.id,
            object: "chat.completion".to_string(),
            // IMPORTANT: Handle optional `created` field from GitHub Copilot API
            // - GitHub Copilot's response may omit the `created` field
            // - OpenAI's API spec requires `created` as a mandatory integer (Unix timestamp)
            // - We default to the current timestamp if Copilot doesn't provide one
            created: copilot_response
                .created
                .unwrap_or(since_the_epoch.as_secs()),
            model: copilot_response.model,
            choices: copilot_response
                .choices
                .into_iter()
                .enumerate()
                .map(|(i, c)| OpenAIChoice {
                    // Use the index from Copilot if available, otherwise use position
                    index: c.index.unwrap_or(i as u32),
                    message: OpenAIMessage {
                        role: c.message.role,
                        content: c.message.content,
                        tool_calls: c.message.tool_calls,
                        tool_call_id: c.message.tool_call_id,
                        name: c.message.name,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_copilot_response_without_created() {
        // Test parsing a Copilot response without the optional 'created' field
        let json = include_str!("resources/chat_completions_response.json");
        let result = serde_json::from_str::<CopilotChatResponse>(json);

        assert!(
            result.is_ok(),
            "Failed to parse response: {:?}",
            result.err()
        );
        let response = result.unwrap();

        assert_eq!(response.id, "chatcmpl-D4RxeWmAd0lF5PPnCosBWQLmVXPlA");
        assert_eq!(response.model, "gpt-4.1-2025-04-14");
        assert!(response.created.is_none(), "Expected created to be None");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello, World!".to_string())
        );
    }

    #[test]
    fn test_parse_copilot_response_with_created() {
        // Test parsing a Copilot response with the optional 'created' field
        let json = r#"{
            "id": "test-id",
            "created": 1234567890,
            "model": "gpt-4",
            "system_fingerprint": "fp_test",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Test response"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let result = serde_json::from_str::<CopilotChatResponse>(json);

        assert!(
            result.is_ok(),
            "Failed to parse response: {:?}",
            result.err()
        );
        let response = result.unwrap();

        assert_eq!(response.id, "test-id");
        assert_eq!(response.created, Some(1234567890));
        assert_eq!(response.model, "gpt-4");
    }

    #[test]
    fn test_openai_response_always_has_created() {
        // Verify that OpenAI response always includes 'created' even when Copilot doesn't provide it
        let copilot_response = CopilotChatResponse {
            id: "test".to_string(),
            created: None, // Copilot doesn't provide it
            model: "gpt-4".to_string(),
            choices: vec![],
            usage: None,
        };

        let since_the_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should go forward");

        let openai_response = OpenAIChatResponse {
            id: copilot_response.id,
            object: "chat.completion".to_string(),
            created: copilot_response
                .created
                .unwrap_or(since_the_epoch.as_secs()),
            model: copilot_response.model,
            choices: vec![],
            usage: OpenAIUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
        };

        // Verify that 'created' is always populated in OpenAI response
        assert!(
            openai_response.created > 0,
            "OpenAI response must have a valid timestamp"
        );
    }

    #[test]
    fn test_index_fallback_to_position() {
        // Verify that when Copilot doesn't provide indices, we use array positions
        let copilot_response = CopilotChatResponse {
            id: "test".to_string(),
            created: Some(1234567890),
            model: "gpt-4".to_string(),
            choices: vec![
                CopilotChoice {
                    index: None, // No index provided
                    message: CopilotMessage {
                        role: "assistant".to_string(),
                        content: Some("First response".to_string()),
                        padding: None,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    },
                    finish_reason: "stop".to_string(),
                },
                CopilotChoice {
                    index: Some(5), // Explicit index provided
                    message: CopilotMessage {
                        role: "assistant".to_string(),
                        content: Some("Second response".to_string()),
                        padding: None,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    },
                    finish_reason: "stop".to_string(),
                },
                CopilotChoice {
                    index: None, // No index provided
                    message: CopilotMessage {
                        role: "assistant".to_string(),
                        content: Some("Third response".to_string()),
                        padding: None,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    },
                    finish_reason: "stop".to_string(),
                },
            ],
            usage: None,
        };

        let since_the_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should go forward");

        let openai_response = OpenAIChatResponse {
            id: copilot_response.id.clone(),
            object: "chat.completion".to_string(),
            created: copilot_response
                .created
                .unwrap_or(since_the_epoch.as_secs()),
            model: copilot_response.model.clone(),
            choices: copilot_response
                .choices
                .into_iter()
                .enumerate()
                .map(|(i, c)| OpenAIChoice {
                    index: i as u32,
                    message: OpenAIMessage {
                        role: c.message.role,
                        content: c.message.content,
                        tool_calls: c.message.tool_calls,
                        tool_call_id: c.message.tool_call_id,
                        name: c.message.name,
                    },
                    finish_reason: c.finish_reason,
                })
                .collect(),
            usage: OpenAIUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
        };

        // Verify indices: 0 (from position), 5 (from Copilot), 2 (from position)
        assert_eq!(openai_response.choices.len(), 3);
        assert_eq!(
            openai_response.choices[0].index, 0,
            "First choice should use position 0"
        );
        assert_eq!(
            openai_response.choices[1].index, 5,
            "Second choice should use Copilot's index 5"
        );
        assert_eq!(
            openai_response.choices[2].index, 2,
            "Third choice should use position 2"
        );
    }

    #[test]
    fn test_openai_request_with_tools() {
        // Test that OpenAI requests with tools can be deserialized
        let json = r#"{
            "model": "gpt-4",
            "messages": [
                {
                    "role": "user",
                    "content": "What's the weather in San Francisco?"
                }
            ],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "description": "Get current weather",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "location": {
                                    "type": "string",
                                    "description": "City name"
                                }
                            },
                            "required": ["location"]
                        }
                    }
                }
            ],
            "tool_choice": "auto"
        }"#;

        let result = serde_json::from_str::<OpenAIChatRequest>(json);
        assert!(
            result.is_ok(),
            "Failed to parse request with tools: {:?}",
            result.err()
        );

        let request = result.unwrap();
        assert_eq!(request.model, "gpt-4");
        assert!(request.tools.is_some(), "Tools should be present");
        assert_eq!(request.tools.unwrap().len(), 1, "Should have one tool");
        assert!(
            request.tool_choice.is_some(),
            "Tool choice should be present"
        );
    }

    #[test]
    fn test_copilot_response_with_tool_calls() {
        // Test parsing a Copilot response that includes tool calls
        let json = r#"{
            "id": "test-id",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\": \"San Francisco\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let result = serde_json::from_str::<CopilotChatResponse>(json);
        assert!(
            result.is_ok(),
            "Failed to parse response with tool calls: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert_eq!(response.choices.len(), 1);
        assert!(
            response.choices[0].message.tool_calls.is_some(),
            "Tool calls should be present"
        );
        assert_eq!(
            response.choices[0]
                .message
                .tool_calls
                .as_ref()
                .unwrap()
                .len(),
            1,
            "Should have one tool call"
        );
    }
}
