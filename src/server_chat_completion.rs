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
    fn assistant_role() -> String {
        "assistant".to_string()
    }

    fn tool_role() -> String {
        "tool".to_string()
    }

    fn has_valid_id(id: &Option<String>) -> bool {
        id.as_ref().map_or(false, |s| !s.is_empty())
    }

    /// Checks if all tool-related messages already have IDs present.
    /// Returns true if IDs are present, meaning the initial payload included them
    /// and we should not modify them. Returns false if any IDs are missing,
    /// indicating we need to generate them via ensure_tool_ids().
    pub fn ids_present(&self) -> bool {
        let all_tool_messages_have_ids = self
            .messages
            .iter()
            .filter(|t| t.role == Self::tool_role())
            .all(|msg| Self::has_valid_id(&msg.tool_call_id));

        let all_tool_calls_have_ids = self
            .messages
            .iter()
            .filter(|msg| msg.role == Self::assistant_role())
            .filter_map(|msg| msg.tool_calls.as_ref())
            .flat_map(|calls| calls.iter())
            .all(|call| Self::has_valid_id(&call.id));

        all_tool_messages_have_ids && all_tool_calls_have_ids
    }

    /// Applies all necessary transformations for GitHub Copilot compatibility.
    ///
    /// This is the main entry point for preparing requests before sending to Copilot.
    /// It orchestrates two critical transformations:
    /// 1. Ensures tool IDs are present (required by OpenAI spec)
    /// 2. Duplicates tool messages as user messages (works around Copilot quirks)
    ///
    /// Call this method once on any request that contains tools before forwarding to Copilot.
    pub fn prepare_for_copilot(&mut self) {
        self.ensure_tool_ids();
        self.duplicate_tool_messages_as_user();
    }

    /// Generates and assigns IDs to tool-related messages when they are missing.
    /// This method only modifies the request if ids_present() returns false.
    ///
    /// It assigns:
    /// - tool_call_id to messages with role "tool" (indexed sequentially)
    /// - id to tool_calls in assistant messages (indexed sequentially)
    /// - name to tool messages (extracted from assistant's tool_calls)
    ///
    /// If the original request already had IDs, this method does nothing,
    /// preserving the client-provided identifiers.
    ///
    /// # Why This Is Necessary
    ///
    /// This normalization is required because different API providers have different requirements:
    /// - **Ollama API**: Does not include tool_call_id or id fields in its specification
    /// - **OpenAI API**: Requires these IDs for proper tool calling workflow
    /// - **GitHub Copilot**: Follows OpenAI's standard and expects IDs to be present
    ///
    /// When using frameworks like [Rig](https://github.com/0xPlaygrounds/rig) with its Ollama provider,
    /// the generated OpenAIChatRequest structs won't have these IDs. This proxy bridges
    /// that gap by auto-generating them before forwarding to GitHub Copilot.
    fn ensure_tool_ids(&mut self) {
        if !self.ids_present() {
            let assistant_tool_name = self
                .messages
                .iter()
                .filter(|message| message.role == Self::assistant_role())
                .flat_map(|message| match &message.tool_calls {
                    Some(tool_calls) => tool_calls.clone(),
                    _ => Vec::new(),
                })
                .map(|tool_call| tool_call.function.name)
                .collect::<Vec<String>>();

            self.messages
                .iter_mut()
                .filter(|message| message.role == Self::tool_role())
                .enumerate()
                .zip(assistant_tool_name.iter())
                .for_each(|((idx, message), tool_name)| {
                    message.name = Some(tool_name.to_string());
                    message.tool_call_id = Some(format!("{}", idx))
                });

            self.messages
                .iter_mut()
                .filter(|message| message.role == Self::assistant_role())
                .filter(|message| message.tool_calls.is_some())
                .for_each(|message| {
                    if let Some(ref mut tc) = message.tool_calls {
                        tc.iter_mut().enumerate().for_each(|(idx, tool_call)| {
                            tool_call.id = Some(format!("{}", idx));
                        })
                    }
                });
        }
    }

    /// Duplicates tool messages as user messages for GitHub Copilot compatibility.
    ///
    /// GitHub Copilot validates that `tool_calls` in assistant messages have corresponding
    /// `role: "tool"` messages with matching IDs. However, when `role: "tool"` messages are
    /// present, Copilot sometimes returns empty choices arrays (intermittent behavior).
    ///
    /// This method works around both constraints by:
    /// 1. Keeping the original `role: "tool"` messages in place (for validation)
    /// 2. Appending `role: "user"` message duplicates after the last tool message
    ///    (for the LLM to actually read and process)
    ///
    /// # Message Flow
    ///
    /// The method preserves the natural message ordering that Copilot expects:
    /// - `assistant` message with `tool_calls`
    /// - All corresponding `tool` messages (grouped together)
    /// - User message summaries (appended at the end)
    ///
    /// Original:
    /// ```json
    /// [
    ///   {"role": "assistant", "tool_calls": [{"id": "call_123", ...}]},
    ///   {"role": "tool", "tool_call_id": "call_123", "name": "get_weather", "content": "{\"temperature\": 72}"}
    /// ]
    /// ```
    ///
    /// After duplication:
    /// ```json
    /// [
    ///   {"role": "assistant", "tool_calls": [{"id": "call_123", ...}]},
    ///   {"role": "tool", "tool_call_id": "call_123", "name": "get_weather", "content": "{\"temperature\": 72}"},
    ///   {"role": "user", "content": "Tool 'get_weather' (call_123) returned: {\"temperature\": 72}"}
    /// ]
    /// ```
    ///
    /// This approach trades token consumption for reliability, ensuring Copilot both
    /// validates the tool calling chain AND consistently processes the results.
    fn duplicate_tool_messages_as_user(&mut self) {
        let mut user_duplicates = Vec::new();
        let mut last_tool_index = None;

        // Find all tool messages and create user message duplicates
        for (idx, message) in self.messages.iter().enumerate() {
            if message.role == Self::tool_role() {
                last_tool_index = Some(idx);

                let tool_name = message.name.as_deref().unwrap_or("unknown_tool");
                let tool_call_id = message.tool_call_id.as_deref().unwrap_or("unknown_id");
                let original_content = message.content.as_deref().unwrap_or("");

                // Create a user message with formatted tool result
                let user_message = OpenAIMessage {
                    role: "user".to_string(),
                    content: Some(format!(
                        "Tool '{}' ({}) returned: {}",
                        tool_name, tool_call_id, original_content
                    )),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                };

                user_duplicates.push(user_message);
            }
        }

        // Insert all user duplicates after the last tool message
        if let Some(insert_pos) = last_tool_index {
            // Insert in reverse order to maintain correct final ordering
            for user_msg in user_duplicates.into_iter().rev() {
                self.messages.insert(insert_pos + 1, user_msg);
            }
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
        request: Json<OpenAIChatRequest>,
    ) -> Result<Json<OpenAIChatResponse>, AppError> {
        let mut request = request.0;
        request.prepare_for_copilot();
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

    #[test]
    fn test_prepare_for_copilot_duplicates_tool_messages() {
        // Test that tool messages are duplicated as user messages appended after last tool
        let mut request = OpenAIChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                OpenAIMessage {
                    role: "user".to_string(),
                    content: Some("What's the weather?".to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                OpenAIMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: Some(vec![ToolCall {
                        id: Some("call_123".to_string()),
                        tool_type: "function".to_string(),
                        function: FunctionCall {
                            name: "get_weather".to_string(),
                            arguments: "{\"location\":\"SF\"}".to_string(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                },
                OpenAIMessage {
                    role: "tool".to_string(),
                    content: Some("{\"temperature\":72,\"condition\":\"sunny\"}".to_string()),
                    tool_calls: None,
                    tool_call_id: Some("call_123".to_string()),
                    name: Some("get_weather".to_string()),
                },
            ],
            temperature: None,
            max_tokens: None,
            stream: false,
            tools: None,
            tool_choice: None,
        };

        request.prepare_for_copilot();

        // Should now have 4 messages: original 3 + 1 duplicate user message
        assert_eq!(request.messages.len(), 4);

        // First two messages unchanged
        assert_eq!(request.messages[0].role, "user");
        assert_eq!(request.messages[1].role, "assistant");

        // Original tool message should still be there
        assert_eq!(request.messages[2].role, "tool");
        assert_eq!(
            request.messages[2].tool_call_id.as_deref(),
            Some("call_123")
        );
        assert_eq!(request.messages[2].name.as_deref(), Some("get_weather"));

        // New user message should be appended after the last tool message
        assert_eq!(request.messages[3].role, "user");
        assert_eq!(
            request.messages[3].content.as_ref().unwrap(),
            "Tool 'get_weather' (call_123) returned: {\"temperature\":72,\"condition\":\"sunny\"}"
        );
        assert!(request.messages[3].tool_call_id.is_none());
        assert!(request.messages[3].name.is_none());
    }

    #[test]
    fn test_prepare_for_copilot_handles_multiple_tools() {
        // Test duplication of multiple tool messages - all user duplicates appended after last tool
        let mut request = OpenAIChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                OpenAIMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: Some(vec![
                        ToolCall {
                            id: Some("call_1".to_string()),
                            tool_type: "function".to_string(),
                            function: FunctionCall {
                                name: "get_weather".to_string(),
                                arguments: "{}".to_string(),
                            },
                        },
                        ToolCall {
                            id: Some("call_2".to_string()),
                            tool_type: "function".to_string(),
                            function: FunctionCall {
                                name: "get_stock".to_string(),
                                arguments: "{}".to_string(),
                            },
                        },
                    ]),
                    tool_call_id: None,
                    name: None,
                },
                OpenAIMessage {
                    role: "tool".to_string(),
                    content: Some("weather data".to_string()),
                    tool_calls: None,
                    tool_call_id: Some("call_1".to_string()),
                    name: Some("get_weather".to_string()),
                },
                OpenAIMessage {
                    role: "tool".to_string(),
                    content: Some("stock data".to_string()),
                    tool_calls: None,
                    tool_call_id: Some("call_2".to_string()),
                    name: Some("get_stock".to_string()),
                },
            ],
            temperature: None,
            max_tokens: None,
            stream: false,
            tools: None,
            tool_choice: None,
        };

        request.prepare_for_copilot();

        // Should have 5 messages: 1 assistant + 2 tool + 2 user duplicates
        assert_eq!(request.messages.len(), 5);

        // Assistant message first
        assert_eq!(request.messages[0].role, "assistant");

        // Both tool messages kept in place
        assert_eq!(request.messages[1].role, "tool");
        assert_eq!(request.messages[1].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(request.messages[2].role, "tool");
        assert_eq!(request.messages[2].tool_call_id.as_deref(), Some("call_2"));

        // User duplicates appended after last tool message
        assert_eq!(request.messages[3].role, "user");
        assert_eq!(
            request.messages[3].content.as_ref().unwrap(),
            "Tool 'get_weather' (call_1) returned: weather data"
        );

        assert_eq!(request.messages[4].role, "user");
        assert_eq!(
            request.messages[4].content.as_ref().unwrap(),
            "Tool 'get_stock' (call_2) returned: stock data"
        );
    }

    #[test]
    fn test_prepare_for_copilot_preserves_non_tool_messages() {
        // Test that non-tool messages are not affected
        let mut request = OpenAIChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                OpenAIMessage {
                    role: "system".to_string(),
                    content: Some("You are helpful".to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                OpenAIMessage {
                    role: "user".to_string(),
                    content: Some("Hello".to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
            ],
            temperature: None,
            max_tokens: None,
            stream: false,
            tools: None,
            tool_choice: None,
        };

        request.prepare_for_copilot();

        // Should still have 2 messages, no duplicates
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, "system");
        assert_eq!(request.messages[1].role, "user");
    }

    #[test]
    fn test_prepare_for_copilot_handles_missing_fields() {
        // Test duplication when tool message has missing optional fields
        let mut request = OpenAIChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![OpenAIMessage {
                role: "tool".to_string(),
                content: Some("result".to_string()),
                tool_calls: None,
                tool_call_id: None, // Missing
                name: None,         // Missing
            }],
            temperature: None,
            max_tokens: None,
            stream: false,
            tools: None,
            tool_choice: None,
        };

        request.prepare_for_copilot();

        // Should have 2 messages now
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, "tool");
        assert_eq!(request.messages[1].role, "user");

        // User message should handle missing fields gracefully
        assert_eq!(
            request.messages[1].content.as_ref().unwrap(),
            "Tool 'unknown_tool' (unknown_id) returned: result"
        );
    }
}
