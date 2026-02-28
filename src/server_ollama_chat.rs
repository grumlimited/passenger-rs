use crate::copilot::CopilotChatRequest;
use crate::copilot::CopilotChatResponse;
use crate::openai::completion::models::OpenAIChatRequest;
use crate::server::{AppError, AppState, Server};
use crate::server_copilot::CopilotIntegration;
use axum::response::IntoResponse;
use axum::{Json, extract::State};
use futures_util::{StreamExt as _, TryStreamExt as _};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;
use tracing::log::{error, info, warn};

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
    pub id: String,
    pub function: OllamaFunction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub arguments: String,
}

pub(crate) trait OllamaChatEndpoint: CopilotIntegration {
    async fn ollama_chat(
        state: State<Arc<AppState>>,
        request: Json<OpenAIChatRequest>,
    ) -> Result<axum::response::Response, AppError>;
}

impl OllamaChatEndpoint for Server {
    async fn ollama_chat(
        State(state): State<Arc<AppState>>,
        request: Json<OpenAIChatRequest>,
    ) -> Result<axum::response::Response, AppError> {
        let mut request = request.0;

        // debug!(
        //     "original_openai_request:\n{}",
        //     serde_json::to_string_pretty(&request).unwrap()
        // );

        request.prepare_for_copilot();

        let is_stream = request.stream;

        // Get a valid Copilot token
        let token = Self::get_token(state.clone()).await?;

        // Transform OpenAI request to Copilot format
        let copilot_request: CopilotChatRequest = request.into();

        debug!(
            "copilot_request:\n{}",
            serde_json::to_string_pretty(&copilot_request).unwrap()
        );

        // Forward request to Copilot API
        let copilot_url = format!("{}/chat/completions", state.config.copilot.api_base_url);

        let response = Self::forward_prompt(state, token, copilot_url, &copilot_request).await?;

        let status = response.status();
        if !status.is_success() {
            return Err(Self::handle_errors(response).await.unwrap_err());
        }

        if is_stream {
            use axum::body::Body;
            use axum::http::header;

            let model = copilot_request.model.clone();

            let byte_stream = response.bytes_stream();

            // Each Copilot SSE chunk may carry one or more "data: <json>\n" lines.
            // We parse the OpenAI-format delta and re-emit as Ollama NDJSON chunks.
            // The final Copilot chunk is "data: [DONE]" — we emit the terminal
            // Ollama object (done: true) at that point.
            let ndjson_stream = byte_stream
                .map_err(|e: reqwest::Error| {
                    error!("Error reading streaming response from Copilot: {}", e);
                    std::io::Error::other(e.to_string())
                })
                .flat_map(move |result: Result<tokio_util::bytes::Bytes, std::io::Error>| {
                    let model = model.clone();
                    let lines: Vec<Result<tokio_util::bytes::Bytes, std::io::Error>> = match result {
                        Err(e) => vec![Err(e)],
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes).into_owned();
                            text.lines()
                                .filter_map(|line| match translate_sse_line(&model, line) {
                                    SseLineOutput::Line(s) => {
                                        Some(Ok(tokio_util::bytes::Bytes::from(s)))
                                    }
                                    SseLineOutput::Skip | SseLineOutput::Unexpected(_) => None,
                                })
                                .collect()
                        }
                    };
                    futures_util::stream::iter(lines)
                });

            info!("Streaming Ollama chat response");
            let body = Body::from_stream(ndjson_stream);
            Ok((
                [(
                    header::CONTENT_TYPE,
                    "application/x-ndjson",
                )],
                body,
            )
                .into_response())
        } else {
            let copilot_response: CopilotChatResponse = response.json().await.map_err(|e| {
                error!("Failed to parse Copilot response: {}", e);
                AppError::InternalServerError(format!("Failed to parse Copilot response: {}", e))
            })?;

            debug!(
                "copilot_response:\n{}",
                serde_json::to_string_pretty(&copilot_response).unwrap()
            );

            // Transform Copilot response to Ollama format
            let ollama_response = transform_to_ollama_response(&copilot_request, copilot_response)?;

            info!("Successfully processed Ollama chat request");

            Ok(Json(ollama_response).into_response())
        }
    }
}

/// Minimal structs to deserialize OpenAI-format SSE delta chunks from Copilot
#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIStreamDelta,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamDelta {
    #[serde(default)]
    content: Option<String>,
}

/// Result of translating a single Copilot SSE line into Ollama NDJSON output.
#[derive(Debug, PartialEq)]
pub(crate) enum SseLineOutput {
    /// A serialised, newline-terminated Ollama NDJSON line ready to write.
    Line(String),
    /// The line was empty or a comment — nothing to emit.
    Skip,
    /// The line was not a valid `data: …` SSE line (logged as a warning).
    Unexpected(String),
}

/// Translate one line of Copilot SSE output into the matching Ollama NDJSON
/// representation.
///
/// * `data: [DONE]`       → terminal `{ …, "done": true }` object
/// * `data: <json-chunk>` → intermediate `{ …, "done": false }` object
/// * empty / whitespace   → `SseLineOutput::Skip`
/// * anything else        → `SseLineOutput::Unexpected`
pub(crate) fn translate_sse_line(model: &str, line: &str) -> SseLineOutput {
    if let Some(payload) = line.strip_prefix("data: ") {
        if payload == "[DONE]" {
            let done_obj = OllamaChatResponse {
                model: model.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                message: OllamaMessage {
                    role: "assistant".to_string(),
                    content: String::new(),
                    thinking: None,
                    tool_calls: None,
                    images: None,
                },
                done: true,
                done_reason: Some("stop".to_string()),
                total_duration: None,
                load_duration: None,
                prompt_eval_count: None,
                prompt_eval_duration: None,
                eval_count: None,
                eval_duration: None,
            };
            let mut json = serde_json::to_string(&done_obj).expect("serialization cannot fail");
            json.push('\n');
            SseLineOutput::Line(json)
        } else {
            match serde_json::from_str::<OpenAIStreamChunk>(payload) {
                Ok(chunk) => {
                    let content = chunk
                        .choices
                        .first()
                        .and_then(|c| c.delta.content.clone())
                        .unwrap_or_default();
                    let chunk_obj = OllamaChatResponse {
                        model: model.to_string(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                        message: OllamaMessage {
                            role: "assistant".to_string(),
                            content,
                            thinking: None,
                            tool_calls: None,
                            images: None,
                        },
                        done: false,
                        done_reason: None,
                        total_duration: None,
                        load_duration: None,
                        prompt_eval_count: None,
                        prompt_eval_duration: None,
                        eval_count: None,
                        eval_duration: None,
                    };
                    let mut json =
                        serde_json::to_string(&chunk_obj).expect("serialization cannot fail");
                    json.push('\n');
                    SseLineOutput::Line(json)
                }
                Err(e) => {
                    warn!(
                        "Failed to parse Copilot SSE chunk: {} — {}",
                        e, payload
                    );
                    SseLineOutput::Unexpected(payload.to_string())
                }
            }
        }
    } else if line.trim().is_empty() {
        SseLineOutput::Skip
    } else {
        warn!("Unexpected SSE line from Copilot: {}", line);
        SseLineOutput::Unexpected(line.to_string())
    }
}

/// Transform CopilotChatResponse to OllamaChatResponse
fn transform_to_ollama_response(
    copilot_request: &CopilotChatRequest,
    copilot: CopilotChatResponse,
) -> Result<OllamaChatResponse, AppError> {
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

    let ollama_tool_calls = choice.message.tool_calls.clone().map(|tools| {
        tools
            .into_iter()
            .enumerate()
            .map(|(i, tool)| OllamaToolCall {
                id: tool.id.unwrap_or(format!("{}", i)),
                function: OllamaFunction {
                    name: tool.function.name.to_string(),
                    description: {
                        copilot_request
                            .tools
                            .clone()
                            .and_then(|request_tools| {
                                request_tools.into_iter().find(|request_tool| {
                                    request_tool.function.name == tool.function.name
                                })
                            })
                            .and_then(|request_tool| request_tool.function.description.clone())
                    },
                    arguments: tool.function.arguments.clone(),
                },
            })
            .collect()
    });

    Ok(OllamaChatResponse {
        model: copilot_request.model.clone(),
        created_at,
        message: OllamaMessage {
            role: choice.message.role.clone(),
            content: choice.message.content.clone().unwrap_or_default(),
            thinking: None,
            tool_calls: ollama_tool_calls,
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
    use crate::copilot::CopilotMessage;
    use crate::openai::completion::models::FunctionDefinition;
    use crate::openai::completion::models::{OpenAIChatRequest, Tool};
    use crate::server_chat_completion::{CopilotChoice, CopilotUsage};

    // -----------------------------------------------------------------------
    // translate_sse_line — streaming conversion tests
    // -----------------------------------------------------------------------

    fn parse_line(line: &str) -> OllamaChatResponse {
        match translate_sse_line("llama3", line) {
            SseLineOutput::Line(s) => {
                serde_json::from_str(s.trim_end_matches('\n')).expect("valid JSON")
            }
            other => panic!("expected SseLineOutput::Line, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_done_emits_terminal_object() {
        let result = translate_sse_line("my-model", "data: [DONE]");
        let SseLineOutput::Line(json) = result else {
            panic!("expected Line");
        };
        assert!(json.ends_with('\n'), "output must be newline-terminated");

        let obj: OllamaChatResponse = serde_json::from_str(json.trim_end_matches('\n')).unwrap();
        assert_eq!(obj.model, "my-model");
        assert!(obj.done, "done must be true for [DONE]");
        assert_eq!(obj.done_reason, Some("stop".to_string()));
        assert_eq!(obj.message.content, "");
        assert_eq!(obj.message.role, "assistant");
    }

    #[test]
    fn test_sse_content_chunk_emits_intermediate_object() {
        let payload = r#"{"id":"x","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":null}]}"#;
        let line = format!("data: {}", payload);

        let obj = parse_line(&line);
        assert_eq!(obj.model, "llama3");
        assert!(!obj.done, "done must be false for a content chunk");
        assert!(obj.done_reason.is_none());
        assert_eq!(obj.message.content, "Hello");
        assert_eq!(obj.message.role, "assistant");
    }

    #[test]
    fn test_sse_chunk_with_null_content_defaults_to_empty_string() {
        // First chunk from Copilot often only carries `role`, with content: null
        let payload = r#"{"id":"x","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}"#;
        let line = format!("data: {}", payload);

        let obj = parse_line(&line);
        assert!(!obj.done);
        assert_eq!(obj.message.content, "", "null content should default to empty string");
    }

    #[test]
    fn test_sse_chunk_with_empty_choices_defaults_to_empty_string() {
        let payload = r#"{"id":"x","object":"chat.completion.chunk","created":1,"model":"m","choices":[]}"#;
        let line = format!("data: {}", payload);

        let obj = parse_line(&line);
        assert!(!obj.done);
        assert_eq!(obj.message.content, "");
    }

    #[test]
    fn test_sse_output_is_newline_terminated() {
        let payload = r#"{"id":"x","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#;
        let line = format!("data: {}", payload);

        let SseLineOutput::Line(s) = translate_sse_line("model", &line) else {
            panic!("expected Line");
        };
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn test_sse_empty_line_is_skipped() {
        assert_eq!(translate_sse_line("m", ""), SseLineOutput::Skip);
        assert_eq!(translate_sse_line("m", "   "), SseLineOutput::Skip);
        assert_eq!(translate_sse_line("m", "\t"), SseLineOutput::Skip);
    }

    #[test]
    fn test_sse_non_data_line_is_unexpected() {
        match translate_sse_line("m", "event: ping") {
            SseLineOutput::Unexpected(_) => {}
            other => panic!("expected Unexpected, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_malformed_json_is_unexpected() {
        match translate_sse_line("m", "data: {not valid json}") {
            SseLineOutput::Unexpected(_) => {}
            other => panic!("expected Unexpected, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_model_name_is_propagated() {
        let payload = r#"{"id":"x","object":"chat.completion.chunk","created":1,"model":"ignored","choices":[{"index":0,"delta":{"content":"x"},"finish_reason":null}]}"#;
        let line = format!("data: {}", payload);
        let obj = parse_line(&line);
        // model comes from the argument, not from the Copilot payload
        assert_eq!(obj.model, "llama3");
    }

    // -----------------------------------------------------------------------
    // Existing non-streaming tests (unchanged)
    // -----------------------------------------------------------------------

    #[test]
    fn test_openai_chat_request_multiple_tools_normalize() {
        let json = include_str!("resources/rig_ollama_request_multiple_tools.json");
        let mut json: OpenAIChatRequest = serde_json::from_str(json).unwrap();

        assert!(
            json.messages
                .iter()
                .filter(|m| m.role == "tool")
                .all(|m| m.tool_call_id.is_none())
        );

        json.prepare_for_copilot();

        assert!(
            json.messages
                .iter()
                .filter(|m| m.role == "tool")
                .all(|m| m.name.is_some() && m.tool_call_id.is_some())
        );
    }

    #[test]
    fn test_openai_chat_request_normalize() {
        let json = include_str!("resources/rig_ollama_request.json");
        let mut json: OpenAIChatRequest = serde_json::from_str(json).unwrap();

        assert!(
            json.messages
                .iter()
                .filter(|m| m.role == "tool")
                .all(|m| m.tool_call_id.is_none())
        );

        json.prepare_for_copilot();

        assert!(
            json.messages
                .iter()
                .filter(|m| m.role == "tool")
                .all(|m| m.name.is_some() && m.tool_call_id.is_some())
        );
    }

    #[test]
    fn test_transform_to_ollama_response() {
        let copilot_request = CopilotChatRequest {
            messages: vec![CopilotMessage {
                role: "tool".to_string(),
                content: None,
                padding: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            model: "gpt-4".to_string(),
            temperature: None,
            max_tokens: None,
            stream: None,
            tools: Some(vec![Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "function_name".to_string(),
                    description: Some("Description".to_string()),
                    parameters: serde_json::Value::Object(serde_json::Map::new()),
                },
            }]),
            tool_choice: None,
        };

        let copilot_response = CopilotChatResponse {
            id: "test-id".to_string(),
            created: Some(1699334516),
            model: "gpt-4".to_string(),
            choices: vec![CopilotChoice {
                index: Some(0),
                message: CopilotMessage {
                    role: "assistant".to_string(),
                    content: Some("Hello, World!".to_string()),
                    padding: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                finish_reason: "stop".to_string(),
            }],
            usage: Some(CopilotUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        };

        let result = transform_to_ollama_response(&copilot_request, copilot_response);
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
        let copilot_request = CopilotChatRequest {
            messages: vec![CopilotMessage {
                role: "tool".to_string(),
                content: None,
                padding: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            model: "model".to_string(),
            temperature: None,
            max_tokens: None,
            stream: None,
            tools: Some(vec![Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "function_name".to_string(),
                    description: Some("Description".to_string()),
                    parameters: serde_json::Value::Object(serde_json::Map::new()),
                },
            }]),
            tool_choice: None,
        };

        let copilot_response = CopilotChatResponse {
            id: "test-id".to_string(),
            created: None,
            model: "gpt-4".to_string(),
            choices: vec![CopilotChoice {
                index: Some(0),
                message: CopilotMessage {
                    role: "assistant".to_string(),
                    content: Some("Test".to_string()),
                    padding: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                finish_reason: "length".to_string(),
            }],
            usage: None,
        };

        let result = transform_to_ollama_response(&copilot_request, copilot_response);
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
