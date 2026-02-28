use crate::copilot::CopilotMessage;
use crate::copilot::{CopilotChatRequest, CopilotChatResponse};
use crate::openai::completion::models::{
    OpenAIChatRequest, OpenAIChatResponse, OpenAIChoice, OpenAIMessage, OpenAIUsage,
};
use crate::server::copilot::CopilotIntegration;
use crate::server::{AppError, AppState, Server};
use axum::response::IntoResponse;
use axum::{Json, extract::State};
use futures_util::{StreamExt as _, TryStreamExt as _};
use serde::{Deserialize, Serialize};
use std::io::Error;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::log::{error, info, warn};

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

pub(crate) trait CoPilotChatCompletions: CopilotIntegration {
    async fn chat_completions(
        state: State<Arc<AppState>>,
        request: Json<OpenAIChatRequest>,
    ) -> Result<axum::response::Response, AppError>;
}

impl CoPilotChatCompletions for Server {
    async fn chat_completions(
        State(state): State<Arc<AppState>>,
        request: Json<OpenAIChatRequest>,
    ) -> Result<axum::response::Response, AppError> {
        let mut request = request.0;

        request.prepare_for_copilot();
        info!(
            "Received chat completion request for model: {} (stream={})",
            request.model, request.stream
        );

        let is_stream = request.stream;

        // Get a valid Copilot token
        let token = Self::get_token(state.clone()).await?;

        // Transform OpenAI request to Copilot format
        let copilot_request: CopilotChatRequest = request.into();

        // Forward request to Copilot API
        let copilot_url = format!("{}/chat/completions", state.config.copilot.api_base_url);

        let response = Self::forward_prompt(state, token, copilot_url, &copilot_request).await?;

        let status = response.status();
        if !status.is_success() {
            return Self::handle_errors(response).await;
        }

        if is_stream {
            use axum::response::sse::{Event, Sse};

            let byte_stream = response.bytes_stream();

            // Each chunk from Copilot is raw SSE text, potentially containing
            // one or more lines of the form "data: <json>\n\n".
            // We split on newlines, strip the "data: " prefix from each line,
            // and re-emit the bare JSON payload as an axum SSE Event.
            let sse_stream = byte_stream
                .map_err(|e: reqwest::Error| {
                    error!("Error reading streaming response from Copilot: {}", e);
                    Error::other(e.to_string())
                })
                .flat_map(|result| {
                    let events: Vec<Result<Event, Error>> = match result {
                        Err(e) => vec![Err(e)],
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes).into_owned();
                            text.lines()
                                .filter_map(|line| match translate_sse_line(line) {
                                    ChatSseLineOutput::Data(payload) => {
                                        Some(Ok(Event::default().data(payload)))
                                    }
                                    ChatSseLineOutput::Skip => None,
                                    ChatSseLineOutput::Unexpected(raw) => {
                                        warn!("Unexpected SSE line from Copilot: {}", raw);
                                        None
                                    }
                                })
                                .collect()
                        }
                    };
                    futures_util::stream::iter(events)
                });

            info!("Streaming chat completion response");
            Ok(Sse::new(sse_stream).into_response())
        } else {
            // Non-streaming path: buffer the full response and return JSON.
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
            Ok(Json(openai_response).into_response())
        }
    }
}

/// Result of processing a single Copilot SSE line for the OpenAI chat completions endpoint.
#[derive(Debug, PartialEq)]
pub(crate) enum ChatSseLineOutput {
    /// A bare payload string (the part after `"data: "`) ready to emit as an SSE data event.
    Data(String),
    /// The line was empty or whitespace-only — nothing to emit.
    Skip,
    /// The line did not start with `"data: "` and was not empty (logged as a warning by the caller).
    Unexpected(String),
}

/// Translate one line of Copilot SSE output for the OpenAI chat completions passthrough.
///
/// * `data: <payload>` → `ChatSseLineOutput::Data(payload)`
/// * empty / whitespace → `ChatSseLineOutput::Skip`
/// * anything else     → `ChatSseLineOutput::Unexpected(line)`
pub(crate) fn translate_sse_line(line: &str) -> ChatSseLineOutput {
    if let Some(payload) = line.strip_prefix("data: ") {
        ChatSseLineOutput::Data(payload.to_string())
    } else if line.trim().is_empty() {
        ChatSseLineOutput::Skip
    } else {
        ChatSseLineOutput::Unexpected(line.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::completion::models::{FunctionCall, ToolCall};

    // translate_sse_line tests

    #[test]
    fn test_sse_data_line_returns_payload() {
        let result = translate_sse_line("data: {\"id\":\"1\"}");
        assert_eq!(
            result,
            ChatSseLineOutput::Data("{\"id\":\"1\"}".to_string())
        );
    }

    #[test]
    fn test_sse_done_line_returns_payload() {
        let result = translate_sse_line("data: [DONE]");
        assert_eq!(result, ChatSseLineOutput::Data("[DONE]".to_string()));
    }

    #[test]
    fn test_sse_empty_line_is_skipped() {
        assert_eq!(translate_sse_line(""), ChatSseLineOutput::Skip);
        assert_eq!(translate_sse_line("   "), ChatSseLineOutput::Skip);
        assert_eq!(translate_sse_line("\t"), ChatSseLineOutput::Skip);
    }

    #[test]
    fn test_sse_non_data_line_is_unexpected() {
        match translate_sse_line("event: ping") {
            ChatSseLineOutput::Unexpected(raw) => assert_eq!(raw, "event: ping"),
            other => panic!("expected Unexpected, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_data_prefix_only_returns_empty_payload() {
        // "data: " with nothing after the space is a valid (empty) payload
        let result = translate_sse_line("data: ");
        assert_eq!(result, ChatSseLineOutput::Data(String::new()));
    }

    #[test]
    fn test_parse_copilot_response_without_created() {
        // Test parsing a Copilot response without the optional 'created' field
        let json = include_str!("../../resources/chat_completions_response.json");
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
    #[ignore = "duplicate_tool_messages_as_user is disabled; Copilot intermittently returns empty choices with role:tool messages"]
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
    #[ignore = "duplicate_tool_messages_as_user is disabled; Copilot intermittently returns empty choices with role:tool messages"]
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
    #[ignore = "duplicate_tool_messages_as_user is disabled; Copilot intermittently returns empty choices with role:tool messages"]
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
