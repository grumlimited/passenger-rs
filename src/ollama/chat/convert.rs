//! Converters between Ollama `/api/chat` types and Copilot `/responses` types.
//!
//! # OllamaChatRequest → CopilotResponsesRequest
//! - `messages` → `input` items (same pattern as chat_completions converter)
//! - `options.{temperature,top_p,num_predict}` → top-level Copilot fields
//! - `tools` → Copilot `tools`
//! - Ollama tool-call `arguments` is a JSON object; Copilot expects a JSON string
//!   so we serialise it with `serde_json::to_string`.
//!
//! # CopilotResponsesResponse → OllamaChatResponse
//! - `MessageOutputItem` text parts → `message.content`
//! - `FunctionCallOutputItem` → `message.tool_calls` (arguments string → Value)
//! - `ReasoningOutputItem` summary → `message.thinking`
//! - `created_at` (u64 epoch seconds) → ISO 8601 string via chrono
//! - Usage token counts stored as timing approximations (no real nanosecond
//!   timings available from Copilot; fields are omitted / set to None).

use chrono::{DateTime, Utc};

use crate::copilot::responses::request::{
    AssistantContentPart, AssistantMessage as CopilotAssistantMessage, CopilotResponsesRequest,
    FunctionCallItem, FunctionCallItemKind, FunctionTool, InputItem,
    SystemMessage as CopilotSystemMessage, Tool, ToolChoice, ToolChoiceFunction, ToolChoiceMode,
    ToolResult, ToolResultKind, UserContent as CopilotUserContent,
    UserMessage as CopilotUserMessage,
};
use crate::copilot::responses::response::{CopilotResponsesResponse, OutputItem};
use crate::ollama::chat::request::{
    OllamaChatRequest, OllamaFunctionTool, OllamaMessage, OllamaRole, OllamaTool, OllamaToolCall,
    OllamaToolCallFunction,
};
use crate::ollama::chat::response::OllamaChatResponse;

// ---------------------------------------------------------------------------
// OllamaMessage → Copilot InputItem(s)
// ---------------------------------------------------------------------------

fn convert_ollama_message(msg: OllamaMessage) -> Vec<InputItem> {
    match msg.role {
        OllamaRole::System => {
            vec![InputItem::SystemMessage(CopilotSystemMessage {
                content: msg.content,
            })]
        }

        OllamaRole::User => {
            vec![InputItem::UserMessage(CopilotUserMessage {
                content: CopilotUserContent::Text(msg.content),
            })]
        }

        OllamaRole::Assistant => {
            let mut items: Vec<InputItem> = vec![];

            if !msg.content.is_empty() {
                items.push(InputItem::AssistantMessage(CopilotAssistantMessage {
                    content: vec![AssistantContentPart::OutputText { text: msg.content }],
                    id: None,
                }));
            }

            // Tool calls become separate FunctionCallItem input items.
            // Ollama arguments is a Value (object); Copilot expects a JSON string.
            for tc in msg.tool_calls.unwrap_or_default() {
                let arguments = serde_json::to_string(&tc.function.arguments)
                    .unwrap_or_else(|_| "{}".to_string());
                // Ollama has no call id; synthesise one from the function name.
                let id = format!("call_{}", tc.function.name);
                items.push(InputItem::FunctionCall(FunctionCallItem {
                    kind: FunctionCallItemKind::FunctionCall,
                    id: id.clone(),
                    call_id: id,
                    name: tc.function.name,
                    arguments,
                }));
            }

            if items.is_empty() {
                items.push(InputItem::AssistantMessage(CopilotAssistantMessage {
                    content: vec![],
                    id: None,
                }));
            }

            items
        }

        OllamaRole::Tool => {
            // Ollama tool messages carry `tool_name`; Copilot needs a `call_id`.
            // We reconstruct the same synthetic id used when the call was emitted.
            let call_id = msg
                .tool_name
                .as_deref()
                .map(|n| format!("call_{n}"))
                .unwrap_or_else(|| "call_unknown".to_string());

            vec![InputItem::ToolResult(ToolResult {
                kind: ToolResultKind::FunctionCallOutput,
                call_id,
                output: msg.content,
            })]
        }
    }
}

// ---------------------------------------------------------------------------
// OllamaTool → Copilot Tool
// ---------------------------------------------------------------------------

fn convert_ollama_tool(tool: OllamaTool) -> Tool {
    match tool {
        OllamaTool::Function(OllamaFunctionTool {
            name,
            description,
            parameters,
        }) => Tool::Function(FunctionTool {
            name,
            description,
            parameters,
            strict: None,
        }),
    }
}

// ---------------------------------------------------------------------------
// OllamaChatRequest → CopilotResponsesRequest
// ---------------------------------------------------------------------------

impl From<OllamaChatRequest> for CopilotResponsesRequest {
    fn from(req: OllamaChatRequest) -> Self {
        let input: Vec<InputItem> = req
            .messages
            .into_iter()
            .flat_map(convert_ollama_message)
            .collect();

        let tools = req
            .tools
            .map(|ts| ts.into_iter().map(convert_ollama_tool).collect());

        // Promote options fields to top-level Copilot parameters.
        let temperature = req.options.as_ref().and_then(|o| o.temperature);
        let top_p = req.options.as_ref().and_then(|o| o.top_p);
        let max_output_tokens = req
            .options
            .as_ref()
            .and_then(|o| o.num_predict)
            .and_then(|n| if n > 0 { Some(n as u32) } else { None });

        CopilotResponsesRequest {
            model: req.model,
            input,
            stream: req.stream,
            temperature,
            top_p,
            max_output_tokens,
            tools,
            tool_choice: None,
            instructions: None,
            store: None,
            previous_response_id: None,
            reasoning: None,
            truncation: None,
            text: None,
            metadata: None,
            user: None,
            service_tier: None,
            parallel_tool_calls: None,
            max_tool_calls: None,
            prompt_cache_key: None,
            safety_identifier: None,
        }
    }
}

// ---------------------------------------------------------------------------
// CopilotResponsesResponse → OllamaChatResponse
// ---------------------------------------------------------------------------

impl From<CopilotResponsesResponse> for OllamaChatResponse {
    fn from(resp: CopilotResponsesResponse) -> Self {
        let mut text_parts: Vec<String> = vec![];
        let mut tool_calls: Vec<OllamaToolCall> = vec![];
        let mut thinking_parts: Vec<String> = vec![];

        for item in resp.output {
            match item {
                OutputItem::Message(msg) => {
                    for part in msg.content {
                        text_parts.push(part.text);
                    }
                }
                OutputItem::FunctionCall(fc) => {
                    // Copilot arguments is a JSON string; Ollama expects a Value.
                    let arguments: serde_json::Value = serde_json::from_str(&fc.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    tool_calls.push(OllamaToolCall {
                        function: OllamaToolCallFunction {
                            name: fc.name,
                            arguments,
                        },
                    });
                }
                OutputItem::Reasoning(r) => {
                    for part in r.summary {
                        thinking_parts.push(part.text);
                    }
                }
                _ => {}
            }
        }

        let content = text_parts.join("");
        let thinking = if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join(""))
        };

        let done_reason = Some("stop".to_string());

        // Format created_at as ISO 8601.
        let created_at = DateTime::<Utc>::from_timestamp(resp.created_at as i64, 0)
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

        let message = OllamaMessage {
            role: OllamaRole::Assistant,
            content,
            images: None,
            thinking,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_name: None,
        };

        OllamaChatResponse {
            model: resp.model,
            created_at,
            message,
            done: true,
            done_reason,
            // Copilot gives us token counts but not real nanosecond timings.
            // We leave all duration fields absent; clients that need them will
            // see None / missing, which is valid per the Ollama spec.
            total_duration: None,
            load_duration: None,
            prompt_eval_count: Some(resp.usage.input_tokens),
            prompt_eval_duration: None,
            eval_count: Some(resp.usage.output_tokens),
            eval_duration: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copilot::responses::request::InputItem;
    use crate::copilot::responses::response::{
        FunctionCallOutputItem, MessageOutputItem, OutputItem, OutputTextKind, OutputTextPart,
        ReasoningOutputItem, SummaryTextKind, SummaryTextPart, Usage,
    };
    use crate::ollama::chat::request::{
        OllamaFunctionTool, OllamaMessage, OllamaOptions, OllamaRole, OllamaTool,
    };
    use serde_json::json;

    fn minimal_request(msgs: Vec<OllamaMessage>) -> OllamaChatRequest {
        OllamaChatRequest {
            model: "llama3.2".to_string(),
            messages: msgs,
            tools: None,
            think: None,
            format: None,
            options: None,
            stream: None,
            keep_alive: None,
        }
    }

    fn copilot_text_response(text: &str) -> CopilotResponsesResponse {
        CopilotResponsesResponse {
            id: "resp-1".to_string(),
            created_at: 1700000000,
            model: "llama3.2".to_string(),
            output: vec![OutputItem::Message(MessageOutputItem {
                id: "msg-1".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputTextPart {
                    kind: OutputTextKind::OutputText,
                    text: text.to_string(),
                    logprobs: None,
                    annotations: vec![],
                }],
            })],
            usage: Usage {
                input_tokens: 10,
                input_tokens_details: None,
                output_tokens: 5,
                output_tokens_details: None,
            },
            error: None,
            service_tier: None,
            incomplete_details: None,
        }
    }

    // --- OllamaChatRequest → CopilotResponsesRequest ---

    #[test]
    fn test_system_message_converts() {
        let req = minimal_request(vec![OllamaMessage {
            role: OllamaRole::System,
            content: "Be helpful.".to_string(),
            images: None,
            thinking: None,
            tool_calls: None,
            tool_name: None,
        }]);
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.input.len(), 1);
        match &copilot.input[0] {
            InputItem::SystemMessage(s) => assert_eq!(s.content, "Be helpful."),
            other => panic!("expected SystemMessage, got {:?}", other),
        }
    }

    #[test]
    fn test_user_message_converts() {
        let req = minimal_request(vec![OllamaMessage {
            role: OllamaRole::User,
            content: "Hello".to_string(),
            images: None,
            thinking: None,
            tool_calls: None,
            tool_name: None,
        }]);
        let copilot: CopilotResponsesRequest = req.into();
        match &copilot.input[0] {
            InputItem::UserMessage(u) => match &u.content {
                CopilotUserContent::Text(t) => assert_eq!(t, "Hello"),
                _ => panic!("expected text"),
            },
            other => panic!("expected UserMessage, got {:?}", other),
        }
    }

    #[test]
    fn test_assistant_text_message_converts() {
        let req = minimal_request(vec![OllamaMessage {
            role: OllamaRole::Assistant,
            content: "Sure!".to_string(),
            images: None,
            thinking: None,
            tool_calls: None,
            tool_name: None,
        }]);
        let copilot: CopilotResponsesRequest = req.into();
        match &copilot.input[0] {
            InputItem::AssistantMessage(a) => match &a.content[0] {
                AssistantContentPart::OutputText { text } => assert_eq!(text, "Sure!"),
                _ => panic!("expected OutputText"),
            },
            other => panic!("expected AssistantMessage, got {:?}", other),
        }
    }

    #[test]
    fn test_assistant_tool_call_becomes_function_call_item() {
        use crate::ollama::chat::request::{OllamaToolCall, OllamaToolCallFunction};
        let req = minimal_request(vec![OllamaMessage {
            role: OllamaRole::Assistant,
            content: "".to_string(),
            images: None,
            thinking: None,
            tool_calls: Some(vec![OllamaToolCall {
                function: OllamaToolCallFunction {
                    name: "get_weather".to_string(),
                    arguments: json!({ "city": "Paris" }),
                },
            }]),
            tool_name: None,
        }]);
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.input.len(), 1);
        match &copilot.input[0] {
            InputItem::FunctionCall(fc) => {
                assert_eq!(fc.name, "get_weather");
                assert_eq!(fc.call_id, "call_get_weather");
                // Arguments must be a JSON string (not an object)
                assert_eq!(fc.arguments, r#"{"city":"Paris"}"#);
            }
            other => panic!("expected FunctionCall, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_message_converts() {
        let req = minimal_request(vec![OllamaMessage {
            role: OllamaRole::Tool,
            content: "22".to_string(),
            images: None,
            thinking: None,
            tool_calls: None,
            tool_name: Some("get_weather".to_string()),
        }]);
        let copilot: CopilotResponsesRequest = req.into();
        match &copilot.input[0] {
            InputItem::ToolResult(tr) => {
                assert_eq!(tr.call_id, "call_get_weather");
                assert_eq!(tr.output, "22");
            }
            other => panic!("expected ToolResult, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_message_without_tool_name_uses_fallback() {
        let req = minimal_request(vec![OllamaMessage {
            role: OllamaRole::Tool,
            content: "result".to_string(),
            images: None,
            thinking: None,
            tool_calls: None,
            tool_name: None,
        }]);
        let copilot: CopilotResponsesRequest = req.into();
        match &copilot.input[0] {
            InputItem::ToolResult(tr) => assert_eq!(tr.call_id, "call_unknown"),
            other => panic!("expected ToolResult, got {:?}", other),
        }
    }

    #[test]
    fn test_options_temperature_and_top_p_promoted() {
        let mut req = minimal_request(vec![]);
        req.options = Some(OllamaOptions {
            temperature: Some(0.8),
            top_p: Some(0.9),
            num_predict: Some(200),
            ..Default::default()
        });
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.temperature, Some(0.8));
        assert_eq!(copilot.top_p, Some(0.9));
        assert_eq!(copilot.max_output_tokens, Some(200));
    }

    #[test]
    fn test_negative_num_predict_not_mapped() {
        // num_predict = -1 means infinite; must not set max_output_tokens
        let mut req = minimal_request(vec![]);
        req.options = Some(OllamaOptions {
            num_predict: Some(-1),
            ..Default::default()
        });
        let copilot: CopilotResponsesRequest = req.into();
        assert!(copilot.max_output_tokens.is_none());
    }

    #[test]
    fn test_function_tool_converts() {
        let mut req = minimal_request(vec![]);
        req.tools = Some(vec![OllamaTool::Function(OllamaFunctionTool {
            name: "get_weather".to_string(),
            description: Some("Get weather".to_string()),
            parameters: json!({ "type": "object" }),
        })]);
        let copilot: CopilotResponsesRequest = req.into();
        let tools = copilot.tools.unwrap();
        assert_eq!(tools.len(), 1);
        match &tools[0] {
            Tool::Function(f) => {
                assert_eq!(f.name, "get_weather");
                assert_eq!(f.description.as_deref(), Some("Get weather"));
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn test_stream_flag_preserved() {
        let mut req = minimal_request(vec![]);
        req.stream = Some(false);
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.stream, Some(false));
    }

    // --- CopilotResponsesResponse → OllamaChatResponse ---

    #[test]
    fn test_text_response_converts() {
        let resp = copilot_text_response("Hello!");
        let ollama: OllamaChatResponse = resp.into();
        assert_eq!(ollama.message.content, "Hello!");
        assert_eq!(ollama.message.role, OllamaRole::Assistant);
        assert!(ollama.done);
        assert_eq!(ollama.done_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_model_preserved() {
        let resp = copilot_text_response("Hi");
        let ollama: OllamaChatResponse = resp.into();
        assert_eq!(ollama.model, "llama3.2");
    }

    #[test]
    fn test_created_at_formatted_as_iso8601() {
        let resp = copilot_text_response("Hi");
        let ollama: OllamaChatResponse = resp.into();
        // epoch 1700000000 = 2023-11-14T22:13:20Z
        assert_eq!(ollama.created_at, "2023-11-14T22:13:20Z");
    }

    #[test]
    fn test_usage_token_counts_preserved() {
        let resp = copilot_text_response("Hi");
        let ollama: OllamaChatResponse = resp.into();
        assert_eq!(ollama.prompt_eval_count, Some(10));
        assert_eq!(ollama.eval_count, Some(5));
    }

    #[test]
    fn test_function_call_output_maps_to_tool_calls() {
        let resp = CopilotResponsesResponse {
            id: "resp-2".to_string(),
            created_at: 1700000000,
            model: "llama3.2".to_string(),
            output: vec![OutputItem::FunctionCall(FunctionCallOutputItem {
                id: "item-1".to_string(),
                call_id: "call_get_weather".to_string(),
                name: "get_weather".to_string(),
                arguments: r#"{"city":"Tokyo"}"#.to_string(),
            })],
            usage: Usage {
                input_tokens: 8,
                input_tokens_details: None,
                output_tokens: 12,
                output_tokens_details: None,
            },
            error: None,
            service_tier: None,
            incomplete_details: None,
        };
        let ollama: OllamaChatResponse = resp.into();
        let tc = &ollama.message.tool_calls.as_ref().unwrap()[0];
        assert_eq!(tc.function.name, "get_weather");
        // arguments must be a JSON object (Value), not a string
        assert_eq!(tc.function.arguments["city"], "Tokyo");
    }

    #[test]
    fn test_reasoning_output_maps_to_thinking() {
        let resp = CopilotResponsesResponse {
            id: "resp-3".to_string(),
            created_at: 1700000000,
            model: "llama3.2".to_string(),
            output: vec![
                OutputItem::Reasoning(ReasoningOutputItem {
                    id: "rsn-1".to_string(),
                    encrypted_content: None,
                    summary: vec![SummaryTextPart {
                        kind: SummaryTextKind::SummaryText,
                        text: "step by step...".to_string(),
                    }],
                }),
                OutputItem::Message(MessageOutputItem {
                    id: "msg-1".to_string(),
                    role: "assistant".to_string(),
                    content: vec![OutputTextPart {
                        kind: OutputTextKind::OutputText,
                        text: "42".to_string(),
                        logprobs: None,
                        annotations: vec![],
                    }],
                }),
            ],
            usage: Usage {
                input_tokens: 5,
                input_tokens_details: None,
                output_tokens: 3,
                output_tokens_details: None,
            },
            error: None,
            service_tier: None,
            incomplete_details: None,
        };
        let ollama: OllamaChatResponse = resp.into();
        assert_eq!(ollama.message.content, "42");
        assert_eq!(ollama.message.thinking.as_deref(), Some("step by step..."));
    }

    #[test]
    fn test_multi_part_text_joined() {
        let resp = CopilotResponsesResponse {
            id: "resp-4".to_string(),
            created_at: 1700000000,
            model: "llama3.2".to_string(),
            output: vec![OutputItem::Message(MessageOutputItem {
                id: "msg-1".to_string(),
                role: "assistant".to_string(),
                content: vec![
                    OutputTextPart {
                        kind: OutputTextKind::OutputText,
                        text: "Hello".to_string(),
                        logprobs: None,
                        annotations: vec![],
                    },
                    OutputTextPart {
                        kind: OutputTextKind::OutputText,
                        text: ", world!".to_string(),
                        logprobs: None,
                        annotations: vec![],
                    },
                ],
            })],
            usage: Usage {
                input_tokens: 5,
                input_tokens_details: None,
                output_tokens: 5,
                output_tokens_details: None,
            },
            error: None,
            service_tier: None,
            incomplete_details: None,
        };
        let ollama: OllamaChatResponse = resp.into();
        assert_eq!(ollama.message.content, "Hello, world!");
    }

    #[test]
    fn test_duration_fields_absent() {
        let resp = copilot_text_response("Hi");
        let ollama: OllamaChatResponse = resp.into();
        assert!(ollama.total_duration.is_none());
        assert!(ollama.load_duration.is_none());
        assert!(ollama.prompt_eval_duration.is_none());
        assert!(ollama.eval_duration.is_none());
    }
}
